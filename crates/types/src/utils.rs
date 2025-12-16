use crate::protos::Frame;
use crate::protos::group_operation::Operation;
use ark_std::iterable::Iterable;
use cortado::CortadoAffine;
use sha3::digest::typenum::op;
use zrt_art::art::PublicArt;
use zrt_art::changes::ApplicableChange;
use zrt_art::changes::aggregations::AggregatedChange;
use zrt_art::changes::branch_change::BranchChange;
use zrt_art::errors::ArtError;

#[derive(Clone, Debug)]
pub enum ArtUpdate {
    BranchChange(Vec<BranchChange<CortadoAffine>>),
    AggregatedChange(AggregatedChange<CortadoAffine>),
}

impl ArtUpdate {
    pub fn is_empty(&self) -> bool {
        match self {
            Self::BranchChange(changes) => changes.is_empty(),
            Self::AggregatedChange(_) => false,
        }
    }
}

impl ApplicableChange<PublicArt<CortadoAffine>, ()> for ArtUpdate {
    fn apply(&self, art: &mut PublicArt<CortadoAffine>) -> Result<(), ArtError> {
        match self {
            Self::BranchChange(changes) => {
                for change in changes {
                    change.apply(art)?;
                }
                art.commit()?;
            }
            Self::AggregatedChange(change) => {
                change.apply(art)?;
            }
        }

        Ok(())
    }
}

/// Decode branch changes from base64 string
pub fn decode_branch_change(
    branch_change_bytes: &[u8],
) -> Result<BranchChange<CortadoAffine>, ArtError> {
    postcard::from_bytes(branch_change_bytes).map_err(ArtError::from)
}

/// Decode AggregatedChange with postcard
pub fn decode_aggregated_change(bytes: &[u8]) -> Result<AggregatedChange<CortadoAffine>, ArtError> {
    postcard::from_bytes(bytes).map_err(ArtError::from)
}

/// Decode art from with postcard
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

pub fn operation_name(operation: &Operation) -> String {
    match operation {
        Operation::Init(_) => "Init".to_string(),
        Operation::AddMember(_) => "AddMember".to_string(),
        Operation::RemoveMember(_) => "RemoveMember".to_string(),
        Operation::KeyUpdate(_) => "KeyUpdate".to_string(),
        Operation::LeaveGroup(_) => "LeaveGroup".to_string(),
        Operation::DropGroup(_) => "DropGroup".to_string(),
        Operation::Aggregated(_) => "Aggregated".to_string(),
    }
}

pub fn extract_branch_changes(
    frame: &Frame,
) -> Result<Option<BranchChange<CortadoAffine>>, ArtError> {
    if let Some(tbs_frame) = &frame.frame
        && let Some(group_operation) = &tbs_frame.group_operation
        && let Some(operation) = &group_operation.operation
    {
        return match operation {
            Operation::AddMember(branch_changes)
            | Operation::RemoveMember(branch_changes)
            | Operation::KeyUpdate(branch_changes)
            | Operation::LeaveGroup(branch_changes) => {
                Ok(Some(decode_branch_change(branch_changes)?))
            }
            _ => Ok(None),
        };
    }

    Ok(None)
}
