use crate::errors::ApiError;
use art::{ART, BranchChanges};
use base64::Engine;
use base64::prelude::BASE64_STANDARD;
use cortado::CortadoAffine as ARTGroup;

/// Decode branch changes from base64 string
pub(crate) fn decode_branch_changes(
    branch_changes: &String,
) -> Result<BranchChanges<ARTGroup>, ApiError> {
    let branch_changes_bytes = BASE64_STANDARD
        .decode(branch_changes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    BranchChanges::<ARTGroup>::deserialize(&branch_changes_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// Decode art from base64 string
pub(crate) fn decode_art(art: &str) -> Result<ART<ARTGroup>, ApiError> {
    let art_bytes = BASE64_STANDARD
        .decode(art)
        .map_err(|e| ApiError::BadRequest(e.to_string()))?;

    ART::<ARTGroup>::deserialize_with_postcard(&art_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}
