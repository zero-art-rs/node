use protos::rpc::v1::{
    GetArtRequest, GetArtResponse, zk_messenger_service_server::ZkMessengerService,
};
use tonic::{Request, Response, Status};

pub struct MessengerRpcServer {
    // DB and other members will go here
}

impl MessengerRpcServer {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl ZkMessengerService for MessengerRpcServer {
    async fn get_art(
        &self,
        request: Request<GetArtRequest>,
    ) -> Result<Response<GetArtResponse>, Status> {
        todo!()
    }
}
