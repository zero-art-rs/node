use crate::{MongoSessionSupport, SessionSupport};
use mongodb::error::Error;
use mongodb::ClientSession;

#[async_trait::async_trait]
impl<M> SessionSupport<Error, ClientSession> for M
where
    M: MongoSessionSupport<Error, ClientSession> + Send + Sync,
{
    async fn start_session(&self) -> Result<ClientSession, Error> {
        self.start_session().await
    }

    async fn start_transaction(session: &mut ClientSession) -> Result<(), Error> {
        session.start_transaction().await
    }

    async fn commit_transaction(session: &mut ClientSession) -> Result<(), Error> {
        session.commit_transaction().await
    }
}
