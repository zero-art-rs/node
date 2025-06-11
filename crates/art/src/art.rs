// Asynchronous Ratchet Tree implementation

use crate::art_node::{ARTNode, Direction};
use crate::helper_tools::{ark_de, ark_se};
use ark_ec::{AffineRepr, CurveGroup, pairing::Pairing};
use ark_ff::{BigInteger, Field, PrimeField};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use ark_std::iterable::Iterable;
use serde::{Deserialize, Serialize};
use serde_json;
use std::{cmp::max, mem, ops::Mul};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(bound = "")]
pub enum BranchChangesType<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    MakeTemporal(
        #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")] G,
        #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")] G::ScalarField,
    ),
    AppendNode(ARTNode<G>),
    UpdateKeys,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    RemoveNode(G),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(bound = "")]
pub struct BranchChanges<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    pub change_type: BranchChangesType<G>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub public_keys: Vec<G>,
    pub next: Vec<Direction>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Copy)]
#[serde(bound = "")]
pub struct ARTRootKey<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub key: G::ScalarField,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub generator: G,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(bound = "")]
pub struct ART<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    root: Box<ARTNode<G>>,
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    generator: G,
    size: usize,
}

impl<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> ART<G> {
    pub fn iota_function(point: &G) -> G::ScalarField {
        // Convert into affine representation, so the result will always be the same
        let x = point.into_affine().x().unwrap();
        let base_field_x = x.to_base_prime_field_elements().next().unwrap();
        let x_bigint = base_field_x.into_bigint().to_bytes_le();
        G::ScalarField::from_le_bytes_mod_order(x_bigint.as_slice())
    }

    fn compute_next_layer_of_tree(
        level_nodes: &mut Vec<ARTNode<G>>,
        level_secrets: &mut Vec<G::ScalarField>,
        generator: &G,
    ) -> (Vec<ARTNode<G>>, Vec<G::ScalarField>) {
        let mut upper_level_nodes = Vec::new();
        let mut upper_level_secrets = Vec::new();

        // iterate until level_nodes is empty, then swap it with the next layer
        while level_nodes.len() > 1 {
            let left_node = level_nodes.remove(0);
            let right_node = level_nodes.remove(0);

            level_secrets.remove(0); // skip the first secret

            let common_secret =
                Self::iota_function(&left_node.public_key.mul(level_secrets.remove(0)));

            let node = ARTNode::new(
                generator.mul(&common_secret),
                Some(Box::new(left_node)),
                Some(Box::new(right_node)),
            );

            upper_level_nodes.push(node);
            upper_level_secrets.push(common_secret);
        }

        // if one have an odd number of nodes, the last one will be added to the next level
        if level_nodes.len() == 1 {
            let first_node = level_nodes.remove(0);
            upper_level_nodes.push(first_node);
            let first_secret = level_secrets.remove(0);
            upper_level_secrets.push(first_secret.clone());
        }

        (upper_level_nodes, upper_level_secrets)
    }

    pub fn new_art_from_secrets(
        secrets: &Vec<G::ScalarField>,
        generator: &G,
    ) -> (Self, ARTRootKey<G>) {
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
            generator: generator.clone(),
        };

        let art = ART {
            root: Box::new(root),
            generator: generator.clone(),
            size: secrets.len(),
        };

        (art, root_key)
    }

    pub fn get_root(&self) -> &Box<ARTNode<G>> {
        &self.root
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn replace_root(&mut self, new_root: Box<ARTNode<G>>) -> Box<ARTNode<G>> {
        mem::replace(&mut self.root, new_root)
    }

    pub fn get_co_path_values(&self, user_public_key: G) -> Result<Vec<G>, String> {
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
        user_val: G,
    ) -> Result<(Vec<&ARTNode<G>>, Vec<Direction>), String> {
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

    pub fn recompute_root_key(&self, leaf_secret: G::ScalarField) -> ARTRootKey<G> {
        let co_path_values = self
            .get_co_path_values(self.generator.mul(leaf_secret))
            .unwrap();

        let mut secret = leaf_secret.clone();
        for public_key in co_path_values.iter() {
            secret = Self::iota_function(&public_key.mul(secret));
        }

        ARTRootKey {
            key: secret,
            generator: self.generator.clone(),
        }
    }

    pub fn public_key_of(&self, secret: G::ScalarField) -> G {
        self.generator.mul(secret)
    }

    pub fn height(&self) -> usize {
        match self.size.is_power_of_two() {
            true => self.size.ilog2() as usize,
            false => (self.size.ilog2() + 1) as usize,
        }
    }

    /// Change all public keys on path from the root to node corresponding to the given secret key
    pub fn update_branch_using_secret_key(
        &mut self,
        leaf_secret: G::ScalarField,
    ) -> Result<(ARTRootKey<G>, BranchChanges<G>), String> {
        let (_, mut next) = self.get_path_to_leaf(self.generator.mul(leaf_secret))?;

        let mut changes = BranchChanges {
            change_type: BranchChangesType::UpdateKeys,
            public_keys: Vec::new(),
            next: next.clone(),
        };

        let mut secret_key = leaf_secret.clone();
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
            generator: self.generator.clone(),
        };

        Ok((key, changes))
    }

    pub fn change_lambda(
        &mut self,
        old_leaf_secret: G::ScalarField,
        new_leaf_secret: G::ScalarField,
    ) -> Result<(ARTRootKey<G>, BranchChanges<G>), String> {
        let (_, next) = self.get_path_to_leaf(self.public_key_of(old_leaf_secret))?;
        let new_public_key = self.public_key_of(new_leaf_secret);

        let mut user_node = self.get_to_node(next)?;
        user_node.set_public_key(new_public_key);

        self.update_branch_using_secret_key(new_leaf_secret)
    }

    /// Searches for the closest leaf to the root. Assume that the required leaf is in a subtree,
    /// with the smallest weight.
    pub fn find_path_to_possible_leaf_for_insertion(&self) -> Vec<Direction> {
        let mut candidate = self.get_root();
        let mut next = vec![];

        while !candidate.is_leaf() {
            let l = candidate.get_left();
            let r = candidate.get_right();

            match l.weight < r.weight {
                true => {
                    next.push(Direction::Left);
                    candidate = candidate.get_left();
                }
                false => {
                    next.push(Direction::Right);
                    candidate = candidate.get_right();
                }
            }
        }

        next
    }

    fn find_place_and_append_node(&mut self, node: ARTNode<G>) -> Result<(), String> {
        let next = self.find_path_to_possible_leaf_for_insertion();

        let mut node_for_extension = self.root.as_mut();
        for direction in &next {
            node_for_extension.weight += 1;
            node_for_extension = node_for_extension.get_mut_child(direction)?;
        }

        node_for_extension.extend_or_replace(node);

        self.size += 1;

        Ok(())
    }

    pub fn append_node_by_secret_key(
        &mut self,
        secret_key: G::ScalarField,
    ) -> Result<(ARTRootKey<G>, BranchChanges<G>), String> {
        let new_public_key = self.generator.mul(secret_key);
        let new_node = ARTNode::new(new_public_key, None, None);

        self.find_place_and_append_node(new_node.clone())?;

        self.update_branch_using_secret_key(secret_key)
            .map(|(root_key, mut changes)| {
                changes.change_type = BranchChangesType::AppendNode(new_node);
                (root_key, changes)
            })
    }

    pub fn change_node_to_temporal(
        &mut self,
        public_key: G,
        temporal_secret_key: G::ScalarField,
    ) -> Result<(ARTRootKey<G>, BranchChanges<G>), String> {
        let new_public_key = self.generator.mul(temporal_secret_key);

        let (_, next) = self.get_path_to_leaf(public_key)?;

        let mut target_node = self.root.as_mut();
        for direction in &next {
            target_node.weight -= 1;
            target_node = target_node.get_mut_child(direction)?;
        }
        target_node.make_temporal(new_public_key);

        self.size -= 1;

        match self.update_branch_using_secret_key(temporal_secret_key) {
            Ok((root_key, mut changes)) => {
                changes.change_type =
                    BranchChangesType::MakeTemporal(public_key, temporal_secret_key);

                Ok((root_key, changes))
            }
            Err(msg) => Err(msg),
        }
    }

    pub fn update_branch_public_keys_using_changes(
        &mut self,
        changes: &BranchChanges<G>,
    ) -> Result<(), String> {
        let mut current_node = self.root.as_mut();
        for i in 0..changes.public_keys.len() - 1 {
            current_node.set_public_key(changes.public_keys[i].clone());
            current_node = current_node.get_mut_child(changes.next.get(i).unwrap())?;
        }

        current_node.set_public_key(changes.public_keys[changes.public_keys.len() - 1].clone());

        Ok(())
    }

    /// Returns mut node by the given path to it
    pub fn get_to_node(&mut self, next: Vec<Direction>) -> Result<&mut ARTNode<G>, String> {
        let mut target_node = self.root.as_mut();
        for direction in &next {
            target_node = target_node.get_mut_child(direction)?;
        }

        Ok(target_node)
    }

    pub fn can_remove(&mut self, lambda: G::ScalarField, public_key: G) -> bool {
        let users_public_key = self.public_key_of(lambda);

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

    pub fn remove_node_from_tree(&mut self, neighbour_public_key: G) -> Result<(), String> {
        let (_, mut next) = self.get_path_to_leaf(neighbour_public_key)?;
        let node_for_deletion = next.pop().unwrap();

        let mut target_node = self.root.as_mut();
        for direction in &next {
            target_node.weight -= 1;
            target_node = target_node.get_mut_child(direction)?;
        }

        target_node.shrink_to_other(node_for_deletion)?;
        self.size -= 1;

        Ok(())
    }

    pub fn remove_node(
        &mut self,
        lambda: G::ScalarField,
        public_key: G,
    ) -> Result<(ARTRootKey<G>, BranchChanges<G>), String> {
        if !self.can_remove(lambda, public_key) {
            return Err("Can't remove a node, because the given node isn't close enough".into());
        }

        self.remove_node_from_tree(public_key)?;

        match self.update_branch_using_secret_key(lambda) {
            Ok((root_key, mut changes)) => {
                changes.change_type = BranchChangesType::RemoveNode(public_key);

                Ok((root_key, changes))
            }
            Err(msg) => Err(msg),
        }
    }

    pub fn update_branch(&mut self, changes: &BranchChanges<G>) -> Result<(), String> {
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

impl<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> PartialEq for ART<G> {
    fn eq(&self, other: &Self) -> bool {
        match self.root != other.root
            || self.generator.into_affine() != other.generator.into_affine()
            || self.size != other.size
        {
            true => false,
            false => true,
        }
    }
}
