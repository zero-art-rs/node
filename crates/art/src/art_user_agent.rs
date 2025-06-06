use crate::art::{ART, ARTRootKey, BranchChanges};
use crate::helper_tools;
use ark_bn254::{
    Bn254, Config, Fq, Fq12Config, G1Projective as G1, G2Projective as ART_G,
    fr::Fr as ScalarField, fr::Fr as ARTScalarField, fr::FrConfig,
};
use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ff::{Field, Fp12, Fp12Config, Fp256, MontBackend, PrimeField, ToConstraintField};
use ark_std::{One, UniformRand};
use rand;

#[derive(Debug, Clone)]
pub struct ARTUserAgent {
    pub root_key: ARTRootKey,
    pub tree: ART,
    pub lambda: ARTScalarField,
}

impl ARTUserAgent {
    pub fn new(tree: ART, lambda: ARTScalarField) -> Self {
        let root_key = tree.recompute_root_key(lambda);

        Self {
            root_key,
            tree,
            lambda,
        }
    }

    pub fn update_key(&mut self) -> Result<(ARTRootKey, BranchChanges), String> {
        let r = helper_tools::random_non_neutral_scalar_field_element();

        let new_lambda = self.lambda.pow(&r.into_bigint());

        self.change_lambda(new_lambda)
    }

    pub fn append_node(
        &mut self,
        lambda: ARTScalarField,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        match self.tree.append_node_by_lambda(lambda) {
            Ok((root_key, changes)) => {
                self.root_key = root_key.clone();
                Ok((root_key, changes))
            }
            Err(e) => Err(e),
        }
    }

    pub fn change_lambda(
        &mut self,
        new_lambda: ARTScalarField,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        match self.tree.change_lambda(self.lambda, new_lambda) {
            Ok((root_key, changes)) => {
                self.lambda = new_lambda;
                self.root_key = root_key.clone();
                Ok((root_key, changes))
            }
            Err(e) => Err(e),
        }
    }

    pub fn make_temporal(
        &mut self,
        public_key: ART_G,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        let temporal_lambda = ARTScalarField::rand(&mut ark_std::rand::thread_rng());

        match self
            .tree
            .change_node_to_temporal(public_key, temporal_lambda)
        {
            Ok((root_key, changes)) => {
                self.root_key = root_key;
                Ok((root_key, changes))
            }
            Err(e) => Err(e),
        }
    }

    pub fn remove_node(
        &mut self,
        public_key: ART_G,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        match self.tree.remove_node(self.lambda, public_key) {
            Ok((root_key, changes)) => {
                self.root_key = root_key;
                Ok((root_key, changes))
            }
            Err(e) => Err(e),
        }
    }

    pub fn update_branch(&mut self, changes: &BranchChanges) -> Result<(), String> {
        let res = self.tree.update_branch(changes);
        self.root_key = self.tree.recompute_root_key(self.lambda);

        res
    }

    pub fn get_root_key(&self) -> ARTRootKey {
        self.root_key
    }

    pub fn serialise_art(&self) -> Result<String, String> {
        match serde_json::to_string(&self.tree) {
            Ok(json) => Ok(json),
            Err(e) => Err(format!("Failed to serialise: {:?}", e)),
        }
    }

    pub fn deserialize_art(&self, canonical_json: String) -> Result<ART, String> {
        let tree: ART = match serde_json::from_str(&canonical_json) {
            Ok(tree) => tree,
            Err(e) => return Err(format!("Failed to deserialize: {:?}", e)),
        };

        Ok(tree)
    }

    pub fn public_key(&self) -> ART_G {
        self.tree.public_key_of(self.lambda)
    }

    pub fn can_remove(&mut self, public_key: ART_G) -> bool {
        self.tree.can_remove(self.lambda, public_key)
    }
}
