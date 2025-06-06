use ark_bn254::fr::Fr as ScalarField;
use ark_bn254::fr::Fr as ARTScalarField;
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};
use ark_std::{One, UniformRand, Zero};
use rand::thread_rng;

// return random ScalarField element, which isn't zero or one
pub fn random_non_neutral_scalar_field_element() -> ScalarField {
    let mut rng = rand::thread_rng();

    let mut k = ScalarField::zero();
    while k.is_one() || k.is_zero() {
        k = ScalarField::rand(&mut rng);
    }

    k
}

// For art serialisation
pub fn ark_se<S, A: CanonicalSerialize>(a: &A, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut bytes = vec![];
    a.serialize_with_mode(&mut bytes, Compress::No)
        .map_err(serde::ser::Error::custom)?;
    s.serialize_bytes(&bytes)
}

// For art deserialization
pub fn ark_de<'de, D, A: CanonicalDeserialize>(data: D) -> Result<A, D::Error>
where
    D: serde::de::Deserializer<'de>,
{
    let s: Vec<u8> = serde::de::Deserialize::deserialize(data)?;
    let a = A::deserialize_with_mode(s.as_slice(), Compress::No, Validate::Yes);
    a.map_err(serde::de::Error::custom)
}

pub fn create_random_secrets(size: usize) -> Vec<ARTScalarField> {
    let mut secrets = Vec::new();
    for x in 0..size {
        secrets.push(ARTScalarField::rand(&mut thread_rng()));
    }

    secrets
}
