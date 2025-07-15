use crate::ProofRecord;
use crate::callback_wrappers::ProofVerifierMessage;
use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::types::BranchChanges;
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTChangesRecord<G: AffineRepr + CanonicalSerialize + CanonicalDeserialize> {
    pub sequence_number: i64,
    pub change: BranchChanges<G>,
    pub proof_record: ProofRecord,
}

impl<G: AffineRepr + CanonicalSerialize + CanonicalDeserialize> fmt::Display
    for ARTChangesRecord<G>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[sequence_number: {}, type: {}]",
            self.sequence_number,
            serde_json::to_string(&self.change.change_type).unwrap_or("Undefined".to_string()),
        )
    }
}
