use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::ART;
use bson::serde_helpers::uuid_1_as_binary;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTRecord<G: AffineRepr + CanonicalSerialize + CanonicalDeserialize> {
    #[serde(with = "uuid_1_as_binary")]
    pub chat_id: Uuid,
    pub art: ART<G>,
    pub is_private: bool,
}

impl<G: AffineRepr + CanonicalSerialize + CanonicalDeserialize> fmt::Display for ARTRecord<G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[sequence_number: {}, root public key: {}]",
            self.chat_id, self.art.root.public_key
        )
    }
}
