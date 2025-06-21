use ark_ec::CurveGroup;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::art::{ART, BranchChanges};
use art::helper_tools::{ark_de, ark_se};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTRecord<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    pub sequence_number: i64,
    pub art: ART<G>,
}

impl<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> fmt::Display for ARTRecord<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[sequence_number: {}, root public key: {}]",
            self.sequence_number,
            self.art.root.public_key.into_affine()
        )
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTChangesRecord<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    pub sequence_number: i64,
    pub change: BranchChanges<G>,
}

impl<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> fmt::Display
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
