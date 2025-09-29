use crate::ARTRecord;
use ark_ec::AffineRepr;
use ark_ff::PrimeField;
use art::errors::ARTError;
use mongodb::bson::doc;
use serde::{Deserialize, Serialize};
use serde_with::{
    base64::{Base64, UrlSafe},
    serde_as,
};
use std::fmt::Display;
use utoipa::{IntoParams, ToSchema};
use validator::Validate;

pub const USE_ROOT_KEY: &str = "use_root_key";
pub const USE_LEAF_KEY: &str = "use_leaf_key";

pub enum ProofMode {
    UseRootKey,
    UseLeafKey,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetARTQuery {
    /// Schnorr signature of the message: (chat_id.as_bytes() || nonce || challenge || epoch.to_be_bytes()).
    #[param(value_type = String)]
    #[serde_as(as = "Base64<UrlSafe>")]
    pub signature: Vec<u8>,

    /// User provided nonce
    #[param(value_type = String)]
    #[serde_as(as = "Base64<UrlSafe>")]
    pub nonce: Vec<u8>,

    /// Server given challenge
    #[param(value_type = String)]
    #[serde_as(as = "Base64<UrlSafe>")]
    pub challenge: Vec<u8>,

    /// Indicates which key to use for verification. It can be eather "use_root_key" or "use_leaf_key" mode.
    pub proof_mode: String,

    /// Users leaf or root public key corresponding to the proof_mode
    #[param(value_type = String)]
    #[serde_as(as = "Base64<UrlSafe>")]
    pub public_key: Vec<u8>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Validate, ToSchema, Clone)]
#[serde(rename_all = "camelCase")]
pub struct GetARTResponse {
    /// Serialized art structure
    #[schema(value_type = Option<String>, content_encoding = "Base64")]
    #[serde_as(as = "Base64")]
    pub art: Vec<u8>,

    /// Defines whether the group is private or not
    pub is_private: bool,
}

#[serde_as]
#[derive(Serialize, ToSchema, Deserialize)]
pub struct ChallengeResponse {
    /// Server provided challenge
    #[schema(value_type = String, content_encoding = "Base64")]
    #[serde_as(as = "Base64")]
    pub challenge: Vec<u8>,
}

impl<G> TryFrom<ARTRecord<G>> for GetARTResponse
where
    G: AffineRepr,
    G::BaseField: PrimeField,
{
    type Error = ARTError;

    fn try_from(record: ARTRecord<G>) -> Result<Self, Self::Error> {
        Ok(Self {
            art: record.art.serialize()?,
            is_private: record.is_private,
        })
    }
}

impl Display for ProofMode {
    fn fmt(&self, f1: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let str = match self {
            ProofMode::UseRootKey => USE_ROOT_KEY,
            ProofMode::UseLeafKey => USE_LEAF_KEY,
        };
        write!(f1, "{}", str)
    }
}

impl TryFrom<&str> for ProofMode {
    type Error = crate::errors::VerificationError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            USE_ROOT_KEY => Ok(ProofMode::UseRootKey),
            USE_LEAF_KEY => Ok(ProofMode::UseLeafKey),
            _ => Err(crate::errors::VerificationError::InvalidInput),
        }
    }
}
