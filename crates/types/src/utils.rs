use crate::protos::Frame;
use crate::protos::group_operation::Operation;
use cortado::CortadoAffine;
use zrt_art::art::art_types::PublicArt;
use zrt_art::changes::branch_change::BranchChange;
use zrt_art::errors::ArtError;

/// Decode branch changes from base64 string
pub fn decode_branch_change(
    branch_changes_bytes: &[u8],
) -> Result<BranchChange<CortadoAffine>, ArtError> {
    postcard::from_bytes(branch_changes_bytes).map_err(ArtError::from)
}

/// Decode art from base64 stringArtError
pub fn decode_art(art_bytes: &[u8]) -> Result<PublicArt<CortadoAffine>, ArtError> {
    postcard::from_bytes(art_bytes).map_err(ArtError::from)
}

pub fn extract_operation(frame: Frame) -> Result<Option<Operation>, ArtError> {
    if let Some(tbs_frame) = frame.frame
        && let Some(group_operation) = tbs_frame.group_operation
    {
        return Ok(group_operation.operation);
    }

    Ok(None)
}

pub fn extract_branch_changes(
    frame: &Frame,
) -> Result<Option<BranchChange<CortadoAffine>>, ArtError> {
    if let Some(tbs_frame) = &frame.frame
        && let Some(group_operation) = &tbs_frame.group_operation
        && let Some(operation) = &group_operation.operation
    {
        return match operation {
            Operation::AddMember(branch_changes) => Ok(Some(decode_branch_change(branch_changes)?)),
            Operation::RemoveMember(branch_changes) => {
                Ok(Some(decode_branch_change(branch_changes)?))
            }
            Operation::KeyUpdate(branch_changes) => Ok(Some(decode_branch_change(branch_changes)?)),
            Operation::LeaveGroup(branch_changes) => {
                Ok(Some(decode_branch_change(branch_changes)?))
            }
            _ => Ok(None),
        };
    }

    Ok(None)
}
