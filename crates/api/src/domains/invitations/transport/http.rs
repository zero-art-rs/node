use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{SeedableRng, rngs::StdRng};
use ark_std::{One, UniformRand, Zero};
use axum::extract::{Path, Query};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{self, HeaderMap, Response, StatusCode},
    response::IntoResponse,
};
use base64::prelude::*;
use mongodb::bson::{Uuid, doc, spec::BinarySubtype};
use postcard;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

use crate::{container::Container, errors::ApiError};
use postcard::to_allocvec;
use types::InvitationRecord;
use zk::curve::cortado::CortadoAffine as ARTGroup;

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddInvitationsRequest {
    /// serialised vector of invitations
    #[schema(
        example = r#"[8, 64, 149, 231, 1, 97, 68, 61, 201, 200, 191, 43, 15, 83, 93, 88, 40, 38, 66, 34, 246, 73, 52, 80, 85, 238, 179, 110, 183, 133, 216, 31, 226, 10, 128, 125, 131, 104, 129, 69, 211, 81, 70, 166, 212, 243, 105, 188, 213, 250, 130, 119, 224, 64, 192, 17, 249, 163, 103, 130, 217, 202, 127, 90, 226, 6, 32, 52, 48, 207, 33, 110, 41, 113, 121, 170, 66, 81, 40, 27, 36, 49, 232, 182, 86, 242, 115, 132, 54, 38, 27, 132, 12, 7, 174, 142, 220, 87, 14, 64, 242, 198, 74, 27, 253, 113, 110, 6, 48, 190, 51, 120, 149, 36, 148, 181, 84, 197, 117, 249, 104, 202, 2, 139, 215, 93, 9, 34, 200, 244, 47, 5, 12, 149, 189, 9, 207, 240, 26, 150, 123, 232, 201, 18, 250, 153, 252, 64, 16, 57, 230, 96, 70, 243, 249, 200, 211, 2, 242, 102, 103, 154, 227, 142, 32, 69, 140, 149, 76, 204, 147, 5, 101, 37, 163, 75, 99, 14, 218, 64, 99, 210, 240, 149, 36, 77, 20, 227, 184, 126, 112, 4, 191, 53, 77, 206, 4, 64, 80, 189, 221, 249, 108, 65, 207, 56, 21, 237, 170, 14, 103, 167, 29, 95, 73, 114, 27, 26, 126, 7, 60, 228, 88, 84, 167, 134, 81, 154, 116, 12, 165, 61, 47, 211, 51, 87, 215, 72, 127, 38, 170, 180, 189, 24, 212, 36, 153, 253, 160, 59, 245, 178, 156, 175, 167, 186, 230, 13, 253, 49, 65, 139, 32, 173, 141, 58, 203, 20, 20, 144, 111, 157, 136, 58, 87, 19, 117, 61, 152, 173, 192, 87, 190, 133, 217, 242, 45, 9, 64, 76, 142, 238, 126, 91, 4, 64, 40, 7, 153, 71, 186, 157, 126, 14, 137, 72, 67, 221, 56, 107, 234, 56, 162, 101, 79, 225, 254, 91, 112, 121, 91, 93, 28, 69, 189, 160, 188, 8, 254, 201, 35, 235, 197, 67, 205, 155, 205, 68, 115, 34, 11, 226, 14, 94, 224, 10, 142, 204, 95, 57, 110, 71, 186, 191, 193, 5, 226, 244, 25, 5, 32, 10, 205, 9, 230, 248, 210, 166, 232, 134, 188, 51, 150, 74, 221, 58, 88, 169, 38, 51, 111, 143, 77, 138, 60, 92, 203, 250, 173, 212, 135, 130, 5, 64, 55, 8, 47, 115, 230, 137, 108, 25, 112, 54, 138, 111, 213, 145, 105, 74, 4, 99, 158, 130, 30, 246, 143, 40, 240, 132, 148, 154, 223, 24, 77, 8, 177, 236, 186, 230, 146, 68, 163, 121, 111, 117, 120, 217, 75, 254, 138, 142, 122, 74, 159, 252, 72, 128, 200, 111, 40, 139, 162, 48, 203, 232, 108, 6, 32, 80, 105, 204, 251, 87, 221, 190, 64, 37, 250, 183, 0, 252, 80, 150, 180, 81, 127, 46, 167, 111, 115, 242, 213, 67, 202, 145, 45, 221, 6, 85, 4, 64, 147, 127, 50, 146, 218, 60, 16, 18, 28, 167, 45, 138, 89, 34, 70, 74, 152, 24, 80, 24, 237, 23, 184, 206, 159, 119, 21, 43, 13, 154, 142, 1, 143, 138, 129, 45, 159, 175, 114, 23, 103, 217, 190, 242, 109, 133, 148, 84, 68, 223, 242, 222, 129, 243, 61, 172, 19, 190, 4, 233, 152, 109, 190, 138, 32, 230, 196, 15, 187, 201, 45, 233, 201, 45, 42, 2, 67, 21, 110, 105, 23, 42, 82, 74, 214, 193, 180, 117, 88, 171, 106, 211, 219, 92, 212, 88, 13, 64, 249, 118, 48, 238, 19, 85, 58, 221, 25, 103, 52, 133, 51, 196, 61, 144, 123, 37, 22, 31, 60, 188, 19, 192, 114, 150, 70, 186, 31, 55, 212, 10, 60, 127, 215, 122, 35, 109, 178, 210, 234, 61, 191, 53, 110, 210, 33, 244, 179, 154, 156, 21, 217, 204, 199, 209, 81, 85, 254, 84, 148, 36, 170, 141, 32, 252, 227, 212, 226, 125, 187, 60, 18, 120, 85, 88, 69, 206, 39, 232, 215, 3, 90, 125, 180, 39, 253, 85, 141, 41, 204, 72, 135, 230, 174, 125, 12, 64, 126, 85, 125, 117, 100, 31, 179, 138, 176, 248, 177, 175, 231, 64, 172, 71, 252, 119, 195, 45, 144, 231, 226, 149, 80, 4, 196, 57, 202, 251, 132, 14, 29, 6, 70, 242, 239, 113, 28, 204, 249, 76, 7, 195, 195, 239, 36, 250, 53, 254, 237, 92, 31, 178, 139, 32, 16, 180, 214, 183, 61, 36, 111, 143, 32, 203, 98, 130, 230, 85, 183, 246, 80, 48, 209, 146, 251, 21, 208, 181, 72, 122, 245, 92, 87, 187, 38, 105, 100, 124, 57, 133, 39, 77, 197, 125, 0]"#
    )]
    invitations: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/invitations/init",
    request_body = AddInvitationsRequest,
    security(("bearer_auth" = [])),
    tag = "Invite operation"
)]
#[instrument(skip(state, headers), err)]
pub async fn add_invitations(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<AddInvitationsRequest>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let invitations = postcard::from_bytes::<Vec<InvitationRecord<ARTGroup>>>(&payload.invitations)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .invitation_service
        .store_invitations(&payload.chat_id, &invitations)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!("Successfully added new invitations.");

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddInvitationRequest {
    /// Serialised vector of Invitations for a new users.
    #[schema(
        example = r#"[64, 30, 90, 205, 55, 133, 143, 219, 75, 66, 181, 85, 220, 145, 7, 46, 79, 2, 111, 103, 197, 211, 111, 132, 29, 64, 211, 74, 218, 235, 68, 34, 3, 154, 215, 251, 198, 199, 241, 60, 22, 121, 237, 115, 232, 193, 65, 75, 253, 190, 107, 152, 232, 21, 205, 14, 61, 180, 178, 144, 25, 241, 81, 217, 138, 32, 203, 194, 156, 91, 134, 206, 248, 31, 31, 199, 1, 172, 202, 28, 82, 36, 120, 13, 188, 119, 216, 44, 216, 86, 110, 217, 116, 38, 196, 148, 177, 0]"#
    )]
    invitation: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/invitations",
    request_body = AddInvitationRequest,
    security(("bearer_auth" = [])),
    tag = "Invite operation"
)]
#[instrument(skip(state, headers), err)]
pub async fn add_member_invitation(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<AddInvitationRequest>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let invitation = postcard::from_bytes::<InvitationRecord<ARTGroup>>(&payload.invitation)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .invitation_service
        .add_invitation(&payload.chat_id, invitation)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!("Successfully added new invitation.");

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetInviteQuery {
    /// New user public key. It is a base64 encoded ark-serialise serialiseed AffineRepr.
    #[into_params(parameter_in = Query)]
    #[param(
        example = r#"HlrNN4WP20tCtVXckQcuTwJvZ8XTb4QdQNNK2utEIgOa1/vGx/E8Fnntc+jBQUv9vmuY6BXNDj20spAZ8VHZig=="#
    )]
    pub receiver: String,

    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/invitations",
    params(
        GetInviteQuery
    ),
    security(("bearer_auth" = [])),
    tag = "Invite operation"
)]
#[instrument(skip(state, headers), err)]
pub async fn get_invitation(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<GetInviteQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let receiver = BASE64_STANDARD
        .decode(payload.receiver)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let invitation = to_allocvec(
        &state
            .invitation_service
            .get_invitation(&payload.chat_id, &receiver)
            .await
            .map_err(|e| ApiError::InternalServerError(e.to_string()))?,
    )
    .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let response = (StatusCode::OK, invitation);

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteInviteQuery {
    /// New user public key. It is a base64 encoded ark-serialise serialiseed AffineRepr.
    #[into_params(parameter_in = Query)]
    #[param(
        example = r#"HlrNN4WP20tCtVXckQcuTwJvZ8XTb4QdQNNK2utEIgOa1/vGx/E8Fnntc+jBQUv9vmuY6BXNDj20spAZ8VHZig=="#
    )]
    pub receiver: String,

    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/invitations",
    params(
        DeleteInviteQuery
    ),
    security(("bearer_auth" = [])),
    tag = "Invite operation"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_invitation(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteInviteQuery>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let receiver = BASE64_STANDARD
        .decode(payload.receiver)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .invitation_service
        .delete_invitation(&payload.chat_id, &receiver)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::OK)
}
