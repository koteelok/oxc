//! Ancestor-stack backend for [`NodeStorage`](super::NodeStorage).
//!
//! Retains only the live ancestor chain (`root..=current`) during traversal,
//! rather than every node. This serves every *upward* query the binder, the
//! class-table builder, and the syntax checker need (parents and ancestors),
//! while avoiding the per-node allocations of full [`AstNodes`](crate::AstNodes)
//! storage.

use oxc_ast::AstKind;
use oxc_data_structures::stack::Stack;
use oxc_syntax::{
    node::{NodeFlags, NodeId},
    scope::ScopeId,
};

use crate::node::AstNode;

/// A single entry in the [`AncestorStack`].
struct StackEntry<'a> {
    id: NodeId,
    node: AstNode<'a>,
    flags: NodeFlags,
}

/// Stores only the live ancestor chain (`root..=current`) during traversal.
///
/// Pushed on `enter_node`, popped on `leave_node`, so at any point the stack
/// holds the path from the root [`Program`] down to the node currently being
/// visited. This serves every *upward* query the builder needs (parents and
/// ancestors) without retaining the entire tree.
///
/// [`Program`]: oxc_ast::ast::Program
pub struct AncestorStack<'a> {
    /// `stack[0]` is the root, `stack.last()` is the current node.
    ///
    /// Uses the cursor-based [`Stack`] (rather than `Vec`) for fast push / pop /
    /// `last`, which run on every node entered and exited.
    stack: Stack<StackEntry<'a>>,
    /// Total number of nodes created. Doubles as the allocator for the next
    /// [`NodeId`], keeping ids consistent with full storage.
    pub(super) len: u32,
}

impl<'a> AncestorStack<'a> {
    /// Initial capacity, sized to cover the maximum AST nesting depth of typical
    /// code so the stack does not reallocate during a build. The stack only ever
    /// holds the live `root..=current` chain, so this stays tiny regardless of the
    /// total node count.
    const INITIAL_CAPACITY: usize = 128;

    /// Create an empty ancestor stack pre-sized to [`Self::INITIAL_CAPACITY`].
    pub(super) fn new() -> Self {
        Self { stack: Stack::with_capacity(Self::INITIAL_CAPACITY), len: 0 }
    }

    /// Find the stack position of a live ancestor `id`.
    ///
    /// The current node (top of stack) is the overwhelmingly common case and is
    /// checked first. Other ancestors (e.g. a scope's or class's node) require a
    /// scan, but the stack depth equals the AST nesting depth, which is small.
    #[inline]
    fn position(&self, id: NodeId) -> usize {
        if let Some(last) = self.stack.last()
            && last.id == id
        {
            return self.stack.len() - 1;
        }
        self.stack.iter().rposition(|entry| entry.id == id).expect(
            "`NodeId` is not a live ancestor (not available in parent-pointer storage mode)",
        )
    }

    #[inline]
    fn next_id(&mut self) -> NodeId {
        let id = NodeId::new(self.len as usize);
        self.len += 1;
        id
    }

    pub(super) fn add_node(
        &mut self,
        kind: AstKind<'a>,
        scope_id: ScopeId,
        flags: NodeFlags,
    ) -> NodeId {
        let node_id = self.next_id();
        kind.set_node_id(node_id);
        self.stack.push(StackEntry { id: node_id, node: AstNode::new(kind, scope_id), flags });
        node_id
    }

    pub(super) fn add_program_node(
        &mut self,
        kind: AstKind<'a>,
        scope_id: ScopeId,
        flags: NodeFlags,
    ) -> NodeId {
        debug_assert!(self.stack.is_empty(), "Program node must be the first node in the AST.");
        let node_id = self.next_id();
        debug_assert_eq!(node_id, NodeId::ROOT);
        kind.set_node_id(node_id);
        self.stack.push(StackEntry { id: node_id, node: AstNode::new(kind, scope_id), flags });
        node_id
    }

    /// Pop the current node and return its parent's id.
    pub(super) fn pop_node(&mut self) -> NodeId {
        self.stack.pop();
        self.stack.last().map_or(NodeId::ROOT, |entry| entry.id)
    }

    #[inline]
    pub(super) fn get_node(&self, id: NodeId) -> &AstNode<'a> {
        &self.stack[self.position(id)].node
    }

    #[inline]
    pub(super) fn parent_id(&self, id: NodeId) -> NodeId {
        let pos = self.position(id);
        if pos == 0 { NodeId::ROOT } else { self.stack[pos - 1].id }
    }

    #[inline]
    pub(super) fn flags_mut(&mut self, id: NodeId) -> &mut NodeFlags {
        let pos = self.position(id);
        &mut self.stack[pos].flags
    }

    #[inline]
    pub(super) fn ancestor_ids(&self, id: NodeId) -> StackAncestorIdsIter<'_, 'a> {
        StackAncestorIdsIter { stack: self.stack.as_slice(), index: self.position(id) }
    }
}

/// Iterator over the ids of a node's ancestors in [`AncestorStack`].
///
/// Yields the parent first and the root ([`Program`]) last, matching the order
/// used by full [`AstNodes`](crate::AstNodes) storage.
///
/// [`Program`]: oxc_ast::ast::Program
#[derive(Clone)]
pub struct StackAncestorIdsIter<'n, 'a> {
    stack: &'n [StackEntry<'a>],
    /// Position of the node whose ancestors are being yielded. Walks downward.
    index: usize,
}

impl Iterator for StackAncestorIdsIter<'_, '_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == 0 {
            // Root has no parent.
            return None;
        }
        self.index -= 1;
        Some(self.stack[self.index].id)
    }
}
