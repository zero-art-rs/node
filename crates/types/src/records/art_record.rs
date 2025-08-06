use ark_ec::AffineRepr;
use ark_ff::PrimeField;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use art::types::PublicART;
use bson::serde_helpers::uuid_1_as_binary;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
    G::BaseField: PrimeField,
{
    #[serde(with = "uuid_1_as_binary")]
    pub chat_id: Uuid,
    pub art: PublicART<G>,
    pub is_private: bool,
    pub sequence_number: i64,
}

impl<G> fmt::Display for ARTRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
    G::BaseField: PrimeField,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[sequence_number: {}, chat_id: {}, root public key: {}]",
            self.sequence_number, self.chat_id, self.art.root.public_key
        )
    }
}
