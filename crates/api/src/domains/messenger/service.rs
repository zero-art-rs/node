use std::marker::PhantomData;
use bytes::{BufMut, BytesMut};
use cortado::CortadoAffine;
use mongodb::bson::Document;
use prost::Message;
use storage::{ARTStorage, DataStorage, FrameStorage, KeyStorage, MongoFramesStorage, SessionSupport};
use tracing::{debug, trace};
use types::{FrameRecord, errors::MessageServiceError, protos::{Frame, SpFrame, SpFrames}, ARTRecord, KeyRecord};
use uuid::Uuid;
use types::errors::ARTServiceError;

pub struct MessengerService<F, S> {
    // art_storage_type: PhantomData<A>,
    frame_storage_type: PhantomData<F>,
    // key_storage_type: PhantomData<K>,
    session_type: PhantomData<S>,
}

impl<F, S> MessengerService<F, S> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<F, S> Default for MessengerService<F, S> {
    fn default() -> Self {
        Self {
            frame_storage_type: Default::default(),
            session_type: Default::default(),
        }
    }
}

impl<F, S> MessengerService<F, S>
where
    // A: ARTStorage<Data = ARTRecord<CortadoAffine>, Session = S> + SessionSupport<A::Error, S>,
    F: FrameStorage<Data = FrameRecord, Session = S> + SessionSupport<F::Error, S>,
    // K: KeyStorage<Data = KeyRecord, Error = A::Error, Session = S> + SessionSupport<A::Error, S>,
    // ARTServiceError: From<A::Error>,
    // ARTServiceError: From<F::Error>,
    MessageServiceError: From<F::Error>,
{
    pub async fn send_message(
        &self,
        message: Vec<u8>,
        id: &Uuid,
        epoch: i64,
        outbox_only: bool,
    ) -> Result<(), MessageServiceError> {
        debug!("Store and send new message");
        F::new(*id)
            .await?
            .store_message(message, epoch, outbox_only)
            .await?;
        debug!("Message sent");
        Ok(())
    }

    pub async fn list_messages(
        &self,
        id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<BytesMut, MessageServiceError> {
        trace!("List messages...");
        let frame_records = F::new(*id)
            .await?
            .list(filter.clone(), None, limit, skip)
            .await?;

        if frame_records.is_empty() {
            debug!("No records found for filter {}", &filter);
        } else {
            debug!(
                "Found {} records for the filter {}",
                frame_records.len(),
                &filter
            );
        }

        let mut sp_frames = SpFrames { sp_frames: vec![] };
        trace!("Retrieve sp_frames from frame_records");
        for message in &frame_records {
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
        trace!("Encode sp_frames...");
        sp_frames.encode(&mut sp_frames_buf)?;

        Ok(sp_frames_buf)
    }

    pub async fn count_messages(
        &self,
        id: &Uuid,
        filter: Document,
        limit: i64,
        skip: i64,
    ) -> Result<u64, MessageServiceError> {
        Ok(F::new(*id)
            .await?
            .count(filter, limit, skip)
            .await?)
    }

    pub async fn delete_messages(
        &self,
        id: &Uuid,
        filter: Document,
    ) -> Result<Vec<FrameRecord>, MessageServiceError> {
        let result = F::new(*id).await?.delete(filter).await?;
        Ok(result)
    }
}
