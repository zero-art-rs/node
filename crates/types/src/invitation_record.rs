use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::{ark_de, ark_se};
use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InvitationRecord<G: AffineRepr + CanonicalSerialize + CanonicalDeserialize> {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub receiver_public_key: G,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub invite_key: G::ScalarField,
}

impl<G: AffineRepr> fmt::Display for InvitationRecord<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[user_id: {}, cursor: {}]",
            self.receiver_public_key, self.invite_key,
        )
    }
}
