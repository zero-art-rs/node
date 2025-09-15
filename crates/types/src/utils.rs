use crate::errors::StorageError;
use crate::protos::Frame;
use crate::protos::group_operation::Operation;
use crate::{ARTRecord, FrameRecord};
use art::errors::ARTError;
use art::types::{BranchChanges, PublicART};
use cortado::CortadoAffine;
use prost::bytes::{BufMut, BytesMut};

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

pub fn extract_branch_changes(
    frame: &Frame,
) -> Result<Option<BranchChanges<CortadoAffine>>, ARTError> {
    if let Some(tbs_frame) = &frame.frame {
        if let Some(group_operation) = &tbs_frame.group_operation {
            if let Some(operation) = &group_operation.operation {
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
        }
    }

    Ok(None)
}
