use crate::domains::art::transport::utils::{as_base64, decode_art, decode_branch_changes};
use crate::{container::Container, errors::ApiError};
use art::types::NodeIndex;
use art::{traits::ARTPublicAPI, types::BranchChangesType};
use axum::{
    Json,
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
};
use base64::{Engine, prelude::BASE64_STANDARD};
use callbacks::callback;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, instrument};
use types::ProofRecord;
use types::callback_wrappers::{ProofVerifierMessage, ProofVerifierResult};
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequest {
    /// Serialised art structure for new chat.
    #[schema(
        example = "QHVOIsS7aF9klJHKUrxekAPKV+33NbmB4J5NK/mh6IQEGL8+nmZHd7rRwbWDOGRq0g9woVvX+rkmxu0tUHNIPYwBQMw5OnIemJkFNHnm8HCsN+99ekIxBgYotCVAYwdDPZAP5HTFQe45VuD23SI2mxW8D8j3KknDPepDEmg8n0j+1AIBQOuX8YX/e0i16YbSJ2lpURvM+0QcuToiM9UyBPvadnEDbZNGFdiQL3ULmmOtRtL2+BP9DTmBeNxx3fhYGU95FAIBQH/P/s7p2KJZctpWuukfHNEAK/oQrZfQ0j5hs+Qm6ecEaiBYdyjJ1FggyqqDkbfDEsjtebcPuZhp5u/cEQtd2wEAAAABAAFAahDbRHi2H9n/6hkYlFvT+EykOrS3Hd9fe9/yhehPeAx458pBSNKYPFXt+zjMTnrm4EMBYPy1boslBDoZBC+cAAAAAAEAAAIAAUCRRwLnU975oL83p+ZFh/zi6+8IfyqHlX9mZ3f6rVU6As4ao2I5tVFmfuuqhkQBjIZRxMfm/rSOIA4yrF5/53mMAUC9TfgZxLFEsOVYQsrnA59gb2Vr6qYW2PaMXfM/aTclAdcCotgt00eyxNt+EgWJ8JrjVVxZIt6RMzmvnmz0NwSPAAAAAQABQPVlwjO+3CsDj5QABAm2nwGO5ZEkZ8SCnzQK1m0tG/INK8L/ZBea3aK0weGxV2aWDtZul1Zf72mO+udih9HdP44AAAABAAACAAAEAAFA0DtiBydl146UurHWfv/bO6YSBkVWLxG3Cpxykz7ZJwJu6sGgmhc5gBRTs5Wq6+fZZh4+iplHCO+PY71NIrXOBwFASx7wVwl1/xvn4u0LYvN0XqsJxtgKehRInq7TO1UEyQ6UHnYb/1oa1sIlHrOs2GoMS5RdFCr8TybWxZDobwTRAwFAyiSMcuKbWNbI1809Y7g3neYpF4+BnhLWmU9Ea3oWOgoIeNPNQKQJ9TQ9eXOzmmCfXw81UGtrE6z39p/fyG+LAwAAAAEAAUBr5E3m12kSS8uDciloIUALp+VvI77c+54dtMdwaT/jDruUOc20HJ2CIbUs3b7cDA8Gae1bHVcGDDXOxB8HHaOPAAAAAQAAAgABQE2+Ilu7OJj6fMBlJLUmYmHU/rQO0U4nRwKyRH7CH6wNzxvjyz3rYSXq1rm72pt8hwJ4vKZtIEv23rVnuZ86p4sBQOZJ/qh4RDUjWBl9010Pa+YP2Kq781NS23Gk6/h6BxYNmT2emqCBuimYo+xrTLDFslQdpWuOQSYaZz+d0u9264wAAAABAAFAjE/8rW2OG9VVZ4vmzKuE0bFnjgMzSnNtkxgh1752lwxFI56bmXf95h4ZE9RRrup+AAE/qU7lpBJBsuqJwllAjgAAAAEAAAIAAAQAAAgAQBb0nJfwNLo+g8mUCYwoe7xHMdoOH8lBUPbSxGO4ZU4L5MbvfV1rfEC5ESVj2skQDLeEMMWdQeH9uM2NQl/Vbos="
    )]
    #[serde(with = "as_base64")]
    art: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Indicates whether the chat is private (one to one).
    #[schema(example = false)]
    pub is_private: bool,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/init-chat",
    request_body = InitChatRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn init_chat(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<InitChatRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    let art = decode_art(&payload.art)?;

    state
        .art_service
        .init_chat(&payload.chat_id, art, payload.is_private)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::CREATED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Serialised proof.
    #[param(
        example = "zUBu5CrotROhc6cAx3bvCKQACuOFRTFeZAmPl4t4kQ8BAAAAAAAAAOJjbwd+2wkEmO4xlU8rRo1vSy2ef9zw8er7ik5BPeUK"
    )]
    #[serde(with = "as_base64")]
    signature: Vec<u8>,

    /// Users leaf node index
    #[param(example = 8)]
    index: u32,

    /// User provided nonce
    #[param(example = "RXhhbXBsZSBub25jZQ==")]
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,

    /// Sequence number of the requested art. If not set, return the latest.
    #[param(example = 1)]
    pub sequence_number: Option<i64>,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/art",
    params(GetARTQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn get_art(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<GetARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let previous_art_record = state
        .art_service
        .get_previous_art(&payload.chat_id, payload.sequence_number)
        .await?;

    let mut msg = Vec::new();
    msg.extend_from_slice(payload.chat_id.as_bytes());
    msg.extend(payload.nonce);
    msg.extend(payload.index.to_le_bytes());

    let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
        signature: payload.signature,
        public_keys: vec![previous_art_record.art.root.public_key],
        msg,
    };

    match callback(&state.proof_verifier_sender, schnorr_signature_message).await {
        Ok(message) => {
            let ProofVerifierResult::SchnorrSignature { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    let art_record = state
        .art_service
        .get_art(&payload.chat_id, payload.sequence_number)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let art_bytes = art_record
        .art
        .serialize()
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let encoded_art = BASE64_STANDARD.encode(art_bytes);

    Ok((StatusCode::OK, encoded_art))
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetInitialARTQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Serialised proof.
    #[param(
        example = "zmSdxy4UcketbhCuM+aVZ09GxA19TTUdPxdEo59ILwYBAAAAAAAAAG6aOMUyql1Vh7w3GNp2qt7rr6G7XvBRWdOCUIhngdgK"
    )]
    #[serde(with = "as_base64")]
    signature: Vec<u8>,

    /// Users leaf node index
    #[param(example = 8)]
    index: u32,

    /// User provided nonce
    #[param(example = "RXhhbXBsZSBub25jZQ==")]
    #[serde(with = "as_base64")]
    nonce: Vec<u8>,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/initial-art",
    params(GetInitialARTQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn get_initial_art(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<GetInitialARTQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let mut initial_art_record = state
        .art_service
        .get_initial_art(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let leaf_node = initial_art_record
        .art
        .get_node(NodeIndex::Index(payload.index))?;
    if !leaf_node.is_leaf() {
        return Err(ApiError::BadRequest("The node isn't a leaf".to_string()));
    }

    let mut msg = Vec::new();
    msg.extend_from_slice(payload.chat_id.as_bytes());
    msg.extend(payload.nonce);
    msg.extend(payload.index.to_le_bytes());

    let schnorr_signature_message = ProofVerifierMessage::SchnorrSignature {
        signature: payload.signature,
        public_keys: vec![leaf_node.public_key],
        msg,
    };

    match callback(&state.proof_verifier_sender, schnorr_signature_message).await {
        Ok(message) => {
            let ProofVerifierResult::SchnorrSignature { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    let art_bytes = initial_art_record
        .art
        .serialize()
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let encoded_art = BASE64_STANDARD.encode(art_bytes);

    Ok((StatusCode::OK, encoded_art))
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberRequest {
    /// Serialised BranchChanges:AppendNode structure.
    #[schema(
        example = "AUB0urTTqwXgQt9FnyA0DzPCkHbfZPx5Tnbtu4ApwNNAAt3A68AFXBv6RU+dOJ2uft6tRml2W6HSPktlP3PS66iNAAAAAQDIAgUAAAAAAAAA2vrTKjDNUuMjI/9/VQBjWMsycwz55AGdDgml5yXHKwkxwmm+Fb+26mRv7d/Wfhi9wpscXZHFXBQZXwgqod8Kioqy60ZImTb3xVNlt74eTzEOmAXKK5uQ2cHzPr6cITcHKmzAqUY8f5s2mRWBDHOsl1/BrUVmH4aKzhBZCGyoBgIRdfOd/IRv0iwex8PFx/V+DaFVTLxhnRcGWk0qaA4bDlwJy0+22eqpdO8xxgc4hsBgz9ImAlz9h7ZjhqbJ10eOJzYF1QxsgHS6g7wsHjJPZqxxkq4mN8vLatzsfnP58QNHfTNlqUMIhXD5IcgR+UELutN9AIxqt0sKTGJCAj8aAHS6tNOrBeBC30WfIDQPM8KQdt9k/HlOdu27gCnA00AC3cDrwAVcG/pFT504na5+3q1GaXZbodI+S2U/c9LrqI0ACA=="
    )]
    #[serde(with = "as_base64")]
    branch_changes: Vec<u8>,

    /// Serialised proof.
    #[schema(
        example = "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"
    )]
    #[serde(with = "as_base64")]
    proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/add-member",
    request_body = AddMemberRequest,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn add_member(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<AddMemberRequest>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    let art = state
        .art_service
        .get_art(&payload.chat_id, None)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?
        .art;

    let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

    let add_member_message = ProofVerifierMessage::AddMember {
        proof: payload.proof.clone(),
        co_path,
        associated_data: art.serialize()?,
    };

    match callback(&state.proof_verifier_sender, add_member_message).await {
        Ok(message) => {
            let ProofVerifierResult::AddMember { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    state
        .art_service
        .append_member(
            &payload.chat_id,
            &branch_changes,
            &ProofRecord::AddMember {
                proof: payload.proof,
            },
        )
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMember {
    /// Serialised BranchChanges:UpdateKeys structure.
    #[schema(
        example = r#"AEBqENtEeLYf2f/qGRiUW9P4TKQ6tLcd31973/KF6E94DHjnykFI0pg8Ve37OMxOeubgQwFg/LVuiyUEOhkEL5wAIDef4VDfd4IpU5zytQEbb07HasgN+7uh8Nuy+Z9sKqAHiAIEAAAAAAAAAOe9K4Vv22xfOUmaL9Lw+/zDisVnTsJcitnAXAiCz38ESx1h3sa/HXUURlpHmKnzwkeSGL0gbTnUNzbjhkSIIohNxl5CFfLAQ2DfRt+Z+egwjgAGY9H22ly4R8I0ekkHC33f34mqKtdccZfRbfnTvzqE/inTg2IAO2BqPQz+lKGP3lpeFTKKD5neWN8xOjEJgJhVZuntoYoWXFmCuZeB6wCXbogsc8vcPKNGte/jiAb7VWfyvgF/vja9v7eusCcziuj2vYhZaT9ROhTy+jhbZDeKcdT2dzo1SvqL3VhbLyMHJkRBbh1WKWhshFlWRLrDN8OohOIxfyIfKTnuNJKhgYwACQ=="#
    )]
    #[serde(with = "as_base64")]
    branch_changes: Vec<u8>,

    /// Serialised proof.
    #[schema(
        example = "AwAAAAAAAABhBAAAAGpIALFtKWoCNlseOiSOJvyZ7/2aDIl7ETIuB7Jo3sgxVK+wg3dFvrj3eoH6C72NUnef5E01Mf0hgpwzq9kwKyx0ZkiEDpAryW5CLdwyMzcJyTwqqxXe6oDBsPPjx/hvEdB5GAy2FW0/hSxrGgiFyXFAEDzVRTjiFvGdrJ/W2A43vCgVn1IFX6eSmJX5dSqrhWEPf+AjgTMQxptlQ13Xug02BIHG89ATMM+CU9LtlyEu4MTowTMAh/39ugY0ifKzK9ZdeK1i5z3y9SMBikpB9GyCLPAjgzA/NJvfJgIAQIQkBA5ZUc4VvSrMcd1dZbtOcJi4AAcQSHWsy/FunwoVVETWuxcwv/USYX3lW9BPazSZWIrj6NvSTTvRVoOAEeR0ARk314Jx5UJtwqpNQZj7sD2ID3ZoLT1zC0XZop3/AgcHRzv7NDLYBFmx9xNSz9f+hZiTt7Q3kiL1BLYUqXRyJgtO/xpQExjo0tLlZFA5eaTgd7vtTXw6hphOqEStgECkANrrDTwIezIAr5V9NpxgvFiBj8tteIYut413LbkWbyoZWIvSJmAPr8s0R1WGKw0LgrwLs3KDLeZ6NuXNbwQHnB7W9bX+EplhcZFiRgD2fx3zB11fyqytPx/KabJzKXHqS2JTowWrNRsqtmsacKVSaRaFgbMiyipYSnswzRTcoWFwUp1sMN6ByB+WhaOwxK84bdjdr5fUuocE05yIQAq7mj68J4G9PlQPWaRHwwhsmSFjxY7Le0vZAR3AX/5+ZpW/RDDepNihcqfxKau6I+hwWk8BfydBZm1Myr0goqWwrDpyjHqY3Yhoe8/ihNdBB7NEiik/6UmWua+M4PFBDfYbx3UWDwaUkNT9O1dFpcVO5ZO4+M4BAy8nAGu9H3u4Y8KmCiy5FlHgR9N4AtAgpDA2dPbkHjsHTVTSZsrmfLM+RqAacL0bM8Io534XZ+rw3fId7KITTSEK4Jvr6RrVKv7Y2wwauIYfuw1C2Zsq0kSGee20YnPmDQr0wviOl791aXYdB+jtNFjTvBW65A7UWB/8vtYyv/BT8T8sQnd3ciVGFfwjaDIJXZf1iCho+e5HpFtG8z4Sj1wKWogZus9WsDb0XgPIA1f+dQ+KO43OtGKoFi8MxiFre/p/YJPGBuG9hyzabz7JYDtUNtJGSHfsoOAxYJovWE9QfYsgRMxEp9cZUeNitI+TmXvfkMeTxwESulHQ2NsoSjW3p+x1rfsnxgUgo2aCtc0AyamWpbzKKMOo+Q+CaFUVt9Upof/RuOU0aymwerCvyPM2uZIYuAVZLhhcFuLE80JsScdN9FAl4RzUTo1EvqlhZIya4wnqZ9VzWsBsEEWZGj/Y6MMKshhswCjpflEi7HYRhaZ1JI5LpeamRvcHRCxPu5f6HNhrQ4RM/2l/cZhQBS/xRhmCYFjZb9MUqvfRl/2OtJvWU9kgs8cyWMwAMmZHymhipE57p5+HAeGQ0pAh79R+82sjLsCuPbXVtgJhBAAAAEzq8R9SiLUy7QjNm9TabcKBfDrzXnRkGoD3t5MDTyIf8nFT9QRP4Ik/hyUiEVnFlbAAVPp0JwEtOpmfHg3JBXPQ70TlbKRhFxkyZVT7LGPa7Q2D7vx+Wu1G1xLhpk23MxBRisUbpNwjybJbR6C7HrrmN+qOVAkaytrafWiK6yJH7jOhVTlMV0hmrQy6Ob67I1iTE5pqbFyp2o4ZdAvs0DSAVo6uYvayaS8N59LY1pR9uzmN07bxJQ/B1ySo/rL0cihqsbVAFRUsHglJzn+PhD7jSc83la8Gb8/CaZ/7mX005K7xR1LNo8tYv7cqcMZ6KQ4qxs/CnqaqbKNuyHsjUzEbuF47/2kvMBw12CS7qbalIZwuMLkQscaII5W8g712DYZGidcqfs3lssUWckkTkCNz12BZoU1fpmuJxQBXF2wNhb19BCWE8RiXeypZ6eOvDtQziCE5CkgYEodxAHxhSgfkh4EtcMw9LtsY2sxiryQFdn7ECbwLyhEZIEpllQ7OO7JpEdolo/SNh8IFACyGO7jF5jSDcUQAkWPsItQeCF82VBNbGSo6s0+8NAsFe6ssmhkK46MhFPu/0N0NZOh18mFaC80UuBEt4la5yqNxpVccQLrs6fVaqGlb/TZ7gMEnV1o3JnBtJclYUbCvmiR8YS8YDt+68HsA68297VGxohlDdmaC1ZvZCgBRD8/GVBfXedFLFMBvfOSRQUp50+uiaRO86r3b5li6H8MADzqDXHiX6lEnPEYSEV1KAiXVia3xVdQCCl8tLRUFkodVEtJ6rZ2Jk069EDPIyF0s3cdfJTUCmnK70pwG0dfKhHwdBQIPImR2wNuj5ObYof3htFQVXSJaGL/mPsBP3HqpwlLY9K0EQlyperorNzX6t9r43cwIeL5RJ3+L8dBEN0e1zQo4I8SvyQ03HT1Y/EIbo0uv6NVIdotXYT/sqLb3dEW21r1Ryr6eMiLIYgzBOvYjy/4PI0igaeXtR5BPRMG2NXGROE8DjZ8dzKYemzmNG4YxNL2CLkI9dgKkeZrIgHgXqV9DXrc5FrFkssezj7/RZzlneqB5uLkfBZPuuCICDdbVW4La9SNS7uQSBgu1/1aTH5vOzlwSTy8k3o/mOLu9+wSPLqUicLwenPSInq4iQkdkfCMJSawsXKzer850iNcDncmkU2VImU7sTw1AFdI9EJ6ZXHwOnlI2Ps2EIu4MTv3SbANLgbFvvzKX27/gducnCAj3YQCmQTSyp5GKLLdMngRV15p2dUJpyxO0BzGIN6SIzcjeCnprbTwlmprnZBY3W67Tk+Mm6xN1IAT6wclm9zPGXr4vLPaEPX6vrSpZGqHWXUgzRqc7rcp6rd4DE9FXMefhXCAqxsyOLyjO6CWDJzJ16Y42NmwozlXUIwHtj3C5Fd5BEWo8rQ/znNvEsljCihbZmCUCkNYLLdXyXWKN6c3B6esMiAUdBpZ2Kgiv/0nV/0JYsJSFRcY1GEf6FO62LmMLRgxhBAAAAK49yGOib78pN3btutlh6dZp7KK2OgdWXQIK/66U5WABJL6QRwsmaEvNnNF0lJKOaR6qpm9Nx+uebHq9vtoHm2HQ4ZSs5+ECwpgI4qnUdqZFe+yz53JghOn4MReopUvaTUol57ZpRBWJLCY8L994AOqY+bINfwxMrTokAImNvK1XAGETXWcxSow1HRXroxg5KqbmRSVse21e6wvNPkONgyUQ4VmHOpo3e7ScialMzZnn+S9HEZckGuZhSflY6p6dGurF2+Dgu9rq6c5TKq0DEyDhzaxtVicrbzz6g8bUQp4EUgYv5Y4ACzAEIGIYwjtxHOoFq7mmPQ0nBR+XII2vB2ki4xNzumG9TngrrkBkFHcLHmhMBKWid/zkloBl1Mj2Ck1iFZ7wFtUl9//oT+WiHcB19PMNCXMMxTbD806v7IUL+nh2pV+I/9CkTzVwYIcbb+iwIqwCDet5nAY1xXfC2AuI5s9N61WBjGpxx8qWHv0LEAno4fQ+3v8NoyWlVq18TRinnYw0CwYQX+gSA8F/fn7ls0xfwmQkPDLfTE0HkC1n8h09lGf5dmKHwc21HZUA0LyzC0JI8pFqn98itf3nhmmcCl4WfOe8AcJFRiwjKJ4LQmAhAYDIWNY7wEm3fOB1D84vX0FvCoTBGfinduHsBUxjC3B9RlDoZeQC8Kj3XlFZXGeBNb7TfYhj3oOEkWBOTkLdoM48UolJ0JNLiILidEH8kMM6BHpIq67jQPNtujAiY8vCreYRnE42PAWEm8qxZd6SumK6wNSUA7X9uze0vHQEVPnXaZslNxW0zzW7NfZbEpFXju6+a9Rjf6l/NNPhliMjzooCY4GLU0QOnpBlPW+4Z7UGo0DGZR09IZKpzFx6HHjV9L+Bky8f2JpFZnY3Pmr9iix133cYtqbSJs+KQQA8XvUMKByqsjn0USmJ7kdWCq/3ROhIh3TGH87EOB/DOYh2u8nAYCVC36YWGssHRkjUGcP9R51iCXc3P3+0SNCuLhapn1dlNnIPJTfN7b6OPQ4ln1MQmgxgIdyD/ckMu9tb8LVE4OoYYwYHeC7STbE6YNOx+A/HSj+z+BPFNg6lO+JwHcOAh+rjlpOo6lYTwmM0WAC8WtTPjOsDqTh6GeEcDsPjKSBbFHNGIozxL1fbKaY5RNM/oZlUDBLmBk458s3XWmsWO1MJCStw0iwY2wcimvEK9CerOU0i9GfAmCFRGjfUOa/O3UsHLbFytOpnoQiGtQxnXIg+DW6YmqDtgstDo6OWwsusH2TTSsveWOAlZ3rJtleinUFdt2Iat9aC9EOdT/X+ePHq9IJPB3rxD9FCAOb7m0te7UJ4vi5eK6rFlp5oWVw2m3YdoI7ho0++shgwMjpdPRDCPQXQ2mDOdccLaAvNr/My0IYY8H4AYBp8VApOArOHYKCu8OmiXEyuwYPJeLH8mF0TX5XVE+/t5LcLrVjdQDgGbh9wLCKVSVrItsh0Yd/o73+7GDvczY9QQQ9bH1hnHLpQS3CgBeW/uveJAAAAAAAAAAAAAAAAAAAAADEAAAAAAAAAAEW4LKASeKfywpdQVPgvhZJUXvy91tj0VnnIp0CJYQ8LArpGV7IE/YyJVZotd/23X0WBN6b5ZbVnUaGvAMOKk/ACAvdeFBv/L0frO6rrl8PjzsLADkFMv0IrP04bB3IAj7oBAtkaxHUj1v7KkRrRNEFkx/0+1OwVHQ8qlNt8tD2aB+YFAj1JFLf6DrIupouwkeaiABEQzTn8eWN8lNotuau/sp0BANEvfqnxTjSoucOI5wzRgykRLdJ9BO5dObuMuoF9pbkOAMqkGhjfSgz9spLr2FON2TzbmBWhbZjBZTxJQ+fk7SYEAEfbiLD98IVOKKyjV5J/f1N9Ki9R6cbATTtX4iibJxwHALZ01SBhxsVFPG77rr3bYBq6tN9olukw3ZcvgysGnjYJASq7i05zopw3vsdxX4rPp3QUH0aZci5TBRsyRVxPrq4FAaLbbRIpk130kDsQ86ZLRjK8KO7911uui2J27FUc59oBAb64OBKhqkwYShjZSni95AIPjVXoCa9POSD98/o8cw8NAT/BOTQ56dBrKnYQ9M91B7/Jh9FSJWYZAMM5fVR6GaIBAl9+vE3zpPswz9SjPEWSzDmU5wR4AD3STMSYOsiwWUQFAr8h/p/DJyxLHXGqTfvtVoX9CWUS1JZmcjf2EXUBetIAAi6xo82BVBd/FL5cKoT/CWsAOZ22Fyj3hG/JrRnTsxIDAmhsV/6C9IkcQDGCaWqItmhPRl+DRrGNEI4m3lPHyjgGAN/2/ymrC4jvruFnFRtQTQtnEC0DgNPGfXoNzZJZ/LcFADL4Y5DJrDosUcjkaSVbOfIzUbflyarBeem+Wvn6R5UIAGbofrrsmTOA5P3SAV68vWWKP4FuI5neHjPxHEJeWfgAAIG5VnPJLe5+JrNpJTCGmzfIQzi09npkaS5uMQmLzQIIAdaRiz1BOoiDWHWY9JYymHyskwBiNTeInY/W1+D5Eo0JAXp3hqkoRlqBAbDh/Y5BcnI2Xr1cwc0AHMwq7LjvZ58KAf9prtUsXHOv0OfPX2+71IMEbVQTjDauWV1RC5SI+K4OATogaHfI3VG9Ti9/aBb1ABqwrracwMi2/ZTkMV0fG8UCAq+bBeyXQh+xg4cb9hclJQ2wS6NMpTolnJtkTg5JcOcCAj8rtY9v7f1MXwlSMMP75m23I1xcojKClIrDr4+QCaIHAid+78DCKD7K+TGsKtFf5imRGuyLXmOUoLS/ysG+K1AFAqad4eDgDmfpjp942uG0cG8VPQphKmjE+L/iGIOXhUUAAANu88+Si8tG6qsSGHM039lbeHnyXdCMd4rs8PApKjsMAIgI0rZsrq77Ym8VPO+blI3Tn1mBTwHpxs34zpuX/24HAC88zug4TjpJtCoPDua6eer9Zzj6EcLx84eycgPjpjcGAHjWsjIJA7rFamA75OmX2qjMblcpD5gb2O39MNWKp4wDAbJJ4D+a/1qfqdz2pQPWMOcvEs15TXYOGCEAddxPzFILAY5VcucXwNQ6H5XE9eHvClIOI8yAZJdFtZyMlpS8DaQLAbvEbFxZU1XhpGZzJO5D9NJUF/TNymoexujkv5elhC4CAUPTJud/qjelB0qUpyDP6nvAru/qrWdPsbqAo07LCsQAAoVkxbGEwfRFSnKaEZ6KHI2i5FQWqJq4XcbW20pKBM0BAqks3Q7jVYg6Nas6mlnTLdjYPKGKefnv1etT+n+zQTwEAg0JNUGrHrhP0QB7hLHAwlhrf0hP7V3AlZ4UhA56DqgBAtRNz7SxbkAq+LIgBNo+7ULtXlT3ghEO0fOifyjXEEkBAH6M6d6mxZs8hkH8BH6BUmIxvKRwpq/HY8YV4KfgA48JACzL54QX6l92P/2yuQWUtP4yzEKi97Ji/s1amSPC9w8OAJaB3FOjaFX1MmdHd+FqJunZHik7hv7B2LhF4DNhjYwDANSun94UNULGCKB5FMCzpDZK9F3JmM33mYnLO54D9GYPARegz9xMmkIRXfFacOtQ9bRoKpb7bx2lLTGYfIJIaewHASzAr89sV03kvlW+qXxwu6sxDKavh6oCd4n5ZA+C2ewPAYj8x/zDtgHJPUE4P5mUMkhlnkZnTp+ZPYzPA6ugmKQHAaKoImd3ldh1PsHlPrPZeIUXcHk/UUaYRFzTwGauttgGAQAAAAAAAACgAwAAkh+6NpRuGihs2V9UQgLed+6jgxZ+0vX91pQxGFSp52PCXNiGGZyPq1cAPwhKX0aoBvd2kG1rl9vBDAOKqILRFOT0q+v2iUij/hl4oBFb7W0+lSES93K8myUv8yvZpY811ucnwNP9pVnwKa+LZQPvhUqus4ZpOujQTp28wz1bJWpIZTqPPwM66XVg8PW7fKhmSghy18WzvxZmYTDwXP1YCGuUhw6eEJ5Rdw7N3ObP5X16/XYBAHFrPIvvg1pXcWQCSiqddC/rvIO539ctAhEXSTv9J+E6NWV7OqaS//FyVwOeomcb3YWvTZb6tuM6wV/q+4pt/d4TOvo4Z4a9Z+OWVH7bL8gsDQ8laWhejMQyqvkwLcfv8Y99MYXSqXrbUWhJThraKQ7mZozon+8v+NH+m3AnAG5U497++RTB6/C+clwkGdQX9Un/k3opCJcXlix5eCuOtpFkbl85BMR7D90aYFBv8EUv2+qmpTQuDiyUM3IM7C8nRd2MDN86pdxCGuE1csTwJl0dWCwBBouDm4mf+yrLAbqDEWAjm0Ggl5KXlFHUPwfws/WzpAllxhWmGTrfYCse3XkcJpyz5TjWgJR8KTwWB1wQbVmVIeA49/x/H4y9Snh8PnZry1sFH12hljdd2JUmJvr1y9t1Q9EVeSMIZTXgL+98OQQ8hJ1ETCMFNRjkaFhELg45Y9jU73ByQO48FZa8GrY9+I0Xz+Wl0Lb0EUAKsZZt+lz5c07r0ZgUxdQLMwdcqt2gyc+eC41Ykjo49Ds5hop0qZNZbrKL+SOplWSQQAoCE20vKnOS1RlWxFx+Se4MK8uyDIOajeiUFGJxyLbj41TqQXLz1RTREM9HeMYxMjpFjksGPd8Zxk91fQNTjFhgbwaCvuM6rPXSsEJbnm/A4dMQrccLJqXoOlfbqJr8li9Wp9VDeAAQMEv4816kovcdORMGyR4mZkGXZwZRR4szkcYuN43VX0NwGUMiF8LxCWRGc/rE3NIMLlJ097DhKC/Pe7VqIpF2aJ7Gf8FXxlLIjXo0iclNRrxh6NDSOFIyv3zAL4mOHp1l+PTyIxY06nzfuq7E+GlTBZ2MhYn0bsqUgg1I44AYPu/F2TsZZ3RUf+IJqnhzQH5wWyc8HnflF3RdDJL/Ro5dFUgt/voNlx3DpAkv8Nikj922HmfA35pbeFiqcikepd5qsUe0lQQex4en4twNOFzwc5TOZ57WjzkM/oa4vHGuI3sDe24HCgQAAAAAAAAAZCeFii+jFqBpiI7g8FdPi168OU/+bYW1vv6Q0G/t+Q2lhPi7QCjYympjx1OdEiWEynvEC7vpF0CxJQRTzPXSARL3QQWEVkqd2Sa8VHwGIfTJCrlcSXnph+hMIL4yVO0J6fFZmtp1hG0+6BI23w8rAlOWzUIiq9udhFHv071pegdwo+4vcorNLM5ZivEi5oYmESHvZ+AVfDTVmN7Ewt4bAJpNa67bOWn6WKkL6dNxDShSG+Hkio1FB+umonMWL8SJ83iRQ5j5mECP4B2Da7Jqk1EXGNWrEnAzEN67gvAzQwZR62yNhNvfh3b4vjQnjbh8JHdhu8LBMxZc0eWBZ+KMBEHUKtKq60QrQVPXmDm3fn2ZOe3z4TUoovyOmHEzTQYNbxntFSfWeXiqVSMI5FAXy4htO7v/CCbqYH2lnUKuGgYMDYHdnDrTZIftZukfVibB9R4V51udTJmlnGmKtVpTBvnBZF4lWVvTL5/Iug86pyhQw3L8W5RQTKDU3X46p/UKqv7mBIoJXPEzQ3vKiINi0IpAdl0woD/osaelmMs5SViKua0AoPyoQSxLVEMD8IbYUj9Jq1arsa0SjrOFYwWBAQO+7yjoerLVP02r3GRm8idK1bO2C23sPNXcKKqbgTI3PbHmIiGiJfVF6iYFfBcYu82enWpTg+7FilCw+RPLLBttJy+raNzqv9hI65mcivrReM3IawxT15LXQggVUaJxSgPE+LNnnLoD9Ie8f8JvZmUx9BK4zAIg5pEiCkUbpJ8o36OgOQL9PbUfO7zwMpGr+adyVj6qfJRSgvIzDNbJ3gc9SQ0IZw3wswBtacvR+sESHYwwOKH8gFtYB8mJgMOGi6oqNqzcddSItG+y1OfaqkKljU2yPq6YAW5jgG7wEBQJ3sInMgh7CoOnYSl3+t+4D49+NiUZQT7Er5IVXqq2RIplaj8oYCySDhZSQZ9UDz1M3XPvNEpdRfdTsGb4vxGpBks/Esc96oviHgivvRTwd13MjEndtiGuqCXKUkYS8bEBwoE1RFwpIaid9xWDmIpCSx4YLZvmBnaaqlnoOUH6fQl0mQNXgTOwcj6XTfeEpkOWFFITgXwn9jPUs8MEmhh6jlPQd7KAWH7aQLagP+AiEKrqAU/pv4SXjwb7eylTzxAAXTYfZA/fZ3S46Oh8pz61HlQN/3ySIaVoER/Gw9an0Yi84MOmxVD3f5QWR3ybTjkgf5TgicrOKV0py2IP24lVHSHPr7nzYgJVa4oMvmt8RpGslqjoDTO6hbI4M6fI60s+axT3xZ6ZG6qtoAzpdTLDiVr7ofWpSspMkvP4Gu5VeCxe31PImsb37LHkQkSQD/8M4TtVKW4SXBTrT1x4/6eAaOZwvu5+QibEHoXz3x35bx0M6JphJa+P6Ri9SslNzdhJSQy79EbKLzuj5u2NwxWf93MHO5v8PhxEgtU47AaJ1iGX6CNbsW7Xvvad0KARNkpCPbX1WlWF5JWRnpT5BZODNqu5Pul6i2PiJLl5icgTeWAm30etJ7QGQSzX6tQvBUZ+xBD3PBFOVGUOOsWBkyxLVy0NizKTnhBlQtJqUu2JFgHuHvUrSFqwQfcqEXcWBhoDTzdWRS89QyL+nMjUZZ0mAZJtJ4Zt4zdsCkjRHOmA5NA5RqfmEwKT69GhjCn4b5cInET9cxuFH+wmZfSkcCNYOZLnMoG6gcTCil6jJKHs5Y5ZpNihMC8XOtIkFlbBWnqgxuaDTnWe83cnoCCcZdj0B/eoJ/eoD4CJnSqwUEwtVAj2AiyQNAGRMIeHQf1LR0iJOYKr+zKFVED3LNWEddeRRKtkCPyI1IY8BKbv5JavRQGCDdk/gaSXJSJfleMUD7dHC1HBkSKYkFpWxjALsSRbjYQl7OayXcwtnmINPZgjFfdqcXymxxhOtQ3ds1bQeJQDHLxcMD2G5RurlFRGwruqyOQKxKqWUyyR2AGGq/Wn4wWw8OgSW2SpMnFdH30uMnDhHBvlzDMwWJ1FtZTFnGHEc9kOBFj/Ec09F/+MtzRFQ7W2z0uBehrXXCuVUJTXVidJLNikwwgMnaB3XHgREUKepJ2NudxHPSGs9KRDTDA+KnVeWzVkz0B37l8GCNtY/MBYOYeLSCackhipY5YiF0cGKThPmuJoUlLTgcTrZgCT3CqA7tC+ze+Pkr5hyNXBPCp8Q8qALSWO/WyoerVfANeARPPs2MLQvy5/pVb/0Diw5FCAuYDDkUKEG51jxbO+gTM+nSDob6fnm93iC6ObtrCwADa6dClvn8Lp35o1q2UN/PG5T+1ibDwRlW3O/bCo38lL8n/LxFAURZ7CIjXYnzhmWrTO1anC2pDlrvRrrPXvQgQr0BM11TjrxCUg2l3WGbNKa/mr+ViVTViaJn2YZGZWiy3cs5lR7j6yfPjcrpllIdTfdKh7bqYLxGZVXr+mNjwKvSs3YeRXJebSs15tlgyz7wYATtphDd72YU28vKxeVIpj6hLTYV2GuRcvZMYh036krXp9DamCdpBBG0y/ujXsAz4UqVQnF641OyyY4wu7eaLm/LVvmbzUNyBDaXKL76UDbvAS57OFGogvuh+TQFztwyBn+gxq9bi6LcQC79+NqQrnPDkbywE9uezlQRMeZ0ntQ9aV5rRqb7B11eIYwBShjjQpnYkImtoVFsoQOgU2nOVRmaSa/YST/KVc359+A+8AEU3OB44EgLGuUS9kjaE3W+6BRXSDvIzaMWxm8c9m1gSELJUWhVobyLDyD4NqbPR3qDQmoKUZoQam+7kG8qTFc4DApEzkTU+R295Ou50kRYRaegXRQm38zCMrR5MBwiQkkD9PsT24aVrLMqIFUkMXNESQWCdzqsU/rV1z8T6T8U+VMDdBKT/9r4JWoba2ImTDS9i+Y7P6HtS45vJZ3X3fLvi01jZpSqL495SQq31IRdRO0xpwQmCU05s5pg3GNyIet5b93yfErgHh/Kzrm0kkgx2raDRkvo+FmirYxHxnSAc8tXex5KgfEt+5BiG0K0KVq9pnwwKOmV4K9+0uOeTvSM0YzGKcSAHkdN198kiJQENm3z1m/UyqHNi40ycchSEDAQAAAAAAAABkJ4WKL6MWoGmIjuDwV0+LXrw5T/5thbW+/pDQb+35DaWE+LtAKNjKamPHU50SJYTKe8QLu+kXQLElBFPM9dIB"
    )]
    #[serde(with = "as_base64")]
    proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/remove-member",
    request_body = RemoveMember,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn remove_member(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<RemoveMember>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    let art = state
        .art_service
        .get_art(&payload.chat_id, None)
        .await?
        .art;

    let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

    let remove_member_message = ProofVerifierMessage::RemoveMember {
        proof: payload.proof.clone(),
        co_path,
        associated_data: art.serialize()?,
    };

    match callback(&state.proof_verifier_sender, remove_member_message).await {
        Ok(message) => {
            let ProofVerifierResult::RemoveMember { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    state
        .art_service
        .remove_member(
            &payload.chat_id,
            &branch_changes,
            &ProofRecord::RemoveMember {
                proof: payload.proof,
            },
        )
        .await?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKey {
    /// Serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"AogCBAAAAAAAAAD55GV+qsCIdq7XQocrftP67C2v+IzRh/2bCusqV/vTBT5mt6GohPQVTqKwwq8XbmK+q4SK5t+lblT6+aLF52+J9UoPL4SNMEtSwwBTv0ogZ7RDvzc1qlgapuQuwcBZrw1R9f9B3pf/4T2gp1eWz09JTmw2eoSGwMCsmlofQj9/BXMQjS0HYKiqp7A54v7YXC+ptl7n5A1xLmF3vb8tFDQNPDR0TIypJKk0y5UoKK8OMt9MDapD3Q9DCnfewAOhb4tkJ4WKL6MWoGmIjuDwV0+LXrw5T/5thbW+/pDQb+35DaWE+LtAKNjKamPHU50SJYTKe8QLu+kXQLElBFPM9dIBAAo="#
    )]
    #[serde(with = "as_base64")]
    branch_changes: Vec<u8>,

    /// Serialised proof.
    #[schema(
        example = "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"
    )]
    #[serde(with = "as_base64")]
    proof: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/update-key",
    request_body = UpdateKey,
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn update_key(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Json(payload): Json<UpdateKey>,
) -> Result<StatusCode, ApiError> {
    payload.validate()?;

    let branch_changes = decode_branch_changes(&payload.branch_changes)?;

    let art = state
        .art_service
        .get_art(&payload.chat_id, None)
        .await?
        .art;

    let associated_data = art.serialize()?;
    let co_path = art.get_co_path_values(&branch_changes.node_index.get_path()?)?;

    let key_update_message = ProofVerifierMessage::KeyUpdate {
        proof: payload.proof.clone(),
        co_path,
        associated_data,
    };

    match callback(&state.proof_verifier_sender, key_update_message).await {
        Ok(message) => {
            let ProofVerifierResult::KeyUpdate { verdict } = message else {
                return Err(ApiError::InternalServerError(
                    "Invalid message from proof verifier".to_string(),
                ));
            };

            if !verdict {
                return Err(ApiError::BadRequest("Invalid proof".to_string()));
            }
        }
        Err(e) => {
            error!("Failed to send message to proof verifier: {}", e);
            return Err(ApiError::InternalServerError(e.to_string()));
        }
    };

    state
        .art_service
        .update_key(
            &payload.chat_id,
            &branch_changes,
            &ProofRecord::AddMember {
                proof: payload.proof,
            },
        )
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetChangesQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Number of results to be returned.
    #[param(example = 10)]
    pub limit: i64,

    /// The amount or results to skip at first.
    #[param(example = 0)]
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/changes",
    params(GetChangesQuery),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn get_changes(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<GetChangesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    let filter = doc! {};
    let changes = state
        .art_service
        .list_changes(&payload.chat_id, filter, payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!("Found changes: {}", changes.len());

    Ok((
        StatusCode::OK,
        postcard::to_allocvec(&changes)
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?,
    ))
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteChatQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/chat",
    params(DeleteChatQuery,),
    tag = "Chat operations"
)]
#[instrument(skip(state, _headers), err)]
pub async fn delete_chat(
    State(state): State<Arc<Container>>,
    _headers: HeaderMap,
    Query(payload): Query<DeleteChatQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload.validate()?;

    state
        .art_service
        .delete_chat(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::OK)
}
