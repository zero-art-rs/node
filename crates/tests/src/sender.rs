use crate::utils::CentrifugoTokenResponse;
use art::types::PublicART;
use axum_test::{TestResponse, TestServer};
use bytes::{Bytes, BytesMut};
use cortado::CortadoAffine;
use hyper::StatusCode;
use prost::Message;
use std::rc::Rc;
use types::art_schemas::{ChallengeResponse, GetARTQuery, GetARTResponse};
use types::centrifugo_schemas::AuthRequest;
use types::messenger_schemas::GetMessageQuery;
use types::protos::{Frame, SpFrames};
use uuid::Uuid;

/// Trait for Sending requests from UserTestModel
pub(crate) trait Sender: Clone {
    /// returns response from the server and the message, which was sent
    async fn send_frame(
        &self,
        frame: Frame,
        id: Uuid,
        check_status_code: Option<StatusCode>,
    ) -> eyre::Result<Vec<u8>>;

    async fn get_centrifugo_token(
        &self,
        request: AuthRequest,
    ) -> eyre::Result<CentrifugoTokenResponse>;

    async fn get_message(&self, query: GetMessageQuery, id: Uuid) -> eyre::Result<SpFrames>;

    async fn get_challenge(&self, id: Uuid) -> eyre::Result<Vec<u8>>;

    async fn get_art(
        &self,
        query: GetARTQuery,
        id: Uuid,
        epoch: u64,
    ) -> eyre::Result<PublicART<CortadoAffine>>;
}

#[derive(Clone, Debug)]
pub(crate) struct TestSender {
    pub(crate) backend_url: String,
    pub(crate) centrifugo_url: String,
    pub(crate) client: reqwest::Client,
}

impl Sender for TestSender {
    async fn send_frame(
        &self,
        frame: Frame,
        id: Uuid,
        check_status_code: Option<StatusCode>,
    ) -> eyre::Result<Vec<u8>> {
        let body = frame.encode_to_vec();

        let res = self
            .client
            .post(format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, "frames"
            ))
            .body(Bytes::from(body.clone()))
            .send()
            .await?;

        match check_status_code {
            Some(status_code) => assert_eq!(res.status(), status_code),
            None => assert!(res.status().is_success()),
        }

        Ok(body)
    }

    async fn get_centrifugo_token(
        &self,
        request: AuthRequest,
    ) -> eyre::Result<CentrifugoTokenResponse> {
        let centrifugo_token_response = self
            .client
            .post(format!("{}/{}", self.backend_url, "centrifugo/auth"))
            .json(&request)
            .send()
            .await?;

        assert_eq!(centrifugo_token_response.status(), StatusCode::OK);

        let centrifugo_token_response = centrifugo_token_response
            .json::<CentrifugoTokenResponse>()
            .await?;

        Ok(centrifugo_token_response)
    }

    async fn get_message(&self, query: GetMessageQuery, id: Uuid) -> eyre::Result<SpFrames> {
        let response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, "frames"
            ))
            .query(&query)
            .send()
            .await?;

        assert_eq!(response.status(), StatusCode::ACCEPTED);

        Ok(SpFrames::decode(BytesMut::from(&*response.bytes().await?))?)
    }

    async fn get_challenge(&self, id: Uuid) -> eyre::Result<Vec<u8>> {
        let challenge_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, "challenge"
            ))
            .send()
            .await?;

        assert_eq!(challenge_response.status(), StatusCode::OK);

        let challenge = challenge_response
            .json::<ChallengeResponse>()
            .await?
            .challenge;

        Ok(challenge)
    }

    async fn get_art(
        &self,
        query: GetARTQuery,
        id: Uuid,
        epoch: u64,
    ) -> eyre::Result<PublicART<CortadoAffine>> {
        let get_art_response = self
            .client
            .get(format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, epoch
            ))
            .query(&query)
            .send()
            .await?;

        assert_eq!(get_art_response.status(), StatusCode::OK);

        let received_art = PublicART::<CortadoAffine>::deserialize(
            &get_art_response.json::<GetARTResponse>().await?.art,
        )?;

        Ok(received_art)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct TestServerSender {
    pub(crate) backend_url: String,
    pub(crate) centrifugo_url: String,
    pub(crate) test_server: Rc<TestServer>,
}

impl Sender for TestServerSender {
    async fn send_frame(
        &self,
        frame: Frame,
        id: Uuid,
        check_status_code: Option<StatusCode>,
    ) -> eyre::Result<Vec<u8>> {
        let body = frame.encode_to_vec();

        let test_response = self
            .test_server
            .post(&format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, "frames"
            ))
            .bytes(Bytes::from(body.clone()))
            .await;

        match check_status_code {
            Some(status_code) => test_response.assert_status(status_code),
            None => test_response.assert_status_in_range(200..204),
        };

        Ok(body)
    }

    async fn get_centrifugo_token(
        &self,
        request: AuthRequest,
    ) -> eyre::Result<CentrifugoTokenResponse> {
        let centrifugo_token_response = self
            .test_server
            .post(&format!("{}/{}", self.backend_url, "centrifugo/auth"))
            .json(&request)
            .await;

        centrifugo_token_response.assert_status(StatusCode::OK);

        let centrifugo_token_response = centrifugo_token_response.json::<CentrifugoTokenResponse>();

        Ok(centrifugo_token_response)
    }

    async fn get_message(&self, query: GetMessageQuery, id: Uuid) -> eyre::Result<SpFrames> {
        let response = self
            .test_server
            .get(&format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, "frames"
            ))
            .add_query_params(&query)
            .await;

        response.assert_status(StatusCode::ACCEPTED);

        Ok(SpFrames::decode(BytesMut::from(
            &*response.as_bytes().to_vec(),
        ))?)
    }

    async fn get_challenge(&self, id: Uuid) -> eyre::Result<Vec<u8>> {
        let challenge_response = self
            .test_server
            .get(&format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, "challenge"
            ))
            .await;

        challenge_response.assert_status(StatusCode::OK);

        let challenge = challenge_response.json::<ChallengeResponse>().challenge;

        Ok(challenge)
    }

    async fn get_art(
        &self,
        query: GetARTQuery,
        id: Uuid,
        epoch: u64,
    ) -> eyre::Result<PublicART<CortadoAffine>> {
        let get_art_response = self
            .test_server
            .get(&format!(
                "{}/{}/{}/{}",
                self.backend_url, "v1/group", id, epoch
            ))
            .add_query_params(&query)
            .await;

        get_art_response.assert_status(StatusCode::OK);

        let received_art = PublicART::<CortadoAffine>::deserialize(
            &get_art_response.json::<GetARTResponse>().art,
        )?;

        Ok(received_art)
    }
}
