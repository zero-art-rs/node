use crate::{container::Container, errors::ApiError};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::rand::{SeedableRng, rngs::StdRng};
use ark_std::{One, UniformRand, Zero};
use art::{ART, BranchChanges, BranchChangesType};
use axum::extract::{Path, Query};
use axum::{
    Json,
    body::Body,
    extract::State,
    http::{self, HeaderMap, Response, StatusCode},
    response::IntoResponse,
};
use mongodb::bson::{Uuid, doc, spec::BinarySubtype};
use postcard;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{info, instrument};
use utoipa::{IntoParams, ToSchema};
use validator::Validate;
use zk::curve::cortado::CortadoAffine as ARTGroup;

// ########################################
// init group
// ########################################

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequestPhase1 {
    /// Postman serialised art structure for new chat
    #[schema(
        example = r#"[64, 118, 143, 155, 196, 131, 198, 253, 246, 182, 95, 154, 210, 152, 134, 205, 236, 92, 238, 148, 9, 22, 48, 138, 178, 133, 21, 148, 133, 205, 152, 119, 6, 53, 133, 65, 42, 144, 255, 77, 145, 19, 217, 184, 150, 142, 96, 48, 202, 35, 46, 219, 227, 244, 82, 168, 198, 67, 63, 91, 132, 82, 151, 143, 7, 1, 64, 47, 140, 140, 81, 124, 83, 158, 216, 138, 223, 100, 0, 84, 122, 112, 58, 54, 107, 249, 23, 162, 117, 132, 244, 213, 51, 199, 196, 147, 231, 190, 4, 95, 161, 36, 60, 26, 81, 7, 71, 121, 31, 83, 227, 127, 39, 186, 7, 16, 70, 22, 177, 38, 86, 107, 71, 199, 65, 51, 173, 156, 189, 72, 0, 1, 64, 83, 116, 253, 140, 77, 203, 234, 251, 156, 146, 251, 79, 165, 229, 145, 128, 240, 188, 168, 7, 186, 48, 229, 31, 87, 213, 205, 7, 203, 64, 121, 2, 46, 90, 32, 122, 177, 8, 44, 149, 210, 21, 214, 26, 80, 114, 129, 239, 184, 189, 164, 228, 11, 201, 217, 101, 239, 81, 168, 147, 192, 160, 125, 142, 1, 64, 133, 27, 70, 77, 120, 29, 81, 255, 194, 184, 96, 200, 246, 85, 86, 17, 14, 59, 236, 124, 148, 242, 202, 176, 104, 68, 199, 157, 38, 252, 98, 9, 23, 226, 117, 38, 206, 228, 115, 98, 24, 7, 231, 156, 180, 85, 163, 88, 149, 120, 28, 22, 67, 168, 185, 94, 14, 211, 196, 122, 59, 225, 108, 0, 0, 0, 0, 1, 1, 64, 126, 183, 224, 25, 197, 235, 238, 134, 227, 164, 172, 88, 54, 141, 236, 59, 52, 123, 121, 214, 93, 154, 31, 155, 100, 237, 172, 186, 216, 68, 102, 9, 175, 17, 93, 209, 47, 73, 70, 100, 57, 217, 213, 62, 236, 62, 214, 149, 2, 166, 205, 87, 180, 79, 86, 181, 223, 227, 164, 132, 190, 49, 178, 137, 0, 0, 0, 1, 0, 2, 1, 64, 60, 130, 155, 87, 40, 103, 225, 139, 207, 42, 78, 241, 152, 116, 28, 252, 0, 69, 254, 122, 36, 169, 52, 186, 231, 225, 131, 204, 166, 57, 208, 3, 246, 210, 159, 230, 143, 6, 196, 159, 253, 8, 146, 158, 121, 46, 144, 175, 204, 37, 12, 109, 239, 79, 146, 69, 10, 24, 126, 252, 195, 181, 26, 1, 1, 64, 239, 112, 226, 191, 47, 202, 63, 226, 81, 147, 121, 115, 29, 121, 250, 50, 232, 62, 27, 246, 254, 120, 214, 34, 195, 61, 97, 60, 60, 159, 75, 12, 146, 20, 52, 160, 239, 94, 74, 0, 251, 48, 62, 202, 190, 108, 65, 4, 122, 110, 214, 209, 243, 2, 112, 213, 49, 210, 6, 38, 32, 231, 211, 138, 0, 0, 0, 1, 1, 64, 27, 157, 121, 11, 222, 246, 35, 161, 166, 239, 240, 84, 7, 117, 20, 131, 191, 204, 210, 153, 37, 210, 144, 226, 233, 167, 73, 249, 246, 187, 78, 13, 6, 114, 36, 247, 49, 49, 56, 144, 169, 150, 83, 158, 144, 23, 216, 125, 151, 146, 30, 78, 248, 110, 104, 70, 93, 202, 25, 163, 127, 6, 112, 140, 0, 0, 0, 1, 0, 2, 0, 4, 1, 64, 229, 95, 25, 205, 242, 90, 198, 212, 213, 191, 226, 116, 76, 2, 89, 62, 22, 107, 173, 201, 249, 144, 69, 135, 131, 57, 113, 147, 77, 198, 205, 9, 169, 123, 175, 21, 42, 13, 164, 11, 107, 133, 23, 95, 118, 232, 135, 17, 120, 225, 62, 200, 55, 216, 173, 126, 17, 118, 51, 126, 159, 71, 209, 136, 1, 64, 14, 251, 72, 244, 254, 132, 55, 126, 61, 180, 196, 1, 97, 16, 111, 216, 103, 160, 221, 139, 206, 93, 15, 182, 55, 119, 172, 157, 143, 202, 159, 3, 45, 25, 231, 99, 224, 99, 173, 0, 145, 133, 223, 143, 33, 244, 216, 224, 143, 98, 206, 15, 252, 12, 101, 27, 171, 139, 211, 249, 180, 253, 52, 1, 1, 64, 149, 223, 132, 114, 249, 201, 189, 201, 249, 28, 148, 247, 128, 171, 72, 232, 27, 177, 23, 176, 186, 182, 249, 234, 126, 28, 4, 109, 12, 179, 239, 1, 196, 136, 142, 195, 116, 171, 30, 172, 228, 221, 4, 7, 91, 195, 227, 97, 148, 75, 23, 244, 135, 149, 155, 102, 81, 191, 61, 216, 189, 200, 81, 2, 0, 0, 0, 1, 1, 64, 231, 149, 41, 39, 123, 118, 169, 42, 47, 157, 245, 184, 48, 188, 19, 82, 20, 222, 71, 79, 66, 143, 44, 101, 50, 4, 54, 9, 245, 179, 61, 9, 124, 148, 193, 8, 189, 27, 37, 135, 238, 29, 158, 163, 23, 239, 247, 234, 115, 21, 253, 151, 125, 123, 242, 53, 102, 189, 203, 81, 186, 115, 135, 3, 0, 0, 0, 1, 0, 2, 1, 64, 25, 90, 31, 64, 88, 5, 182, 177, 137, 178, 98, 8, 168, 240, 153, 158, 219, 163, 133, 23, 158, 245, 175, 165, 56, 70, 99, 248, 174, 173, 165, 13, 46, 36, 107, 103, 30, 65, 241, 230, 84, 218, 233, 184, 77, 189, 39, 57, 67, 92, 76, 86, 187, 66, 118, 150, 132, 143, 153, 194, 185, 52, 239, 4, 1, 64, 13, 84, 111, 165, 221, 116, 4, 149, 35, 2, 45, 237, 173, 222, 66, 161, 128, 47, 111, 120, 2, 247, 122, 79, 92, 30, 215, 191, 67, 14, 47, 6, 216, 87, 58, 23, 38, 13, 50, 246, 188, 154, 58, 108, 30, 63, 232, 197, 103, 61, 225, 124, 16, 254, 88, 103, 233, 155, 206, 221, 215, 43, 119, 6, 0, 0, 0, 1, 1, 64, 182, 172, 103, 122, 153, 127, 224, 169, 132, 34, 0, 52, 158, 136, 249, 211, 229, 163, 131, 164, 41, 146, 19, 92, 186, 50, 142, 162, 76, 112, 139, 5, 186, 37, 165, 74, 168, 175, 55, 181, 177, 93, 26, 155, 23, 182, 22, 137, 168, 19, 204, 51, 54, 248, 99, 131, 105, 106, 18, 175, 234, 80, 99, 3, 0, 0, 0, 1, 0, 2, 0, 4, 0, 8, 64, 22, 244, 156, 151, 240, 52, 186, 62, 131, 201, 148, 9, 140, 40, 123, 188, 71, 49, 218, 14, 31, 201, 65, 80, 246, 210, 196, 99, 184, 101, 78, 11, 228, 198, 239, 125, 93, 107, 124, 64, 185, 17, 37, 99, 218, 201, 16, 12, 183, 132, 48, 197, 157, 65, 225, 253, 184, 205, 141, 66, 95, 213, 110, 139]"#
    )]
    art: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/init-group/phase1",
    request_body = InitChatRequestPhase1,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn init_chat_phase1(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<InitChatRequestPhase1>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = ART::deserialize_with_postcard(&payload.art).unwrap();

    state
        .art_service
        .init_chat(&payload.chat_id, art)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    info!("Successful phase1 of init-chat request");

    Ok(StatusCode::CREATED)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct InitChatQueryPhase2 {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/init-group/phase2",
    params(
        InitChatQueryPhase2,
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn init_chat_phase2(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<InitChatQueryPhase2>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = state
        .art_service
        .get_art(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    match art.art.serialise_with_postcard() {
        Ok(art) => {
            let response = (StatusCode::OK, art);
            Ok(response)
        }
        Err(e) => Err(ApiError::InternalServerError(e.to_string())),
    }
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InitChatRequestPhase3 {
    /// Postman serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"[2, 136, 2, 4, 0, 0, 0, 0, 0, 0, 0, 224, 230, 230, 120, 22, 40, 196, 89, 141, 246, 66, 1, 246, 189, 25, 51, 123, 50, 13, 117, 215, 177, 93, 249, 207, 104, 50, 246, 252, 99, 136, 9, 240, 214, 252, 86, 133, 211, 73, 142, 183, 152, 249, 121, 248, 25, 74, 135, 186, 151, 147, 60, 160, 97, 19, 253, 104, 24, 53, 44, 242, 5, 131, 140, 102, 26, 63, 143, 180, 223, 138, 78, 32, 132, 50, 203, 121, 144, 196, 163, 29, 159, 175, 157, 168, 100, 60, 160, 71, 248, 122, 143, 53, 230, 99, 11, 152, 64, 103, 68, 21, 194, 251, 154, 244, 193, 73, 194, 120, 0, 7, 59, 2, 133, 5, 23, 26, 171, 138, 56, 109, 27, 42, 119, 131, 111, 185, 2, 234, 88, 104, 94, 239, 52, 149, 63, 28, 148, 191, 36, 203, 194, 159, 72, 9, 117, 5, 159, 83, 116, 176, 219, 232, 166, 134, 122, 249, 35, 247, 1, 245, 167, 118, 34, 107, 190, 120, 125, 194, 228, 72, 255, 108, 200, 95, 171, 83, 157, 139, 63, 176, 134, 247, 71, 116, 72, 197, 224, 11, 112, 90, 138, 199, 18, 199, 65, 66, 73, 226, 151, 32, 25, 51, 39, 119, 40, 74, 60, 7, 128, 137, 211, 167, 105, 99, 95, 13, 214, 51, 118, 208, 218, 252, 11, 72, 19, 65, 35, 16, 50, 18, 149, 165, 6, 97, 153, 37, 94, 45, 203, 250, 225, 47, 140, 227, 127, 31, 107, 103, 59, 124, 169, 87, 239, 167, 142, 3, 1, 1, 2]"#
    )]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/init-group/phase3",
    request_body = InitChatRequestPhase3,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn init_chat_phase3(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<InitChatRequestPhase3>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = postcard::from_bytes::<BranchChanges<ARTGroup>>(&payload.branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    match branch_changes.change_type {
        BranchChangesType::UpdateKeys => {
            state
                .art_service
                .update_art(&payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Chat isn't fully initialized yet".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

// ########################################
// add member
// ########################################

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberPhase1 {
    /// Postman serialised BranchChanges:AppendNode structure
    #[schema(
        example = r#"[1, 64, 224, 72, 230, 124, 160, 0, 175, 82, 48, 247, 82, 98, 115, 95, 104, 91, 161, 251, 13, 20, 247, 199, 202, 48, 112, 13, 70, 64, 91, 108, 163, 10, 142, 40, 186, 25, 100, 95, 10, 199, 248, 95, 223, 61, 128, 34, 141, 11, 197, 47, 21, 91, 33, 180, 198, 239, 59, 4, 248, 93, 99, 249, 240, 138, 0, 0, 0, 1, 200, 2, 5, 0, 0, 0, 0, 0, 0, 0, 104, 72, 43, 14, 119, 223, 224, 69, 180, 15, 138, 187, 194, 160, 242, 191, 213, 151, 131, 231, 14, 182, 26, 14, 225, 202, 22, 135, 242, 184, 199, 14, 112, 106, 11, 133, 106, 1, 200, 124, 120, 228, 249, 34, 205, 209, 54, 33, 128, 77, 194, 206, 206, 228, 132, 84, 113, 150, 193, 182, 180, 92, 23, 143, 118, 221, 106, 215, 209, 213, 131, 196, 35, 68, 136, 121, 253, 169, 94, 208, 42, 47, 183, 196, 136, 148, 162, 44, 156, 192, 24, 185, 94, 194, 164, 7, 172, 57, 49, 151, 91, 30, 190, 127, 249, 155, 120, 49, 148, 200, 128, 69, 24, 146, 111, 1, 169, 178, 22, 246, 9, 128, 50, 243, 75, 155, 124, 143, 16, 31, 32, 157, 149, 26, 62, 251, 214, 208, 157, 55, 156, 33, 184, 73, 142, 229, 225, 76, 180, 8, 86, 134, 235, 32, 100, 250, 33, 91, 55, 8, 128, 130, 207, 135, 232, 73, 238, 133, 113, 122, 186, 250, 236, 24, 232, 254, 92, 134, 109, 55, 204, 64, 156, 214, 240, 60, 32, 121, 138, 163, 76, 6, 149, 39, 37, 197, 195, 105, 206, 147, 242, 213, 12, 11, 241, 156, 140, 40, 161, 16, 251, 249, 12, 184, 238, 154, 212, 132, 130, 169, 89, 150, 239, 13, 28, 65, 228, 177, 134, 188, 101, 199, 229, 103, 57, 81, 219, 165, 241, 251, 217, 170, 117, 170, 100, 124, 206, 178, 20, 245, 26, 98, 210, 122, 23, 140, 224, 72, 230, 124, 160, 0, 175, 82, 48, 247, 82, 98, 115, 95, 104, 91, 161, 251, 13, 20, 247, 199, 202, 48, 112, 13, 70, 64, 91, 108, 163, 10, 142, 40, 186, 25, 100, 95, 10, 199, 248, 95, 223, 61, 128, 34, 141, 11, 197, 47, 21, 91, 33, 180, 198, 239, 59, 4, 248, 93, 99, 249, 240, 138, 4, 1, 1, 1, 2]"#
    )]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/add-member/phase1",
    request_body = AddMemberPhase1,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn add_member_phase1(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<AddMemberPhase1>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = postcard::from_bytes::<BranchChanges<ARTGroup>>(&payload.branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    match branch_changes.change_type {
        BranchChangesType::AppendNode(_) => {
            state
                .art_service
                .add_user(&payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected new member addition".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberQueryPhase2 {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/add-member/phase2",
    params(
        AddMemberQueryPhase2,
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn add_member_phase2(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<AddMemberQueryPhase2>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let art = state
        .art_service
        .get_art(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    // let result = (StatusCode::OK, art.serialise_with_postcard()?);

    match art.art.serialise_with_postcard() {
        Ok(art) => {
            let response = (StatusCode::OK, art);
            Ok(response)
        }
        Err(e) => Err(ApiError::InternalServerError(e.to_string())),
    }
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AddMemberPhase3 {
    /// Postman serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"[2, 200, 2, 5, 0, 0, 0, 0, 0, 0, 0, 131, 71, 71, 43, 203, 50, 199, 154, 56, 249, 88, 235, 135, 198, 237, 83, 28, 227, 172, 64, 163, 76, 67, 132, 149, 60, 152, 101, 125, 91, 133, 2, 189, 198, 107, 240, 74, 28, 102, 61, 88, 185, 101, 230, 132, 177, 105, 170, 254, 69, 185, 122, 17, 87, 222, 46, 178, 46, 174, 16, 194, 201, 37, 7, 131, 171, 113, 235, 112, 192, 156, 110, 59, 102, 195, 152, 207, 89, 132, 221, 215, 93, 156, 193, 221, 205, 185, 35, 146, 102, 30, 120, 24, 75, 143, 4, 8, 237, 187, 125, 77, 56, 92, 177, 101, 218, 209, 94, 142, 187, 113, 188, 170, 157, 54, 157, 99, 72, 198, 99, 103, 126, 160, 140, 195, 130, 46, 139, 25, 230, 38, 94, 77, 87, 241, 101, 64, 206, 69, 108, 1, 37, 97, 95, 178, 99, 25, 76, 83, 220, 155, 181, 59, 149, 153, 87, 204, 132, 219, 1, 175, 180, 162, 145, 73, 35, 228, 139, 100, 227, 212, 46, 202, 218, 213, 85, 73, 0, 165, 79, 4, 93, 173, 142, 141, 156, 224, 100, 175, 200, 235, 6, 222, 203, 13, 200, 44, 17, 172, 77, 87, 51, 41, 3, 61, 107, 36, 139, 196, 243, 103, 67, 66, 136, 242, 173, 89, 53, 36, 13, 96, 187, 32, 13, 211, 6, 129, 116, 117, 107, 97, 127, 139, 215, 194, 136, 109, 88, 166, 41, 103, 10, 30, 79, 8, 61, 75, 54, 146, 68, 79, 110, 133, 229, 0, 7, 69, 131, 97, 202, 60, 113, 105, 180, 231, 37, 10, 200, 114, 89, 211, 159, 66, 83, 186, 86, 7, 153, 34, 19, 185, 207, 214, 213, 101, 88, 51, 11, 215, 209, 203, 173, 228, 111, 185, 60, 217, 48, 253, 125, 79, 31, 51, 16, 13, 215, 52, 80, 226, 47, 204, 97, 8, 61, 254, 10, 164, 4, 22, 143, 4, 1, 1, 1, 2]"#
    )]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/add-member/phase3",
    request_body = AddMemberPhase3,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn add_member_phase3(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<AddMemberPhase3>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = postcard::from_bytes::<BranchChanges<ARTGroup>>(&payload.branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    match branch_changes.change_type {
        BranchChangesType::UpdateKeys => {
            state
                .art_service
                .update_art(&payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected UpdateKeys".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

// ########################################
// remove member
// ########################################

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RemoveMember {
    /// Postman serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"[0, 64, 239, 112, 226, 191, 47, 202, 63, 226, 81, 147, 121, 115, 29, 121, 250, 50, 232, 62, 27, 246, 254, 120, 214, 34, 195, 61, 97, 60, 60, 159, 75, 12, 146, 20, 52, 160, 239, 94, 74, 0, 251, 48, 62, 202, 190, 108, 65, 4, 122, 110, 214, 209, 243, 2, 112, 213, 49, 210, 6, 38, 32, 231, 211, 138, 32, 203, 194, 156, 91, 134, 206, 248, 31, 31, 199, 1, 172, 202, 28, 82, 36, 120, 13, 188, 119, 216, 44, 216, 86, 110, 217, 116, 38, 196, 148, 177, 0, 136, 2, 4, 0, 0, 0, 0, 0, 0, 0, 138, 207, 70, 229, 90, 170, 123, 210, 3, 212, 144, 89, 204, 18, 228, 175, 176, 34, 182, 78, 66, 148, 181, 113, 53, 203, 126, 232, 149, 141, 156, 1, 65, 34, 78, 223, 107, 236, 31, 52, 33, 123, 251, 211, 235, 58, 88, 228, 48, 183, 229, 44, 184, 124, 174, 135, 220, 104, 234, 114, 74, 143, 142, 5, 45, 55, 60, 228, 82, 67, 39, 83, 113, 190, 210, 177, 195, 210, 60, 199, 157, 78, 68, 9, 133, 7, 184, 49, 9, 65, 188, 24, 48, 95, 3, 14, 85, 92, 150, 125, 19, 34, 139, 193, 45, 126, 191, 117, 53, 93, 226, 217, 243, 187, 202, 39, 112, 59, 227, 62, 19, 199, 145, 23, 147, 180, 246, 3, 184, 245, 87, 181, 77, 30, 246, 56, 106, 118, 52, 143, 133, 47, 119, 166, 57, 158, 49, 53, 231, 251, 34, 130, 39, 58, 124, 97, 81, 124, 24, 14, 76, 125, 118, 229, 155, 103, 113, 132, 150, 28, 67, 228, 34, 99, 209, 4, 247, 64, 96, 8, 62, 163, 17, 178, 132, 40, 189, 93, 145, 120, 1, 0, 227, 206, 108, 6, 161, 109, 0, 232, 191, 122, 167, 91, 48, 6, 75, 218, 169, 235, 70, 2, 96, 219, 97, 148, 10, 246, 218, 2, 172, 23, 60, 10, 96, 103, 38, 110, 233, 68, 87, 147, 115, 208, 5, 148, 125, 12, 233, 175, 249, 219, 105, 219, 57, 84, 171, 89, 202, 142, 56, 150, 44, 186, 193, 143, 3, 1, 2, 1]"#
    )]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/remove-member",
    request_body = RemoveMember,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn remove_member(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<RemoveMember>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = postcard::from_bytes::<BranchChanges<ARTGroup>>(&payload.branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    match branch_changes.change_type {
        BranchChangesType::MakeTemporal(_, _) => {
            state
                .art_service
                .update_art(&payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected RemoveNode".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct LeaveChat {
    /// Serialised art structure for new chat
    // #[schema(example = r#""#)]
    leave_message: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/leave",
    request_body = LeaveChat,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn leave_chat(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<LeaveChat>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    // state.messenger_service.send_message(payload.leave_message)

    Ok(StatusCode::OK)
}

// ########################################
// update-key
// ########################################

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UpdateKey {
    /// Postman serialised BranchChanges:UpdateKeys structure
    #[schema(
        example = r#"[2, 136, 2, 4, 0, 0, 0, 0, 0, 0, 0, 13, 98, 130, 96, 91, 70, 97, 151, 83, 131, 233, 4, 132, 43, 32, 207, 59, 222, 81, 229, 121, 227, 28, 99, 189, 119, 230, 47, 130, 130, 202, 6, 69, 251, 188, 230, 228, 104, 55, 239, 179, 182, 10, 40, 45, 93, 81, 66, 15, 12, 234, 36, 38, 251, 29, 227, 7, 209, 159, 181, 27, 179, 4, 1, 219, 38, 241, 79, 137, 22, 214, 119, 143, 222, 138, 98, 0, 196, 236, 128, 225, 55, 49, 93, 228, 77, 84, 224, 125, 199, 71, 138, 88, 229, 215, 11, 49, 198, 39, 245, 162, 112, 117, 52, 220, 210, 153, 73, 195, 16, 89, 58, 157, 45, 104, 158, 60, 55, 215, 140, 27, 60, 69, 102, 113, 7, 1, 137, 156, 16, 85, 225, 253, 220, 104, 49, 48, 169, 154, 24, 161, 167, 170, 123, 182, 105, 219, 94, 43, 27, 228, 61, 167, 42, 239, 243, 74, 9, 126, 1, 106, 130, 156, 139, 165, 207, 155, 191, 225, 193, 211, 39, 215, 101, 16, 180, 201, 99, 239, 93, 170, 96, 12, 216, 244, 226, 26, 224, 111, 182, 198, 5, 239, 80, 97, 243, 205, 128, 129, 12, 99, 212, 208, 83, 46, 106, 204, 86, 211, 24, 168, 89, 5, 199, 88, 35, 247, 151, 143, 178, 40, 0, 100, 11, 32, 174, 241, 56, 212, 33, 150, 117, 241, 158, 128, 46, 13, 99, 151, 74, 63, 195, 150, 19, 163, 168, 35, 33, 41, 141, 160, 204, 194, 226, 26, 137, 3, 1, 2, 2]"#
    )]
    branch_changes: Vec<u8>,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    post,
    path = "/v1/messenger/update-key",
    request_body = UpdateKey,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn update_key(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Json(payload): Json<UpdateKey>,
) -> Result<StatusCode, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let branch_changes = postcard::from_bytes::<BranchChanges<ARTGroup>>(&payload.branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    match branch_changes.change_type {
        BranchChangesType::UpdateKeys => {
            state
                .art_service
                .update_art(&payload.chat_id, &branch_changes)
                .await
                .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
        }
        _ => {
            return Err(ApiError::BadRequest(
                "Invalid change type. Expected UpdateKeys.".to_owned(),
            ));
        }
    }

    Ok(StatusCode::OK)
}

// ########################################
// get_changes
// ########################################

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetChangesQuery {
    /// Serialised art structure for new chat
    #[schema(
        example = r#"{"root":{"public_key":[93,204,250,239,86,184,225,19,125,201,203,238,144,197,170,153,170,109,111,102,221,244,35,197,34,146,107,80,133,225,225,14,115,37,119,103,215,149,190,117,198,41,73,219,149,186,57,156,21,227,162,136,186,253,134,127,33,135,80,82,37,13,2,143],"l":{"public_key":[163,136,76,181,232,10,167,59,44,1,200,84,84,247,126,197,243,131,169,41,135,0,178,216,144,223,179,188,22,242,36,0,211,222,140,178,157,49,202,100,246,174,122,158,245,208,229,114,94,147,151,183,250,156,150,163,179,110,109,4,33,169,47,137],"l":null,"r":null,"is_temporal":false,"weight":1},"r":{"public_key":[83,85,176,206,106,64,75,186,85,178,13,41,109,63,238,100,32,49,194,89,58,85,147,133,9,198,173,0,22,86,148,8,233,53,14,99,155,164,223,2,19,157,107,220,0,212,197,25,136,253,37,172,73,117,125,45,184,14,253,231,117,82,146,2],"l":null,"r":null,"is_temporal":false,"weight":1},"is_temporal":false,"weight":2},"generator":[22,244,156,151,240,52,186,62,131,201,148,9,140,40,123,188,71,49,218,14,31,201,65,80,246,210,196,99,184,101,78,11,228,198,239,125,93,107,124,64,185,17,37,99,218,201,16,12,183,132,48,197,157,65,225,253,184,205,141,66,95,213,110,139]}"#
    )]
    art: String,

    /// Unique identifier of the chat to send the message to.
    #[schema(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,

    /// Number of results to be returned
    pub limit: i64,

    /// The amount or results to skip at first
    pub skip: i64,
}

#[utoipa::path(
    get,
    path = "/v1/messenger/changes",
    request_body = GetChangesQuery,
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn get_changes(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<GetChangesQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    let filter = doc! {};
    let changes = state
        .art_service
        .list_changes(&payload.chat_id, filter, payload.limit, payload.skip)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    let response_body = postcard::to_allocvec(&changes)
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;
    let response = (StatusCode::OK, response_body);

    Ok(response)
}

#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct DeleteQuery {
    /// Unique identifier of the chat to send the message to.
    #[param(example = r#"3fa85f64-5717-4562-b3fc-2c963f66afa6"#)]
    pub chat_id: Uuid,
}

#[utoipa::path(
    delete,
    path = "/v1/messenger/chat",
    params(
        DeleteQuery,
    ),
    security(("bearer_auth" = [])),
    tag = "Chat operations"
)]
#[instrument(skip(state, headers), err)]
pub async fn delete_chat(
    State(state): State<Arc<Container>>,
    headers: HeaderMap,
    Query(payload): Query<DeleteQuery>,
) -> Result<impl IntoResponse, ApiError> {
    payload
        .validate()
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    state
        .art_service
        .delete_art(&payload.chat_id)
        .await
        .map_err(|e| ApiError::InternalServerError(e.to_string()))?;

    Ok(StatusCode::OK)
}
