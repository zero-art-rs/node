// Asynchronous Ratchet Tree implementation

use crate::helper_tools::{self, ark_de, ark_se};
use ark_ec::pairing::{Pairing, PairingOutput};
use ark_ec::{AffineRepr, CurveGroup, PrimeGroup};
use ark_ff::{Field, Fp12, Fp12Config, Fp256, MontBackend, PrimeField, ToConstraintField};
use ark_std::iterable::Iterable;
use ark_std::{One, UniformRand, Zero};
use serde::{Deserialize, Serialize};
use serde_json;
use std::cmp::max;
use std::mem;
use std::ops::{Add, DerefMut, Mul};

use ark_bn254::{
    Bn254, Config, Fq, Fq12Config, G1Projective as G1, G2Projective as ART_G, G2Projective as G2,
    fr::Fr as ARTScalarField, fr::FrConfig,
};

use crate::art_node::{ARTNode, Direction};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum BranchChangesType {
    MakeTemporal(
        #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")] ART_G,
        #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")] ARTScalarField,
    ),
    AppendNode(ARTNode),
    UpdateKeys,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    RemoveNode(ART_G),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BranchChanges {
    pub change_type: BranchChangesType,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub public_keys: Vec<ART_G>,
    pub next: Vec<Direction>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
pub struct ARTRootKey {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub key: ARTScalarField,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub lambda: Option<ARTScalarField>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub generator: ART_G,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct ART {
    root: Box<ARTNode>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    generator: ART_G,
    size: usize,
}

impl ART {
    pub fn iota_function(point: &ART_G) -> ARTScalarField {
        // Convert into affine representation, so result will always be the same
        ARTScalarField::from(point.into_affine().x.c0.into_bigint())
    }

    fn compute_next_layer_of_tree(
        level_nodes: &mut Vec<ARTNode>,
        level_secrets: &mut Vec<ARTScalarField>,
        generator: &ART_G,
    ) -> (Vec<ARTNode>, Vec<ARTScalarField>) {
        let mut upper_level_nodes = Vec::new();
        let mut upper_level_secrets = Vec::new();

        // iterate until level_nodes is empty, then swap it with the next layer
        while level_nodes.len() > 1 {
            let left_node = level_nodes.remove(0);
            let right_node = level_nodes.remove(0);

            level_secrets.remove(0); // skip the first secret

            let common_secret = left_node.public_key.mul(level_secrets.remove(0));
            let secret_hash = Self::iota_function(&common_secret);

            let node = ARTNode::new(
                generator.mul(&secret_hash),
                Some(Box::new(left_node)),
                Some(Box::new(right_node)),
            );

            upper_level_nodes.push(node);
            upper_level_secrets.push(secret_hash);
        }

        if level_nodes.len() == 1 {
            let first_node = level_nodes.remove(0);
            upper_level_nodes.push(first_node);
            let first_secret = level_secrets.remove(0);
            upper_level_secrets.push(first_secret.clone());
        }

        (upper_level_nodes, upper_level_secrets)
    }

    pub fn new_art_from_secrets(
        secrets: &Vec<ARTScalarField>,
        generator: &ART_G,
    ) -> (Self, ARTRootKey) {
        let mut level_nodes = Vec::new();
        let mut level_secrets = Vec::new();

        // leaves of the tree
        for leaf_secret in secrets {
            let node = ARTNode::new(generator.mul(leaf_secret), None, None);

            level_nodes.push(node);
            level_secrets.push(leaf_secret.clone());
        }

        // iterate by levels. Go from current level to upper level
        while level_nodes.len() > 1 {
            (level_nodes, level_secrets) =
                ART::compute_next_layer_of_tree(&mut level_nodes, &mut level_secrets, generator);
        }

        let root = level_nodes.remove(0);
        let root_key = ARTRootKey {
            key: level_secrets.remove(0),
            lambda: None,
            generator: generator.clone(),
        };

        let art = ART {
            root: Box::new(root),
            generator: generator.clone(),
            size: secrets.len(),
        };

        (art, root_key)
    }

    pub fn get_root(&self) -> &Box<ARTNode> {
        &self.root
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn replace_root(&mut self, new_root: Box<ARTNode>) -> Box<ARTNode> {
        mem::replace(&mut self.root, new_root)
    }

    pub fn get_co_path_values(&self, user_public_key: ART_G) -> Result<Vec<ART_G>, String> {
        let (path_nodes, next_node) = self.get_path_to_leaf(user_public_key)?;

        let mut co_path_values = Vec::new();

        for i in (0..path_nodes.len() - 1).rev() {
            let node = path_nodes.get(i).unwrap();
            let direction = next_node.get(i).unwrap();

            match direction {
                Direction::Left => co_path_values.push(node.get_right().public_key),
                Direction::Right => co_path_values.push(node.get_left().public_key),
                _ => return Err("Unexpected direction".into()),
            }
        }

        Ok(co_path_values)
    }

    pub fn get_path_to_leaf(
        &self,
        user_val: ART_G,
    ) -> Result<(Vec<&ARTNode>, Vec<Direction>), String> {
        let root = self.get_root();

        let mut path = vec![root.as_ref()];
        let mut next = vec![Direction::NoDirection];

        while !path.is_empty() {
            let last_node = path.last().unwrap();

            if last_node.is_leaf() {
                if last_node.public_key.eq(&user_val) {
                    next.pop();
                    return Ok((path, next));
                } else {
                    path.pop();
                    next.pop();
                }
            } else {
                match next.pop().unwrap() {
                    Direction::Left => {
                        path.push(last_node.get_right().as_ref());

                        next.push(Direction::Right);
                        next.push(Direction::NoDirection);
                    }
                    Direction::Right => {
                        path.pop();
                    }
                    Direction::NoDirection => {
                        path.push(last_node.get_left().as_ref());

                        next.push(Direction::Left);
                        next.push(Direction::NoDirection);
                    }
                }
            }
        }

        Err("Can't find a path.".to_string())
    }

    pub fn recompute_root_key(&self, lambda: ARTScalarField) -> ARTRootKey {
        let mut secret_key = lambda.clone();

        let user_public_key = self.generator.mul(secret_key);
        let co_path_values = self.get_co_path_values(user_public_key).unwrap();

        for public_key in co_path_values.iter() {
            secret_key = Self::iota_function(&public_key.mul(secret_key));
        }

        ARTRootKey {
            key: secret_key,
            lambda: Some(lambda),
            generator: self.generator.clone(),
        }
    }

    pub fn public_key_from_lambda(&self, lambda: ARTScalarField) -> ART_G {
        let secret_key = lambda.clone();
        self.generator.mul(secret_key)
    }

    pub fn height(&self) -> usize {
        match self.size.is_power_of_two() {
            true => self.size.ilog2() as usize,
            false => (self.size.ilog2() + 1) as usize,
        }
    }

    pub fn update_branch_public_keys(
        &mut self,
        lambda: ARTScalarField,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        let (_, mut next) = self.get_path_to_leaf(self.public_key_from_lambda(lambda))?;

        let mut changes = BranchChanges {
            change_type: BranchChangesType::UpdateKeys,
            public_keys: Vec::new(),
            next: next.clone(),
        };

        let mut secret_key = lambda.clone();
        let mut public_key = self.generator.mul(secret_key);

        while !next.is_empty() {
            let next_child = next.pop().unwrap();

            let mut parent = self.root.as_mut();
            for direction in &next {
                parent = parent.get_mut_child(direction)?;
            }

            parent
                .get_mut_child(&next_child)?
                .set_public_key(public_key);

            changes.public_keys.push(public_key);

            let other_child_public_key = parent.get_other_child(&next_child)?.public_key.clone();
            let secret = other_child_public_key.mul(secret_key);
            secret_key = Self::iota_function(&secret);
            public_key = self.generator.mul(&secret_key);
        }

        self.root.set_public_key(public_key);
        changes.public_keys.push(public_key);
        changes.public_keys.reverse();

        let key = ARTRootKey {
            key: secret_key,
            lambda: Some(lambda),
            generator: self.generator.clone(),
        };

        Ok((key, changes))
    }

    pub fn change_lambda(
        &mut self,
        old_lambda: ARTScalarField,
        new_lambda: ARTScalarField,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        let (_, mut next) = self.get_path_to_leaf(self.public_key_from_lambda(old_lambda))?;
        let new_public_key = self.public_key_from_lambda(new_lambda);

        let mut user_node = self.get_to_node(next)?;
        user_node.set_public_key(new_public_key);

        self.update_branch_public_keys(new_lambda)
    }

    pub fn find_path_to_possible_leaf_for_insertion(&self) -> Result<Vec<Direction>, String> {
        let root = self.get_root();
        let height = self.height();

        let mut path = vec![root.as_ref()];
        let mut next = vec![Direction::NoDirection];

        while !path.is_empty() {
            let last_node = path.last().unwrap();

            if last_node.is_leaf() {
                // there is <=, because next contains additional NoDirection
                if next.len() <= height || last_node.is_temporal {
                    return Ok(next);
                } else {
                    path.pop();
                    next.pop();
                }
            } else {
                match next.pop().unwrap() {
                    Direction::Left => {
                        path.push(last_node.get_right().as_ref());

                        next.push(Direction::Right);
                        next.push(Direction::NoDirection);
                    }
                    Direction::Right => {
                        path.pop();
                    }
                    Direction::NoDirection => {
                        path.push(last_node.get_left().as_ref());

                        next.push(Direction::Left);
                        next.push(Direction::NoDirection);
                    }
                }
            }
        }

        Err("Can't find a place for insertion.".into())
    }

    pub fn is_full_binary_tree(&self) -> bool {
        self.size.is_power_of_two()
    }

    fn find_place_and_append_node(&mut self, node: ARTNode) -> Result<(), String> {
        match self.is_full_binary_tree() {
            true => {
                self.root.extend(node);
            }
            false => {
                let mut next = self.find_path_to_possible_leaf_for_insertion()?;

                let mut node_for_extension = self.root.as_mut();
                for direction in &next {
                    if node_for_extension.have_child(direction) {
                        node_for_extension = node_for_extension.get_mut_child(direction)?;
                    } else {
                        break;
                    }
                }

                node_for_extension.extend_or_replace(node);
            }
        };

        self.size += 1;

        Ok(())
    }

    pub fn append_node_by_lambda(
        &mut self,
        lambda: ARTScalarField,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        let secret_key = lambda.clone();
        let new_public_key = self.generator.mul(secret_key);

        let new_node = ARTNode::new(new_public_key, None, None);

        self.find_place_and_append_node(new_node.clone())?;

        match self.update_branch_public_keys(lambda) {
            Ok((root_key, mut changes)) => {
                changes.change_type = BranchChangesType::AppendNode(new_node);

                Ok((root_key, changes))
            }
            Err(msg) => Err(msg),
        }
    }

    pub fn change_node_to_temporal(
        &mut self,
        public_key: ART_G,
        temporal_lambda: ARTScalarField,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        let temporal_secret_key = temporal_lambda.clone();
        let new_public_key = self.generator.mul(temporal_secret_key);

        let (_, mut next) = self.get_path_to_leaf(public_key)?;

        self.get_to_node(next)?.make_temporal(new_public_key);
        self.size -= 1;

        match self.update_branch_public_keys(temporal_lambda) {
            Ok((root_key, mut changes)) => {
                changes.change_type = BranchChangesType::MakeTemporal(public_key, temporal_lambda);

                Ok((root_key, changes))
            }
            Err(msg) => Err(msg),
        }
    }

    pub fn update_branch_public_keys_using_changes(
        &mut self,
        changes: &BranchChanges,
    ) -> Result<(), String> {
        let mut current_node = self.root.as_mut();
        for i in (0..changes.public_keys.len() - 1) {
            current_node.set_public_key(changes.public_keys[i].clone());
            current_node = current_node.get_mut_child(changes.next.get(i).unwrap())?;
        }

        current_node.set_public_key(changes.public_keys[changes.public_keys.len() - 1].clone());

        Ok(())
    }

    pub fn get_to_node(&mut self, next: Vec<Direction>) -> Result<&mut ARTNode, String> {
        let mut target_node = self.root.as_mut();
        for direction in &next {
            target_node = target_node.get_mut_child(direction)?;
        }

        Ok(target_node)
    }

    pub fn can_remove(&mut self, lambda: ARTScalarField, public_key: ART_G) -> bool {
        let users_public_key = self.public_key_from_lambda(lambda);

        if users_public_key == public_key {
            return false;
        }

        let (_, mut path_to_other) = self.get_path_to_leaf(public_key).unwrap();
        let (_, mut path_to_self) = self.get_path_to_leaf(users_public_key).unwrap();

        if path_to_other.len().abs_diff(path_to_self.len()) > 1 {
            return false;
        }

        for i in 0..(max(path_to_self.len(), path_to_other.len()) - 2) {
            if path_to_self[i] != path_to_other[i] {
                return false;
            }
        }

        true
    }

    pub fn remove_node_from_tree(&mut self, neighbour_public_key: ART_G) -> Result<(), String> {
        let (_, mut next) = self.get_path_to_leaf(neighbour_public_key)?;
        let for_deletion = next.pop().unwrap();
        let parent = self.get_to_node(next)?;

        parent.shrink_to_other(for_deletion)?;
        self.size -= 1;

        Ok(())
    }

    pub fn remove_node(
        &mut self,
        lambda: ARTScalarField,
        public_key: ART_G,
    ) -> Result<(ARTRootKey, BranchChanges), String> {
        if !self.can_remove(lambda, public_key) {
            return Err("Can't remove a node, because the given node isn't close enough".into());
        }

        self.remove_node_from_tree(public_key)?;

        match self.update_branch_public_keys(lambda) {
            Ok((root_key, mut changes)) => {
                changes.change_type = BranchChangesType::RemoveNode(public_key);

                Ok((root_key, changes))
            }
            Err(msg) => Err(msg),
        }
    }

    pub fn update_branch(&mut self, changes: &BranchChanges) -> Result<(), String> {
        match &changes.change_type {
            BranchChangesType::UpdateKeys => self.update_branch_public_keys_using_changes(changes),
            BranchChangesType::AppendNode(node) => {
                self.find_place_and_append_node(node.clone())?;
                self.update_branch_public_keys_using_changes(changes)
            }
            BranchChangesType::MakeTemporal(public_key, temporal_lambda) => {
                match self.change_node_to_temporal(public_key.clone(), temporal_lambda.clone()) {
                    Ok(_) => Ok(()),
                    Err(msg) => Err(msg),
                }
            }
            BranchChangesType::RemoveNode(public_key) => {
                self.remove_node_from_tree(public_key.clone())?;

                self.update_branch_public_keys_using_changes(changes)
            }
        }
    }

    pub fn serialise(&self) -> Result<String, String> {
        match serde_json::to_string(&self) {
            Ok(json) => Ok(json),
            Err(e) => Err(format!("Failed to serialise: {:?}", e)),
        }
    }

    pub fn from_json(canonical_json: &String) -> Result<Self, String> {
        let tree: Self = match serde_json::from_str(canonical_json) {
            Ok(tree) => tree,
            Err(e) => return Err(format!("Failed to deserialize: {:?}", e)),
        };

        Ok(tree)
    }
}
