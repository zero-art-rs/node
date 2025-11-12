use ark_ec::AffineRepr;
use ark_ff::PrimeField;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use bson::serde_helpers::uuid_1_as_binary;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;
use zrt_art::art_node::TreeMethods;
use zrt_art::art::PublicZeroArt;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
    G::BaseField: PrimeField,
{
    #[serde(with = "uuid_1_as_binary")]
    pub chat_id: Uuid,
    pub art: PublicZeroArt<G>,
    pub is_private: bool,
    pub epoch: u64,
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
            self.epoch,
            self.chat_id,
            self.art.get_upstream_art().get_root().get_public_key()
        )
    }
}
