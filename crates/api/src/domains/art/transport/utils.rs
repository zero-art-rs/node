use crate::errors::ApiError;
use art::types::{BranchChanges, PublicART};
use cortado::CortadoAffine as ARTGroup;

/// Decode branch changes from base64 string
pub(crate) fn decode_branch_changes(
    branch_changes_bytes: &Vec<u8>,
) -> Result<BranchChanges<ARTGroup>, ApiError> {
    BranchChanges::<ARTGroup>::deserialize(branch_changes_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// Decode art from base64 string
pub(crate) fn decode_art(art_bytes: &Vec<u8>) -> Result<PublicART<ARTGroup>, ApiError> {
    PublicART::<ARTGroup>::deserialize(art_bytes)
        .map_err(|e| ApiError::BadRequest(e.to_string()))
}

/// Helper for base64 serialization and deserialization
pub(crate) mod as_base64 {
    use base64::Engine;
    use base64::prelude::BASE64_STANDARD;
    use serde::{Deserialize, Serialize};
    use serde::{Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        let base64 = BASE64_STANDARD.encode(v);
        String::serialize(&base64, s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let base64 = String::deserialize(d)?;
        BASE64_STANDARD
            .decode(base64.as_bytes())
            .map_err(serde::de::Error::custom)
    }
}
