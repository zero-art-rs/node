use crate::protos::Frame;
use crate::protos::group_operation::Operation;
use cortado::CortadoAffine;
use zrt_art::errors::ARTError;
use zrt_art::types::{BranchChanges, PublicART};

/// Decode branch changes from base64 string
pub fn decode_branch_changes(
    branch_changes_bytes: &[u8],
) -> Result<BranchChanges<CortadoAffine>, ARTError> {
    BranchChanges::<CortadoAffine>::deserialize(branch_changes_bytes)
}

/// Decode art from base64 string
pub fn decode_art(art_bytes: &[u8]) -> Result<PublicART<CortadoAffine>, ARTError> {
    PublicART::<CortadoAffine>::deserialize(art_bytes)
}

pub fn extract_operation(frame: Frame) -> Result<Option<Operation>, ARTError> {
    if let Some(tbs_frame) = frame.frame
        && let Some(group_operation) = tbs_frame.group_operation
    {
        return Ok(group_operation.operation);
    }

    Ok(None)
}

pub fn extract_branch_changes(
    frame: &Frame,
) -> Result<Option<BranchChanges<CortadoAffine>>, ARTError> {
    if let Some(tbs_frame) = &frame.frame
        && let Some(group_operation) = &tbs_frame.group_operation
        && let Some(operation) = &group_operation.operation
    {
        return match operation {
            Operation::AddMember(branch_changes) => {
                Ok(Some(decode_branch_changes(branch_changes)?))
            }
            Operation::RemoveMember(branch_changes) => {
                Ok(Some(decode_branch_changes(branch_changes)?))
            }
            Operation::KeyUpdate(branch_changes) => {
                Ok(Some(decode_branch_changes(branch_changes)?))
            }
            _ => Ok(None),
        };
    }

    Ok(None)
}
