use art::types::BranchChanges;
use bytes::{BufMut, BytesMut};
use cortado::CortadoAffine;
use mongodb::bson::{Document, doc};
use prost::Message;
use storage::{DataStorage, FrameStorage, MongoFramesStorage};
use tracing::debug;
use types::errors::{ARTServiceError, MessageServiceError};
use types::protos::group_operation::Operation;
use types::protos::{Frame, SpFrame, SpFrames};
use types::utils::decode_branch_changes;
use types::{FrameRecord, protos};
use uuid::Uuid;

pub struct MessengerService {}

impl MessengerService {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for MessengerService {
    fn default() -> Self {
        Self::new()
    }
}

impl MessengerService {
    pub async fn send_message(
        &self,
        message: Vec<u8>,
        chat_id: &Uuid,
        epoch: i64,
        outbox_only: bool,
    ) -> Result<(), MessageServiceError> {
        debug!("Store and send new message");
        MongoFramesStorage::new(chat_id)
            .await?
            .store_message(message, epoch, outbox_only)
            .await?;
        debug!("Message sent");
        Ok(())
    }

    pub async fn list_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<BytesMut, MessageServiceError> {
        let messages = MongoFramesStorage::new(chat_id)
            .await?
            .list(filter.clone(), None, limit, skip)
            .await?;

        if messages.is_empty() {
            debug!("No messages found for filter {}", &filter);
        } else {
            debug!(
                "Found {} messages for the filter {}",
                messages.len(),
                &filter
            );
        }

        let mut sp_frames = SpFrames { sp_frames: vec![] };
        for message in &messages {
            let mut frame_buf = BytesMut::new();
            frame_buf.put(&*message.content);

            sp_frames.sp_frames.push(SpFrame {
                seq_num: message.sequence_number,
                created: Some(prost_types::Timestamp {
                    seconds: message.created_at.timestamp(),
                    nanos: message.created_at.timestamp_subsec_nanos() as i32,
                }),
                frame: Some(Frame::decode(&mut frame_buf)?),
            });
        }

        let mut sp_frames_buf = BytesMut::new();
        sp_frames.encode(&mut sp_frames_buf)?;

        Ok(sp_frames_buf)
    }

    pub async fn count_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<u64, MessageServiceError> {
        Ok(MongoFramesStorage::new(chat_id)
            .await?
            .count(filter, limit, skip)
            .await?)
    }

    pub async fn delete_messages(
        &self,
        id: &Uuid,
        filter: Document,
    ) -> Result<Vec<FrameRecord>, MessageServiceError> {
        let result = MongoFramesStorage::new(id).await?.delete(filter).await?;
        Ok(result)
    }
}
