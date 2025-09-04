use art::types::{BranchChanges, PublicART};
use cortado::CortadoAffine as ARTGroup;
use types::errors::ApiError;

/// Decode branch changes from base64 string
pub(crate) fn decode_branch_changes(
    branch_changes_bytes: &Vec<u8>,
) -> Result<BranchChanges<ARTGroup>, ApiError> {
    BranchChanges::<ARTGroup>::deserialize(branch_changes_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// Decode art from base64 string
pub(crate) fn decode_art(art_bytes: &Vec<u8>) -> Result<PublicART<ARTGroup>, ApiError> {
    PublicART::<ARTGroup>::deserialize(art_bytes).map_err(|e| ApiError::BadRequest(e.to_string()))
}
