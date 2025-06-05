use crate::art::{ART, ARTCiphertext, ARTRootKey, UserIdentity};
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
    pub gamma: ScalarField,
    pub g2: ART_G,
    pub secret_keys: Option<Vec<Fp12<Fq12Config>>>,
    pub art_generator: ART_G,
    pub base_generator: G1,
}

impl ARTTrustedAgent {
    pub fn new(gamma: ScalarField, g2: ART_G) -> Self {
        let art_generator = ART_G::generator();
        let base_generator = G1::generator();

        ARTTrustedAgent {
            gamma,
            g2,
            secret_keys: None,
            art_generator,
            base_generator,
        }
    }

    pub fn compute_art_and_ciphertexts(
        &mut self,
        secret_keys: &Vec<Fp12<Fq12Config>>,
    ) -> (ART, ARTRootKey) {
        self.secret_keys = Some(secret_keys.clone());
        let (tree, root_key) = ART::new_art_from_secrets(&secret_keys, &self.art_generator);

        (tree, root_key)
    }

    pub fn get_recomputed_art(&self) -> ART {
        ART::new_art_from_secrets(&self.secret_keys.clone().unwrap(), &self.art_generator).0
    }
}
