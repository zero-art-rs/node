use ark_ec::AffineRepr;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use bson::serde_helpers::uuid_1_as_binary;
use serde::{Deserialize, Serialize};
use std::fmt;
use uuid::Uuid;
use zrt_art::art::PublicArt;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(bound = "")]
pub struct ARTRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
{
    #[serde(with = "uuid_1_as_binary")]
    pub chat_id: Uuid,
    pub art: PublicArt<G>,
    pub is_private: bool,
    pub epoch: u64,
}

impl<G> fmt::Display for ARTRecord<G>
where
    G: AffineRepr + CanonicalSerialize + CanonicalDeserialize,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "[sequence_number: {}, chat_id: {}, root public key: {}]",
            self.epoch,
            self.chat_id,
            self.art.root().data().public_key()
        )
    }
}

impl<G: AffineRepr> ARTRecord<G> {
    pub fn new(chat_id: Uuid, art: PublicArt<G>, is_private: bool) -> Self {
        Self {
            chat_id,
            art,
            is_private,
            epoch: 0,
        }
    }
}
