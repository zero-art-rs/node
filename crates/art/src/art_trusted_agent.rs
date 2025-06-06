use crate::art::{ART, ARTRootKey};
use crate::helper_tools;
use ark_bn254::{
    Bn254, Config, Fq, Fq12Config, G1Projective as G1, G2Projective as ART_G,
    fr::Fr as ARTScalarField, fr::Fr as ScalarField, fr::FrConfig,
};
use ark_ec::PrimeGroup;
use ark_ec::pairing::Pairing;
use ark_ff::{Field, Fp12, PrimeField};
use std::ops::{Add, Mul};

pub struct ARTTrustedAgent {
    pub secret_keys: Option<Vec<ARTScalarField>>,
    pub generator: ART_G,
}

impl ARTTrustedAgent {
    pub fn new(generator: ART_G) -> Self {
        ARTTrustedAgent {
            secret_keys: None,
            generator,
        }
    }

    pub fn compute_art_and_ciphertexts(
        &mut self,
        secret_keys: &Vec<ARTScalarField>,
    ) -> (ART, ARTRootKey) {
        self.secret_keys = Some(secret_keys.clone());
        let (tree, root_key) = ART::new_art_from_secrets(&secret_keys, &self.generator);

        (tree, root_key)
    }

    pub fn get_recomputed_art(&self) -> ART {
        ART::new_art_from_secrets(&self.secret_keys.clone().unwrap(), &self.generator).0
    }
}
