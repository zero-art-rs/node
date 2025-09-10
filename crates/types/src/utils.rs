use art::errors::ARTError;
use art::types::{BranchChanges, PublicART};
use cortado::CortadoAffine;

/// Decode branch changes from base64 string
pub fn decode_branch_changes(
    branch_changes_bytes: &Vec<u8>,
) -> Result<BranchChanges<CortadoAffine>, ARTError> {
    BranchChanges::<CortadoAffine>::deserialize(branch_changes_bytes)
}

/// Decode art from base64 string
pub fn decode_art(art_bytes: &Vec<u8>) -> Result<PublicART<CortadoAffine>, ARTError> {
    PublicART::<CortadoAffine>::deserialize(art_bytes)
}
