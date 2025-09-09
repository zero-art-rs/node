use bytes::{BufMut, BytesMut};
use mongodb::bson::Document;
use prost::Message;
use storage::{DataStorage, MessageStorage, MongoMessageStorage};
use tracing::debug;
use types::MessageRecord;
use types::errors::MessageServiceError;
use types::protos::{Frame, SpFrame, SpFrames};
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
    ) -> Result<(), MessageServiceError> {
        debug!("Store and send new message");
        MongoMessageStorage::new(chat_id)
            .await?
            .store_message(message, epoch)
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
        let messages = MongoMessageStorage::new(chat_id)
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
        Ok(MongoMessageStorage::new(chat_id)
            .await?
            .count(filter, limit, skip)
            .await?)
    }

    pub async fn delete_messages(
        &self,
        chat_id: &Uuid,
        filter: Document,
    ) -> Result<Vec<MessageRecord>, MessageServiceError> {
        let result = MongoMessageStorage::new(chat_id)
            .await?
            .delete(filter)
            .await?;
        Ok(result)
    }
}
