//! Dynamic AST node storage used by [`SemanticBuilder`] during traversal.
//!
//! Two backends are available:
//!
//! 1. [`NodeStorage::Full`] — every node is recorded in [`AstNodes`], giving
//!    random access by [`NodeId`] after the build finishes. Required by
//!    consumers that walk the whole tree (linter, formatter, mangler).
//! 2. [`NodeStorage::Ancestors`] — only the *live ancestor chain*
//!    (`root..=current`) is retained, via [`AncestorStack`]. This is enough for
//!    the binder, the class-table builder, and the syntax checker — which only
//!    ever look *upwards* from the current node — while avoiding the per-node
//!    allocations of full storage. Used by pipelines that discard the AST nodes
//!    and keep only [`Scoping`] (transform, minify, define/inject).
//!
//! Both backends allocate [`NodeId`]s from the same monotonic counter, so the
//! ids stored in [`Scoping`] are identical regardless of the backend.
//!
//! [`SemanticBuilder`]: crate::SemanticBuilder
//! [`Scoping`]: crate::Scoping

mod ancestor_stack;

use itertools::Either;

use oxc_ast::AstKind;
use oxc_syntax::{
    node::{NodeFlags, NodeId},
    scope::ScopeId,
};

#[cfg(feature = "cfg")]
use oxc_cfg::BlockNodeId;

use super::AstNode;
use crate::node::AstNodes;

use ancestor_stack::AncestorStack;

/// Dynamic AST node storage. See the [module docs](self).
pub enum NodeStorage<'a> {
    /// Full random-access storage, retained after the build.
    Full(AstNodes<'a>),
    /// Only the live ancestor chain is retained during the build.
    Ancestors(AncestorStack<'a>),
}

impl Default for NodeStorage<'_> {
    fn default() -> Self {
        NodeStorage::Full(AstNodes::default())
    }
}

impl<'a> NodeStorage<'a> {
    /// Create full random-access storage.
    pub fn full() -> Self {
        NodeStorage::Full(AstNodes::default())
    }

    /// Create lightweight parent-pointer (ancestor stack) storage.
    pub fn ancestor_stack() -> Self {
        NodeStorage::Ancestors(AncestorStack::new())
    }

    /// Consume the storage, returning the recorded [`AstNodes`].
    ///
    /// In ancestor-stack mode the chain is empty by the end of traversal, so
    /// this returns an empty [`AstNodes`].
    pub fn into_ast_nodes(self) -> AstNodes<'a> {
        match self {
            NodeStorage::Full(nodes) => nodes,
            NodeStorage::Ancestors(_) => AstNodes::default(),
        }
    }

    pub fn add_node(
        &mut self,
        kind: AstKind<'a>,
        scope_id: ScopeId,
        parent_node_id: NodeId,
        #[cfg(feature = "cfg")] cfg_id: BlockNodeId,
        flags: NodeFlags,
    ) -> NodeId {
        match self {
            NodeStorage::Full(nodes) => nodes.add_node(
                kind,
                scope_id,
                parent_node_id,
                #[cfg(feature = "cfg")]
                cfg_id,
                flags,
            ),
            NodeStorage::Ancestors(stack) => stack.add_node(kind, scope_id, flags),
        }
    }

    pub fn add_program_node(
        &mut self,
        kind: AstKind<'a>,
        scope_id: ScopeId,
        #[cfg(feature = "cfg")] cfg_id: BlockNodeId,
        flags: NodeFlags,
    ) -> NodeId {
        match self {
            NodeStorage::Full(nodes) => nodes.add_program_node(
                kind,
                scope_id,
                #[cfg(feature = "cfg")]
                cfg_id,
                flags,
            ),
            NodeStorage::Ancestors(stack) => stack.add_program_node(kind, scope_id, flags),
        }
    }

    /// Pop the current node and return its parent's id.
    #[inline]
    pub fn pop_node(&mut self, current_node_id: NodeId) -> NodeId {
        match self {
            NodeStorage::Full(nodes) => nodes.parent_id(current_node_id),
            NodeStorage::Ancestors(stack) => stack.pop_node(),
        }
    }

    #[inline]
    pub fn get_node(&self, id: NodeId) -> &AstNode<'a> {
        match self {
            NodeStorage::Full(nodes) => nodes.get_node(id),
            NodeStorage::Ancestors(stack) => stack.get_node(id),
        }
    }

    #[inline]
    pub fn kind(&self, id: NodeId) -> AstKind<'a> {
        self.get_node(id).kind()
    }

    #[inline]
    pub fn parent_id(&self, id: NodeId) -> NodeId {
        match self {
            NodeStorage::Full(nodes) => nodes.parent_id(id),
            NodeStorage::Ancestors(stack) => stack.parent_id(id),
        }
    }

    #[inline]
    pub fn parent_kind(&self, id: NodeId) -> AstKind<'a> {
        self.kind(self.parent_id(id))
    }

    #[inline]
    pub fn parent_node(&self, id: NodeId) -> &AstNode<'a> {
        self.get_node(self.parent_id(id))
    }

    #[inline]
    pub fn flags_mut(&mut self, id: NodeId) -> &mut NodeFlags {
        match self {
            NodeStorage::Full(nodes) => nodes.flags_mut(id),
            NodeStorage::Ancestors(stack) => stack.flags_mut(id),
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        match self {
            NodeStorage::Full(nodes) => nodes.len(),
            NodeStorage::Ancestors(stack) => stack.len as usize,
        }
    }

    pub fn reserve(&mut self, additional: usize) {
        // Only full storage grows with the node count; the ancestor stack only
        // ever holds the current nesting depth.
        if let NodeStorage::Full(nodes) = self {
            nodes.reserve(additional);
        }
    }

    /// Walk up the AST, iterating over each parent [`NodeId`].
    ///
    /// The first id produced is the parent of `id`; the last is always the
    /// root [`Program`].
    ///
    /// [`Program`]: oxc_ast::ast::Program
    #[inline]
    pub fn ancestor_ids(&self, id: NodeId) -> impl Iterator<Item = NodeId> + Clone + '_ {
        match self {
            NodeStorage::Full(nodes) => Either::Left(nodes.ancestor_ids(id)),
            NodeStorage::Ancestors(stack) => Either::Right(stack.ancestor_ids(id)),
        }
    }

    /// Walk up the AST, iterating over each parent [`AstKind`].
    #[inline]
    pub fn ancestor_kinds(&self, id: NodeId) -> impl Iterator<Item = AstKind<'a>> + Clone + '_ {
        self.ancestor_ids(id).map(move |id| self.kind(id))
    }

    /// Walk up the AST, iterating over each parent [`AstNode`].
    #[inline]
    pub fn ancestors(&self, id: NodeId) -> impl Iterator<Item = &AstNode<'a>> + Clone + '_ {
        self.ancestor_ids(id).map(move |id| self.get_node(id))
    }

    /// Walk up the AST, iterating over each parent [`NodeId`] and [`AstNode`].
    #[inline]
    pub fn ancestors_enumerated(
        &self,
        id: NodeId,
    ) -> impl Iterator<Item = (NodeId, &AstNode<'a>)> + Clone + '_ {
        self.ancestor_ids(id).map(move |id| (id, self.get_node(id)))
    }
}
