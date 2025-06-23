use std::fmt;
use std::fmt::{Display, Formatter};
use ark_ec::{AffineRepr, CurveGroup};
use ark_serialize::{CanonicalDeserialize, CanonicalSerialize};
use serde::{Deserialize, Serialize};

use crate::helper_tools::{ark_de, ark_se};

#[derive(Debug, Clone, Copy, Eq, PartialEq, Deserialize, Serialize)]
pub enum Direction {
    NoDirection,
    Left,
    Right,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
#[serde(bound = "")]
pub struct ARTNode<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> {
    #[serde(serialize_with = "ark_se", deserialize_with = "ark_de")]
    pub public_key: G,
    pub l: Option<Box<ARTNode<G>>>,
    pub r: Option<Box<ARTNode<G>>>,
    pub is_temporal: bool,
    pub weight: usize,
}

impl<G> Display for ARTNode<G>
where
    G: CurveGroup + CanonicalSerialize + CanonicalDeserialize + std::fmt::Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "ARTNode {{")?;
        // writeln!(f, "  public_key: {:?}", self.public_key)?;
        // writeln!(f, "  is_temporal: {}", self.is_temporal)?;
        writeln!(f, "  weight: {}", self.weight)?;

        match &self.l {
            Some(left) => writeln!(f, "  l: {}", left)?,
            None => {},
        }

        match &self.r {
            Some(right) => writeln!(f, "  r: {}", right)?,
            None => {},
        }

        write!(f, "}}")
    }
}

impl<G: CurveGroup> ARTNode<G> {
    pub fn new(
        public_key: G,
        l: Option<Box<ARTNode<G>>>,
        r: Option<Box<ARTNode<G>>>,
    ) -> Result<ARTNode<G>, String> {
        let weight = match (&l, &r) {
            (Some(l), Some(r)) => l.weight + r.weight, //internal node
            (None, None) => 1,                         // leaf node
            _ => return Err("Cannot create a node with only one child".to_string()),
        };

        Ok(ARTNode {
            public_key,
            l,
            r,
            is_temporal: false,
            weight,
        })
    }

    pub fn new_internal_node(public_key: G, l: Box<ARTNode<G>>, r: Box<ARTNode<G>>) -> ARTNode<G> {
        let weight = l.weight + r.weight;

        ARTNode {
            public_key,
            l: Some(l),
            r: Some(r),
            is_temporal: false,
            weight,
        }
    }

    pub fn new_leaf(public_key: G) -> ARTNode<G> {
        ARTNode {
            public_key,
            l: None,
            r: None,
            is_temporal: false,
            weight: 1,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.l.is_none() || self.r.is_none()
    }

    pub fn get_left(&self) -> &Box<ARTNode<G>> {
        match &self.l {
            Some(l) => l,
            None => panic!("Leaf doesn't have a left child."),
        }
    }

    pub fn make_temporal(&mut self, temporal_public_key: &G) -> Result<(), String> {
        if self.is_leaf() {
            self.set_public_key(temporal_public_key.clone());
            self.is_temporal = true;
            self.weight = 0;
            Ok(())
        } else { 
            Err("Cannot convert internal node to temporal one.".to_string())
        }
    }

    pub fn get_mut_left(&mut self) -> &mut Box<ARTNode<G>> {
        match &mut self.l {
            Some(l) => l,
            None => panic!("Leaf doesn't have a left child."),
        }
    }

    pub fn get_right(&self) -> &Box<ARTNode<G>> {
        match &self.r {
            Some(r) => r,
            None => panic!("Leaf doesn't have a right child."),
        }
    }

    pub fn get_mut_right(&mut self) -> &mut Box<ARTNode<G>> {
        match &mut self.r {
            Some(r) => r,
            None => panic!("Leaf doesn't have a right child."),
        }
    }

    pub fn set_left(&mut self, other: ARTNode<G>) {
        self.l = Some(Box::new(other));
    }

    pub fn set_right(&mut self, other: ARTNode<G>) {
        self.r = Some(Box::new(other));
    }

    pub fn get_public_key(&self) -> G {
        self.public_key.clone()
    }

    pub fn set_public_key(&mut self, public_key: G) {
        self.public_key = public_key;
    }

    pub fn have_child(&self, child: &Direction) -> bool {
        match child {
            Direction::Left => self.l.is_some(),
            Direction::Right => self.r.is_some(),
            _ => false,
        }
    }

    pub fn get_child(&self, child: &Direction) -> Result<&Box<ARTNode<G>>, String> {
        match child {
            Direction::Left => Ok(self.get_left()),
            Direction::Right => Ok(self.get_right()),
            Direction::NoDirection => Err("Unexpected direction".into()),
        }
    }

    pub fn get_mut_child(&mut self, child: &Direction) -> Result<&mut Box<ARTNode<G>>, String> {
        match child {
            Direction::Left => Ok(self.get_mut_left()),
            Direction::Right => Ok(self.get_mut_right()),
            Direction::NoDirection => Err("Unexpected direction".into()),
        }
    }

    pub fn get_other_child(&self, child: &Direction) -> Result<&Box<ARTNode<G>>, String> {
        match child {
            Direction::Left => Ok(self.get_right()),
            Direction::Right => Ok(self.get_left()),
            Direction::NoDirection => Err("Unexpected direction".into()),
        }
    }

    pub fn get_mut_other_child(
        &mut self,
        child: &Direction,
    ) -> Result<&mut Box<ARTNode<G>>, String> {
        match child {
            Direction::Left => Ok(self.r.as_mut().unwrap()),
            Direction::Right => Ok(self.l.as_mut().unwrap()),
            Direction::NoDirection => Err("Unexpected direction".into()),
        }
    }

    /// Move current node down to left child, and append other node to the right. The current node
    /// becomes iternal.
    pub fn extend(&mut self, other: ARTNode<G>) {
        let weight = other.weight + self.weight;

        let new_self = ARTNode {
            public_key: self.public_key.clone(),
            l: self.l.take(),
            r: self.r.take(),
            is_temporal: false,
            weight,
        };

        self.weight = other.weight + new_self.weight;
        self.l = Some(Box::new(new_self));
        self.r = Some(Box::new(other));
    }

    pub fn replace_with(&mut self, other: ARTNode<G>) {
        self.set_public_key(other.get_public_key());
        self.l = other.l;
        self.r = other.r;
        self.is_temporal = other.is_temporal;
        self.weight = other.weight;
    }

    /// If the node is temporal, replace the node, else moves current node down to left,
    /// and append other node to the right
    pub fn extend_or_replace(&mut self, other: ARTNode<G>) -> Result<(), String> {
        if !self.is_leaf() {
            return Err("Cannot extend a leaf node.".to_string());
        }
        
        match self.is_temporal {
            true => self.replace_with(other),
            false => self.extend(other),
        }
        
        Ok(())
    }

    /// Change current node with child. Other child is removed. The result is other child
    pub fn shrink_to(&mut self, child: Direction) -> Result<Option<Box<ARTNode<G>>>, String> {
        let (mut new_self, other_child) = match child {
            Direction::Left => (self.l.take(), self.r.take()),
            Direction::Right => (self.r.take(), self.l.take()),
            _ => return Err("Unexpected direction".into()),
        };

        let mut new_self = new_self.unwrap();

        self.public_key = new_self.public_key.clone();
        self.l = new_self.l.take();
        self.r = new_self.r.take();

        Ok(other_child)
    }

    pub fn shrink_to_other(
        &mut self,
        for_removal: Direction,
    ) -> Result<Option<Box<ARTNode<G>>>, String> {
        match for_removal {
            Direction::Left => self.shrink_to(Direction::Right),
            Direction::Right => self.shrink_to(Direction::Left),
            _ => return Err("Unexpected direction".into()),
        }
    }
}

impl<G: CurveGroup + CanonicalSerialize + CanonicalDeserialize> PartialEq for ARTNode<G> {
    fn eq(&self, other: &Self) -> bool {
        match self.public_key.into_affine() != other.public_key.into_affine()
            || self.l != other.l
            || self.r != other.r
            || self.is_temporal != other.is_temporal
            || self.weight != other.weight
        {
            true => false,
            false => true,
        }
    }
}
