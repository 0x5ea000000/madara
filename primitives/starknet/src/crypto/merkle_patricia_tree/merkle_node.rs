//! Contains constructs for describing the nodes in a Binary Merkle Patricia Tree
//! used by Starknet.
//!
//! For more information about how these Starknet trees are structured, see
//! [`MerkleTree`](super::merkle_tree::MerkleTree).

use alloc::rc::Rc;
use core::cell::RefCell;

use bitvec::order::Msb0;
use bitvec::prelude::BitVec;
use bitvec::slice::BitSlice;
use starknet_crypto::FieldElement;

use crate::traits::hash::CryptoHasher;
/// A node in a Binary Merkle-Patricia Tree graph.
#[derive(Clone, Debug, PartialEq)]
pub enum Node {
    /// A node that has not been fetched from storage yet.
    ///
    /// As such, all we know is its hash.
    Unresolved(FieldElement),
    /// A branch node with exactly two children.
    Binary(BinaryNode),
    /// Describes a path connecting two other nodes.
    Edge(EdgeNode),
    /// A leaf node that contains a value.
    Leaf(FieldElement),
}

/// Describes the [Node::Binary] variant.
#[derive(Clone, Debug, PartialEq)]
pub struct BinaryNode {
    /// The hash of this node. Is [None] if the node
    /// has not yet been committed.
    pub hash: Option<FieldElement>,
    /// The height of this node in the tree.
    pub height: usize,
    /// [Left](Direction::Left) child.
    pub left: Rc<RefCell<Node>>,
    /// [Right](Direction::Right) child.
    pub right: Rc<RefCell<Node>>,
}

/// Node that is an edge.
#[derive(Clone, Debug, PartialEq)]
pub struct EdgeNode {
    /// The hash of this node. Is [None] if the node
    /// has not yet been committed.
    pub hash: Option<FieldElement>,
    /// The starting height of this node in the tree.
    pub height: usize,
    /// The path this edge takes.
    pub path: BitVec<Msb0, u8>,
    /// The child of this node.
    pub child: Rc<RefCell<Node>>,
}

/// Describes the direction a child of a [BinaryNode] may have.
///
/// Binary nodes have two children, one left and one right.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Left direction.
    Left,
    /// Right direction.
    Right,
}

impl Direction {
    /// Inverts the [Direction].
    ///
    /// [Left] becomes [Right], and [Right] becomes [Left].
    ///
    /// [Left]: Direction::Left
    /// [Right]: Direction::Right
    pub fn invert(self) -> Direction {
        match self {
            Direction::Left => Direction::Right,
            Direction::Right => Direction::Left,
        }
    }
}

impl From<bool> for Direction {
    fn from(tf: bool) -> Self {
        match tf {
            true => Direction::Right,
            false => Direction::Left,
        }
    }
}

impl From<Direction> for bool {
    fn from(direction: Direction) -> Self {
        match direction {
            Direction::Left => false,
            Direction::Right => true,
        }
    }
}

impl BinaryNode {
    /// Maps the key's bit at the binary node's height to a [Direction].
    ///
    /// This can be used to check which direction the key descibes in the context
    /// of this binary node i.e. which direction the child along the key's path would
    /// take.
    pub fn direction(&self, key: &BitSlice<Msb0, u8>) -> Direction {
        key[self.height].into()
    }

    /// Returns the [Left] or [Right] child.
    ///
    /// [Left]: Direction::Left
    /// [Right]: Direction::Right
    pub fn get_child(&self, direction: Direction) -> Rc<RefCell<Node>> {
        match direction {
            Direction::Left => self.left.clone(),
            Direction::Right => self.right.clone(),
        }
    }

    /// If possible, calculates and sets its own hash value.
    ///
    /// Does nothing if the hash is already [Some].
    ///
    /// If either childs hash is [None], then the hash cannot
    /// be calculated and it will remain [None].
    pub(crate) fn calculate_hash<H: CryptoHasher>(&mut self) {
        if self.hash.is_some() {
            return;
        }

        let left = match self.left.borrow().hash() {
            Some(hash) => hash,
            None => unreachable!("subtrees have to be commited first"),
        };

        let right = match self.right.borrow().hash() {
            Some(hash) => hash,
            None => unreachable!("subtrees have to be commited first"),
        };

        self.hash = Some(H::hash(left, right));
    }
}

impl Node {
    /// Convenience function which sets the inner node's hash to [None], if
    /// applicable.
    ///
    /// Used to indicate that this node has been mutated.
    pub fn mark_dirty(&mut self) {
        match self {
            Node::Binary(inner) => inner.hash = None,
            Node::Edge(inner) => inner.hash = None,
            _ => {}
        }
    }

    /// Returns true if the node represents an empty node -- this is defined as a node
    /// with the [FieldElement::ZERO].
    ///
    /// This can occur for the root node in an empty graph.
    pub fn is_empty(&self) -> bool {
        match self {
            Node::Unresolved(hash) => hash == &FieldElement::ZERO,
            _ => false,
        }
    }

    /// Is the node a binary node.
    pub fn is_binary(&self) -> bool {
        matches!(self, Node::Binary(..))
    }

    /// Convert to node to binary node type (returns None if it's not a binary node).
    pub fn as_binary(&self) -> Option<&BinaryNode> {
        match self {
            Node::Binary(binary) => Some(binary),
            _ => None,
        }
    }

    /// Convert to node to edge node type (returns None if it's not a edge node).
    pub fn as_edge(&self) -> Option<&EdgeNode> {
        match self {
            Node::Edge(edge) => Some(edge),
            _ => None,
        }
    }

    /// Get the hash of a node.
    pub fn hash(&self) -> Option<FieldElement> {
        match self {
            Node::Unresolved(hash) => Some(*hash),
            Node::Binary(binary) => binary.hash,
            Node::Edge(edge) => edge.hash,
            Node::Leaf(value) => Some(*value),
        }
    }
}

impl EdgeNode {
    /// Returns true if the edge node's path matches the same path given by the key.
    pub fn path_matches(&self, key: &BitSlice<Msb0, u8>) -> bool {
        self.path == key[self.height..self.height + self.path.len()]
    }

    /// Returns the common bit prefix between the edge node's path and the given key.
    ///
    /// This is calculated with the edge's height taken into account.
    pub fn common_path(&self, key: &BitSlice<Msb0, u8>) -> &BitSlice<Msb0, u8> {
        let key_path = key.iter().skip(self.height);
        let common_length = key_path.zip(self.path.iter()).take_while(|(a, b)| a == b).count();

        &self.path[..common_length]
    }

    /// If possible, calculates and sets its own hash value.
    ///
    /// Does nothing if the hash is already [Some].
    ///
    /// If the child's hash is [None], then the hash cannot
    /// be calculated and it will remain [None].
    pub(crate) fn calculate_hash<H: CryptoHasher>(&mut self) {
        if self.hash.is_some() {
            return;
        }

        let child = match self.child.borrow().hash() {
            Some(hash) => hash,
            None => unreachable!("subtree has to be commited before"),
        };
        let mut temp_path = self.path.clone();
        temp_path.force_align();

        let path = FieldElement::from_byte_slice_be(&temp_path.into_vec()).unwrap();
        let mut length = [0; 32];
        // Safe as len() is guaranteed to be <= 251
        length[31] = self.path.len() as u8;

        let length = FieldElement::from_byte_slice_be(&length).unwrap();
        let hash = H::hash(child, path) + length;
        self.hash = Some(hash);
    }
}