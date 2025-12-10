use crate::{DataStorage, FrameStorage, StorageError, DATABASE};
use bytes::{BufMut, BytesMut};
use cortado::CortadoAffine;
use futures_util::TryStreamExt;
use mongodb::{
    bson::doc,
    change_stream::{event::ChangeStreamEvent, ChangeStream},
    options::IndexOptions,
    ClientSession, Collection, IndexModel,
};
use prost::Message;
use tracing::debug;
use types::protos::group_operation::Operation;
use types::protos::Frame;
use types::utils::{decode_aggregated_change, decode_branch_change, ArtUpdate};
use types::{utils, FrameRecord};
use uuid::Uuid;
use zrt_art::changes::branch_change::BranchChange;

pub const GROUP_COLLECTION_NAME: &str = "group";
pub const OUTBOX_COLLECTION_NAME: &str = "messages_outbox";

pub struct MongoFramesStorage {
    pub messages_collection: Collection<FrameRecord>,
    pub messages_outbox_collection: Collection<FrameRecord>,
    pub chat_id: Uuid,
}

impl MongoFramesStorage {
    pub async fn new(chat_id: &Uuid) -> Result<Self, StorageError> {
        let db = DATABASE
            .get()
            .ok_or_else(|| StorageError::DatabaseRetrieval)?;

        let messages_collection_name = format!("{GROUP_COLLECTION_NAME}/{chat_id}");
        let messages_collection = db.collection(&messages_collection_name);

        let messages_outbox_collection = db.collection(OUTBOX_COLLECTION_NAME);

        let messages_index_model = IndexModel::builder()
            .keys(doc! { "sequence_number": -1})
            .options(IndexOptions::builder().build())
            .build();
        messages_collection
            .create_index(messages_index_model)
            .await?;

        Ok(Self {
            messages_collection,
            messages_outbox_collection,
            chat_id: *chat_id,
        })
    }
}

#[async_trait::async_trait]
impl FrameStorage for MongoFramesStorage {
    async fn stream_messages(
        &self,
    ) -> Result<ChangeStream<ChangeStreamEvent<FrameRecord>>, StorageError> {
        let change_stream = self.messages_collection.watch().await?;

        Ok(change_stream)
    }

    async fn next_sequence_number(&self, session: &mut ClientSession) -> Result<u64, StorageError> {
        let message_collection = &self.messages_collection;

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .session(&mut *session)
            .await?;

        let next_sequence_number = match cursor.next(&mut *session).await {
            Some(result) => result?.sequence_number + 1,
            None => 0,
        };

        Ok(next_sequence_number)
    }

    async fn store_message(
        &self,
        content: Vec<u8>,
        epoch: i64,
        outbox_only: bool,
        session: &mut ClientSession,
    ) -> Result<(), StorageError> {
        let message_collection = &self.messages_collection;

        let next_sequence_number = self.next_sequence_number(&mut *session).await?;
        let message = FrameRecord::new(
            content.clone(),
            next_sequence_number.clone(),
            None,
            epoch.clone(),
        );

        if !outbox_only {
            message_collection
                .insert_one(message)
                .session(&mut *session)
                .await?;
        }

        // change message for outbox_collection
        let outbox_message = FrameRecord::new(
            content.clone(),
            next_sequence_number.clone(),
            Some(self.chat_id),
            epoch.clone(),
        );

        self.messages_outbox_collection
            .insert_one(outbox_message)
            .session(&mut *session)
            .await?;

        Ok(())
    }

    async fn store_message_in_session(
        &self,
        session: &mut ClientSession,
        content: Vec<u8>,
        epoch: i64,
    ) -> Result<(), mongodb::error::Error> {
        let message_collection = &self.messages_collection;

        let mut cursor = message_collection
            .find(doc! {})
            .sort(doc! { "sequence_number": -1 })
            .limit(1)
            .await?;

        let next_sequence_number = match cursor.try_next().await? {
            Some(result) => result.sequence_number + 1,
            None => 0,
        };
        debug!(
            "Store message with sequence number {}",
            next_sequence_number
        );

        let mut message = FrameRecord::new(content, next_sequence_number, None, epoch);

        message_collection
            .insert_one(message.clone())
            .session(&mut *session)
            .await?;

        // change message for outbox_collection
        message.chat_id = Some(self.chat_id);

        self.messages_outbox_collection
            .insert_one(message)
            .session(&mut *session)
            .await?;

        Ok(())
    }

    async fn get_existing_collection(chat_id: Uuid) -> Result<Self, mongodb::error::Error> {
        let db = DATABASE.get().ok_or_else(|| {
            mongodb::error::Error::from(std::io::Error::other("DATABASE is not initialized"))
        })?;

        let messages_collection = db.collection(&format!("{GROUP_COLLECTION_NAME}/{}", &chat_id));
        let messages_outbox_collection = db.collection(OUTBOX_COLLECTION_NAME);

        Ok(Self {
            messages_collection,
            messages_outbox_collection,
            chat_id,
        })
    }

    async fn drop_in_session(&self, session: &mut ClientSession) -> Result<(), StorageError> {
        self.messages_collection
            .drop()
            .session(&mut *session)
            .await?;

        Ok(())
    }

    fn extract_branch_change(
        messages: &FrameRecord,
    ) -> Result<Option<BranchChange<CortadoAffine>>, StorageError> {
        let mut buf = BytesMut::new();
        buf.put(messages.content.as_slice());
        let frame = Frame::decode(buf)?;

        if let Some(operation) = &types::utils::extract_operation(frame)? {
            return match operation {
                Operation::AddMember(branch_changes)
                | Operation::RemoveMember(branch_changes)
                | Operation::KeyUpdate(branch_changes)
                | Operation::LeaveGroup(branch_changes) => {
                    Ok(Some(decode_branch_change(branch_changes)?))
                }
                _ => Ok(None),
            };
        }

        Ok(None)
    }

    async fn get_epoch_changes(&self, id: Uuid, epoch: u64) -> Result<ArtUpdate, StorageError> {
        let limit = types::DEFAULT_LIMIT;
        let mut skip = 0;

        let mut records = MongoFramesStorage::new(&id)
            .await?
            .list(doc! {"epoch": epoch as i64}, limit, skip)
            .await?;

        let mut branch_changes = Vec::new();
        while !records.is_empty() {
            for record in records {
                let mut buf = BytesMut::new();
                buf.put(record.content.as_slice());
                let frame = Frame::decode(buf)?;

                let operation = utils::extract_operation(frame)?;

                match operation {
                    Some(Operation::KeyUpdate(branch_change_bytes))
                    | Some(Operation::AddMember(branch_change_bytes))
                    | Some(Operation::RemoveMember(branch_change_bytes))
                    | Some(Operation::LeaveGroup(branch_change_bytes)) => {
                        branch_changes.push(decode_branch_change(&branch_change_bytes)?);
                    }
                    Some(Operation::Aggregated(aggregation_change_data)) => {
                        return Ok(ArtUpdate::AggregatedChange(decode_aggregated_change(
                            &aggregation_change_data,
                        )?))
                    }
                    _ => {}
                }
            }
            skip += types::DEFAULT_LIMIT;

            records = MongoFramesStorage::new(&id)
                .await?
                .list(doc! {"epoch": epoch as i64}, limit, skip)
                .await?;
        }

        Ok(ArtUpdate::BranchChange(branch_changes))
    }

    async fn get_epoch_changes_in_session(
        &self,
        id: Uuid,
        epoch: u64,
        session: &mut ClientSession,
    ) -> Result<ArtUpdate, StorageError> {
        let limit = types::DEFAULT_LIMIT;
        let mut skip = 0;

        let mut records = MongoFramesStorage::new(&id)
            .await?
            .list_in_session(doc! {"epoch": epoch as i64}, limit, skip, &mut *session)
            .await?;

        let mut branch_changes = Vec::new();
        while !records.is_empty() {
            for record in records {
                let mut buf = BytesMut::new();
                buf.put(record.content.as_slice());
                let frame = Frame::decode(buf)?;

                let operation = utils::extract_operation(frame)?;

                match operation {
                    Some(Operation::KeyUpdate(branch_change_bytes))
                    | Some(Operation::AddMember(branch_change_bytes))
                    | Some(Operation::RemoveMember(branch_change_bytes))
                    | Some(Operation::LeaveGroup(branch_change_bytes)) => {
                        branch_changes.push(decode_branch_change(&branch_change_bytes)?);
                    }
                    Some(Operation::Aggregated(aggregation_change_data)) => {
                        return Ok(ArtUpdate::AggregatedChange(decode_aggregated_change(
                            &aggregation_change_data,
                        )?))
                    }
                    _ => {}
                }
            }
            skip += types::DEFAULT_LIMIT;

            records = MongoFramesStorage::new(&id)
                .await?
                .list_in_session(doc! {"epoch": epoch as i64}, limit, skip, &mut *session)
                .await?;
        }

        Ok(ArtUpdate::BranchChange(branch_changes))
    }
}

#[async_trait::async_trait]
impl DataStorage for MongoFramesStorage {
    type Data = FrameRecord;

    async fn get_collection(&self) -> &Collection<Self::Data> {
        &self.messages_collection
    }
}
