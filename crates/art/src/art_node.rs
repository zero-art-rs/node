use ark_bn254::{G2Projective as ART_G, fr::Fr as ARTScalarField};
use ark_ec::{AffineRepr, CurveGroup, short_weierstrass::SWCurveConfig};
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
}

impl<G: CurveGroup> ARTNode<G> {
    pub fn new(
        public_key: G,
        l: Option<Box<ARTNode<G>>>,
        r: Option<Box<ARTNode<G>>>,
    ) -> ARTNode<G> {
        ARTNode {
            public_key,
            l,
            r,
            is_temporal: false,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.l.is_none() && self.r.is_none()
    }

    pub fn get_left(&self) -> &Box<ARTNode<G>> {
        match &self.l {
            Some(l) => l,
            None => panic!("Leaf doesn't have a left child."),
        }
    }

    pub fn make_temporal(&mut self, temporal_public_key: G) {
        if self.is_leaf() {
            self.set_public_key(temporal_public_key);
            self.is_temporal = true;
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

    // Move current node down to left child, and append other node to right
    pub fn extend(&mut self, other: ARTNode<G>) {
        let new_self = ARTNode {
            public_key: self.public_key.clone(),
            l: self.l.take(),
            r: self.r.take(),
            is_temporal: false,
        };

        self.l = Some(Box::new(new_self));
        self.r = Some(Box::new(other));
    }

    pub fn replace_with(&mut self, other: ARTNode<G>) {
        self.set_public_key(other.get_public_key());
        self.l = other.l;
        self.r = other.r;
        self.is_temporal = other.is_temporal;
    }

    pub fn extend_or_replace(&mut self, other: ARTNode<G>) {
        match self.is_temporal {
            true => self.replace_with(other),
            false => self.extend(other),
        }
    }

    // Change current node with child. Other child is removed
    pub fn shrink_to(&mut self, child: Direction) -> Result<Option<Box<ARTNode<G>>>, String> {
        let (mut new_self, mut other_child) = match child {
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
        {
            true => false,
            false => true,
        }
    }
}
