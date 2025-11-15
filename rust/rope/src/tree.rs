// Copyright 2016 The xi-editor Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A general b-tree structure suitable for ropes and the like.

use std::cmp::{min, Ordering};
use std::marker::PhantomData;
use std::ops::Range;
use std::sync::Arc;

use smallvec::SmallVec;

use crate::interval::{Interval, IntervalBounds};

const MIN_CHILDREN: usize = 4;
const MAX_CHILDREN: usize = 8;

pub trait NodeInfo<L: Leaf>: Clone {
    /// An operator that combines info from two subtrees. It is intended
    /// (but not strictly enforced) that this operator be associative and
    /// obey an identity property. In mathematical terms, the accumulate
    /// method is the operation of a monoid.
    fn accumulate(&mut self, other: &Self);

    /// A mapping from a leaf into the info type. It is intended (but
    /// not strictly enforced) that applying the accumulate method to
    /// the info derived from two leaves gives the same result as
    /// deriving the info from the concatenation of the two leaves. In
    /// mathematical terms, the compute_info method is a monoid
    /// homomorphism.
    fn compute_info(_: &L) -> Self;

    /// The identity of the monoid. Need not be implemented because it
    /// can be computed from the leaf default.
    ///
    /// This is here to demonstrate that this is a monoid.
    fn identity() -> Self {
        Self::compute_info(&L::default())
    }

    /// The interval covered by the first `len` base units of this node. The
    /// default impl is sufficient for most types, but interval trees may need
    /// to override it.
    fn interval(&self, len: usize) -> Interval {
        Interval::new(0, len)
    }
}

/// Provides conversions between the default metric of a node and other metrics.
///
/// Implementors supply the logic used by [`Node::count`] and
/// [`Node::count_base_units`] to translate offsets between metrics.
pub trait DefaultMetricProvider<L: Leaf>: NodeInfo<L> {
    fn convert_from_default<M: Metric<Self, L>>(node: &Node<Self, L>, offset: usize) -> usize;
    fn convert_to_default<M: Metric<Self, L>>(node: &Node<Self, L>, offset: usize) -> usize;
}

/// A trait for the leaves of trees of type [Node](struct.Node.html).
///
/// Two leafs can be concatenated using `push_maybe_split`.
pub trait Leaf: Sized + Clone + Default {
    /// Measurement of leaf in base units.
    /// A 'base unit' refers to the smallest discrete unit
    /// by which a given concrete type can be indexed.
    /// Concretely, for Rust's String type the base unit is the byte.
    fn len(&self) -> usize;

    /// Generally a minimum size requirement for leaves.
    fn is_ok_child(&self) -> bool;

    /// Combine the part `other` denoted by the `Interval` `iv` into `self`,
    /// optionly splitting off a new `Leaf` if `self` would have become too big.
    /// Returns either `None` if no splitting was needed, or `Some(rest)` if
    /// `rest` was split off.
    ///
    /// Interval is in "base units".  Generally implements a maximum size.
    ///
    /// # Invariants:
    /// - If one or the other input is empty, then no split.
    /// - If either input satisfies `is_ok_child`, then, on return, `self`
    ///   satisfies this, as does the optional split.
    fn push_maybe_split(&mut self, other: &Self, iv: Interval) -> Option<Self>;

    /// Same meaning as push_maybe_split starting from an empty
    /// leaf, but maybe can be implemented more efficiently?
    ///
    // TODO: remove if it doesn't pull its weight
    fn subseq(&self, iv: Interval) -> Self {
        let mut result = Self::default();
        if result.push_maybe_split(self, iv).is_some() {
            panic!("unexpected split");
        }
        result
    }
}

/// A b-tree node storing leaves at the bottom, and with info
/// retained at each node. It is implemented with atomic reference counting
/// and copy-on-write semantics, so an immutable clone is a very cheap
/// operation, and nodes can be shared across threads. Even so, it is
/// designed to be updated in place, with efficiency similar to a mutable
/// data structure, using uniqueness of reference count to detect when
/// this operation is safe.
///
/// When the leaf is a string, this is a rope data structure (a persistent
/// rope in functional programming jargon). However, it is not restricted
/// to strings, and it is expected to be the basis for a number of data
/// structures useful for text processing.
/// Internal helper that wraps `Arc<NodeBody>` and centralizes copy-on-write logic.
#[derive(Clone)]
pub(crate) struct SharedNode<N: NodeInfo<L>, L: Leaf> {
    arc: Arc<NodeBody<N, L>>,
}

#[derive(Clone)]
pub struct Node<N: NodeInfo<L>, L: Leaf> {
    shared: SharedNode<N, L>,
}

#[derive(Clone)]
pub(crate) struct NodeBody<N: NodeInfo<L>, L: Leaf> {
    height: usize,
    len: usize,
    info: N,
    val: NodeVal<N, L>,
}

#[derive(Clone)]
enum NodeVal<N: NodeInfo<L>, L: Leaf> {
    Leaf(L),
    Internal(Vec<Node<N, L>>),
}

impl<N: NodeInfo<L>, L: Leaf> SharedNode<N, L> {
    #[inline]
    pub(crate) fn new(body: NodeBody<N, L>) -> Self {
        SharedNode { arc: Arc::new(body) }
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn from_arc(arc: Arc<NodeBody<N, L>>) -> Self {
        SharedNode { arc }
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn into_arc(self) -> Arc<NodeBody<N, L>> {
        self.arc
    }

    #[inline]
    #[allow(dead_code)]
    pub(crate) fn arc(&self) -> &Arc<NodeBody<N, L>> {
        &self.arc
    }

    #[inline]
    pub(crate) fn body(&self) -> &NodeBody<N, L> {
        &self.arc
    }

    #[inline]
    pub(crate) fn ensure_unique(&mut self) -> &mut NodeBody<N, L> {
        Arc::make_mut(&mut self.arc)
    }

    #[inline]
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.arc, &other.arc)
    }

    pub(crate) fn from_children(children: Vec<Node<N, L>>) -> Self {
        debug_assert!(children.len() > 1);
        debug_assert!(children.len() <= MAX_CHILDREN);
        let height = children[0].height() + 1;
        let expected_child_height = height - 1;
        let mut iter = children.iter();
        let first = iter.next().unwrap();
        debug_assert_eq!(first.height(), expected_child_height);
        debug_assert!(first.is_ok_child());

        let mut len = first.len();
        let mut info = first.body().info.clone();
        for child in iter {
            debug_assert_eq!(child.height(), expected_child_height);
            debug_assert!(child.is_ok_child());
            len += child.len();
            info.accumulate(&child.body().info);
        }

        SharedNode::new(NodeBody { height, len, info, val: NodeVal::Internal(children) })
    }

    pub(crate) fn clone_with_children(&self, children: &[Node<N, L>]) -> SharedNode<N, L> {
        debug_assert!(self.body().height > 0, "cannot clone leaf with children");
        debug_assert!(!children.is_empty());
        debug_assert!(children.len() <= MAX_CHILDREN);
        let expected_child_height = self.body().height - 1;

        let mut iter = children.iter();
        let first = iter.next().unwrap();
        debug_assert_eq!(first.height(), expected_child_height);

        let mut len = first.len();
        let mut info = first.body().info.clone();
        for child in iter {
            debug_assert_eq!(child.height(), expected_child_height);
            len += child.len();
            info.accumulate(&child.body().info);
        }

        SharedNode::new(NodeBody {
            height: self.body().height,
            len,
            info,
            val: NodeVal::Internal(children.to_vec()),
        })
    }

    pub(crate) fn replace_child_range(&mut self, range: Range<usize>, replacements: &[Node<N, L>]) {
        let body_height = self.body().height;
        debug_assert!(body_height > 0, "cannot replace children on a leaf node");
        let expected_child_height = body_height - 1;
        let range_start = range.start;
        let range_end = range.end;
        let body = self.ensure_unique();
        {
            let NodeVal::Internal(ref mut children) = body.val else {
                panic!("replace_child_range called on leaf node");
            };
            debug_assert!(range_start <= range_end && range_end <= children.len());
            for child in replacements {
                debug_assert_eq!(child.height(), expected_child_height);
            }
            children.splice(range_start..range_end, replacements.iter().cloned());
        }
        SharedNode::refresh_len_info(body);
    }

    fn refresh_len_info(body: &mut NodeBody<N, L>) {
        match &body.val {
            NodeVal::Leaf(l) => {
                body.len = l.len();
                body.info = N::compute_info(l);
            }
            NodeVal::Internal(children) => {
                if let Some(first) = children.first() {
                    let mut len = first.len();
                    let mut info = first.body().info.clone();
                    for child in &children[1..] {
                        len += child.len();
                        info.accumulate(&child.body().info);
                    }
                    body.len = len;
                    body.info = info;
                } else {
                    body.len = 0;
                    body.info = N::identity();
                }
            }
        }
    }
}

// also consider making Metric a newtype for usize, so type system can
// help separate metrics

/// A trait for quickly processing attributes of a
/// [NodeInfo](struct.NodeInfo.html).
///
/// For the conceptual background see the
/// [blog post, Rope science, part 2: metrics](https://github.com/google/xi-editor/blob/master/docs/docs/rope_science_02.md).
pub trait Metric<N: NodeInfo<L>, L: Leaf> {
    /// Return the size of the
    /// leaf as measured by this metric.
    ///
    /// The usize argument is the total size/length of the node, in base units.
    ///
    /// # Examples
    /// For the [LinesMetric](../rope/struct.LinesMetric.html), this gives the number of
    /// lines in string contained in the leaf. For the
    /// [BaseMetric](../rope/struct.BaseMetric.html), this gives the size of the string
    /// in uft8 code units, that is, bytes.
    ///
    fn measure(info: &N, len: usize) -> usize;

    /// Returns the smallest offset, in base units, for an offset in measured units.
    ///
    /// # Invariants:
    ///
    /// - `from_base_units(to_base_units(x)) == x` is True for valid `x`
    fn to_base_units(l: &L, in_measured_units: usize) -> usize;

    /// Returns the smallest offset in measured units corresponding to an offset in base units.
    ///
    /// # Invariants:
    ///
    /// - `from_base_units(to_base_units(x)) == x` is True for valid `x`
    fn from_base_units(l: &L, in_base_units: usize) -> usize;

    /// Return whether the offset in base units is a boundary of this metric.
    /// If a boundary is at end of a leaf then this method must return true.
    /// However, a boundary at the beginning of a leaf is optional
    /// (the previous leaf will be queried).
    fn is_boundary(l: &L, offset: usize) -> bool;

    /// Returns the index of the boundary directly preceding offset,
    /// or None if no such boundary exists. Input and result are in base units.
    fn prev(l: &L, offset: usize) -> Option<usize>;

    /// Returns the index of the first boundary for which index > offset,
    /// or None if no such boundary exists. Input and result are in base units.
    fn next(l: &L, offset: usize) -> Option<usize>;

    /// Returns true if the measured units in this metric can span multiple
    /// leaves.  As an example, in a metric that measures lines in a rope, a
    /// line may start in one leaf and end in another; however in a metric
    /// measuring bytes, storage of a single byte cannot extend across leaves.
    fn can_fragment() -> bool;
}

impl<N: NodeInfo<L>, L: Leaf> Node<N, L> {
    #[inline]
    fn from_shared(shared: SharedNode<N, L>) -> Self {
        Node { shared }
    }

    #[inline]
    pub(crate) fn shared(&self) -> &SharedNode<N, L> {
        &self.shared
    }

    #[inline]
    pub(crate) fn shared_mut(&mut self) -> &mut SharedNode<N, L> {
        &mut self.shared
    }

    #[inline]
    pub(crate) fn body(&self) -> &NodeBody<N, L> {
        self.shared.body()
    }

    pub fn from_leaf(l: L) -> Node<N, L> {
        let len = l.len();
        let info = N::compute_info(&l);
        Node::from_shared(SharedNode::new(NodeBody { height: 0, len, info, val: NodeVal::Leaf(l) }))
    }

    /// Create a node from a vec of nodes.
    ///
    /// The input must satisfy the following balancing requirements:
    /// * The length of `nodes` must be <= MAX_CHILDREN and > 1.
    /// * All the nodes are the same height.
    /// * All the nodes must satisfy is_ok_child.
    fn from_nodes(nodes: Vec<Node<N, L>>) -> Node<N, L> {
        Node::from_shared(SharedNode::from_children(nodes))
    }

    pub fn len(&self) -> usize {
        self.body().len
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns `true` if these two `Node`s share the same underlying data.
    ///
    /// This is principally intended to be used by the druid crate, without needing
    /// to actually add a feature and implement druid's `Data` trait.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        self.shared.ptr_eq(other.shared())
    }

    fn height(&self) -> usize {
        self.body().height
    }

    fn is_leaf(&self) -> bool {
        self.height() == 0
    }

    fn interval(&self) -> Interval {
        self.body().info.interval(self.body().len)
    }

    fn get_children(&self) -> &[Node<N, L>] {
        if let NodeVal::Internal(ref v) = self.body().val {
            v
        } else {
            panic!("get_children called on leaf node");
        }
    }

    fn get_leaf(&self) -> &L {
        if let NodeVal::Leaf(ref l) = self.body().val {
            l
        } else {
            panic!("get_leaf called on internal node");
        }
    }

    /// Call a callback with a mutable reference to a leaf.
    ///
    /// This clones the leaf if the reference is shared. It also recomputes
    /// length and info after the leaf is mutated.
    fn with_leaf_mut<T>(&mut self, f: impl FnOnce(&mut L) -> T) -> T {
        let body = self.shared.ensure_unique();
        if let NodeVal::Leaf(ref mut l) = body.val {
            let result = f(l);
            body.len = l.len();
            body.info = N::compute_info(l);
            result
        } else {
            panic!("with_leaf_mut called on internal node");
        }
    }

    fn is_ok_child(&self) -> bool {
        match self.body().val {
            NodeVal::Leaf(ref l) => l.is_ok_child(),
            NodeVal::Internal(ref nodes) => nodes.len() >= MIN_CHILDREN,
        }
    }

    fn merge_nodes(children1: &[Node<N, L>], children2: &[Node<N, L>]) -> Node<N, L> {
        let n_children = children1.len() + children2.len();
        if n_children <= MAX_CHILDREN {
            Node::from_nodes([children1, children2].concat())
        } else {
            // Note: this leans left. Splitting at midpoint is also an option
            let splitpoint = min(MAX_CHILDREN, n_children - MIN_CHILDREN);
            let mut iter = children1.iter().chain(children2.iter()).cloned();
            let left = iter.by_ref().take(splitpoint).collect();
            let right = iter.collect();
            let parent_nodes = vec![Node::from_nodes(left), Node::from_nodes(right)];
            Node::from_nodes(parent_nodes)
        }
    }

    fn merge_leaves(mut rope1: Node<N, L>, rope2: Node<N, L>) -> Node<N, L> {
        debug_assert!(rope1.is_leaf() && rope2.is_leaf());

        let both_ok = rope1.get_leaf().is_ok_child() && rope2.get_leaf().is_ok_child();
        if both_ok {
            return Node::from_nodes(vec![rope1, rope2]);
        }
        let res = {
            let body = rope1.shared.ensure_unique();
            let leaf2 = rope2.get_leaf();
            if let NodeVal::Leaf(ref mut leaf1) = body.val {
                let leaf2_iv = Interval::new(0, leaf2.len());
                let new = leaf1.push_maybe_split(leaf2, leaf2_iv);
                body.len = leaf1.len();
                body.info = N::compute_info(leaf1);
                new
            } else {
                panic!("merge_leaves called on non-leaf");
            }
        };
        match res {
            Some(new) => Node::from_nodes(vec![rope1, Node::from_leaf(new)]),
            None => rope1,
        }
    }

    pub fn concat(rope1: Node<N, L>, rope2: Node<N, L>) -> Node<N, L> {
        let h1 = rope1.height();
        let h2 = rope2.height();

        match h1.cmp(&h2) {
            Ordering::Less => {
                let children2 = rope2.get_children();
                if h1 == h2 - 1 && rope1.is_ok_child() {
                    return Node::merge_nodes(&[rope1], children2);
                }
                let newrope = Node::concat(rope1, children2[0].clone());
                if newrope.height() == h2 - 1 {
                    Node::merge_nodes(&[newrope], &children2[1..])
                } else {
                    Node::merge_nodes(newrope.get_children(), &children2[1..])
                }
            }
            Ordering::Equal => {
                if rope1.is_ok_child() && rope2.is_ok_child() {
                    return Node::from_nodes(vec![rope1, rope2]);
                }
                if h1 == 0 {
                    return Node::merge_leaves(rope1, rope2);
                }
                Node::merge_nodes(rope1.get_children(), rope2.get_children())
            }
            Ordering::Greater => {
                let children1 = rope1.get_children();
                if h2 == h1 - 1 && rope2.is_ok_child() {
                    return Node::merge_nodes(children1, &[rope2]);
                }
                let lastix = children1.len() - 1;
                let newrope = Node::concat(children1[lastix].clone(), rope2);
                if newrope.height() == h1 - 1 {
                    Node::merge_nodes(&children1[..lastix], &[newrope])
                } else {
                    Node::merge_nodes(&children1[..lastix], newrope.get_children())
                }
            }
        }
    }

    pub fn measure<M: Metric<N, L>>(&self) -> usize {
        M::measure(&self.body().info, self.body().len)
    }

    pub fn subseq<T: IntervalBounds>(&self, iv: T) -> Node<N, L> {
        let iv = iv.into_interval(self.len());
        let mut b = TreeBuilder::new();
        b.push_slice(self, iv);
        b.build()
    }

    pub fn edit<T, IV>(&mut self, iv: IV, new: T)
    where
        T: Into<Node<N, L>>,
        IV: IntervalBounds,
    {
        let mut b = TreeBuilder::new();
        let iv = iv.into_interval(self.len());
        let self_iv = self.interval();
        b.push_slice(self, self_iv.prefix(iv));
        b.push(new.into());
        b.push_slice(self, self_iv.suffix(iv));
        *self = b.build();
    }

    // doesn't deal with endpoint, handle that specially if you need it
    pub fn convert_metrics<M1: Metric<N, L>, M2: Metric<N, L>>(&self, mut m1: usize) -> usize {
        if m1 == 0 {
            return 0;
        }
        // If M1 can fragment, then we must land on the leaf containing
        // the m1 boundary. Otherwise, we can land on the beginning of
        // the leaf immediately following the M1 boundary, which may be
        // more efficient.
        let m1_fudge = if M1::can_fragment() { 1 } else { 0 };
        let mut m2 = 0;
        let mut node = self;
        while node.height() > 0 {
            for child in node.get_children() {
                let child_m1 = child.measure::<M1>();
                if m1 < child_m1 + m1_fudge {
                    node = child;
                    break;
                }
                m2 += child.measure::<M2>();
                m1 -= child_m1;
            }
        }
        let l = node.get_leaf();
        let base = M1::to_base_units(l, m1);
        m2 + M2::from_base_units(l, base)
    }
}

impl<N: DefaultMetricProvider<L>, L: Leaf> Node<N, L> {
    /// Measures the length of the text bounded by the default metric offset using another metric.
    ///
    /// # Examples
    /// ```
    /// use crate::xi_rope::{Rope, LinesMetric};
    ///
    /// // the default metric of Rope is BaseMetric (aka number of bytes)
    /// let my_rope = Rope::from("first line \n second line \n");
    ///
    /// // count the number of lines in my_rope
    /// let num_lines = my_rope.count::<LinesMetric>(my_rope.len());
    /// assert_eq!(2, num_lines);
    /// ```
    pub fn count<M: Metric<N, L>>(&self, offset: usize) -> usize {
        N::convert_from_default::<M>(self, offset)
    }

    /// Measures the length of the text bounded by another metric using the default metric.
    ///
    /// # Examples
    /// ```
    /// use crate::xi_rope::{Rope, LinesMetric};
    ///
    /// // the default metric of Rope is BaseMetric (aka number of bytes)
    /// let my_rope = Rope::from("first line \n second line \n");
    ///
    /// // get the byte offset of the line at index 1
    /// let byte_offset = my_rope.count_base_units::<LinesMetric>(1);
    /// assert_eq!(12, byte_offset);
    /// ```
    pub fn count_base_units<M: Metric<N, L>>(&self, offset: usize) -> usize {
        N::convert_to_default::<M>(self, offset)
    }
}

impl<N: NodeInfo<L>, L: Leaf> Default for Node<N, L> {
    fn default() -> Node<N, L> {
        Node::from_leaf(L::default())
    }
}

/// A builder for creating new trees.
pub struct TreeBuilder<N: NodeInfo<L>, L: Leaf> {
    // A stack of partially built trees. These are kept in order of
    // strictly descending height, and all vectors have a length less
    // than MAX_CHILDREN and greater than zero.
    //
    // In addition, there is a balancing invariant: for each vector
    // of length greater than one, all elements satisfy `is_ok_child`.
    stack: Vec<Vec<Node<N, L>>>,
}

impl<N: NodeInfo<L>, L: Leaf> TreeBuilder<N, L> {
    /// A new, empty builder.
    pub fn new() -> TreeBuilder<N, L> {
        TreeBuilder { stack: Vec::new() }
    }

    /// Append a node to the tree being built.
    pub fn push(&mut self, mut n: Node<N, L>) {
        loop {
            let ord = if let Some(last) = self.stack.last() {
                last[0].height().cmp(&n.height())
            } else {
                Ordering::Greater
            };
            match ord {
                Ordering::Less => {
                    n = Node::concat(self.pop(), n);
                }
                Ordering::Equal => {
                    let tos = self.stack.last_mut().unwrap();
                    if tos.last().unwrap().is_ok_child() && n.is_ok_child() {
                        tos.push(n);
                    } else if n.height() == 0 {
                        let iv = Interval::new(0, n.len());
                        let new_leaf = tos
                            .last_mut()
                            .unwrap()
                            .with_leaf_mut(|l| l.push_maybe_split(n.get_leaf(), iv));
                        if let Some(new_leaf) = new_leaf {
                            tos.push(Node::from_leaf(new_leaf));
                        }
                    } else {
                        let mut last = tos.pop().unwrap();
                        let existing_len = last.get_children().len();
                        let new_children: Vec<Node<N, L>> =
                            n.get_children().iter().cloned().collect();
                        let total_children = existing_len + new_children.len();
                        if total_children <= MAX_CHILDREN {
                            last.shared_mut()
                                .replace_child_range(existing_len..existing_len, &new_children);
                            tos.push(last);
                        } else {
                            // Note: this leans left. Splitting at midpoint is also an option
                            let mut combined: Vec<Node<N, L>> =
                                last.get_children().iter().cloned().collect();
                            combined.extend(new_children);
                            let splitpoint = min(MAX_CHILDREN, total_children - MIN_CHILDREN);
                            let left_nodes = combined[..splitpoint].to_vec();
                            let right_nodes = combined[splitpoint..].to_vec();
                            let left =
                                Node::from_shared(last.shared().clone_with_children(&left_nodes));
                            let right =
                                Node::from_shared(last.shared().clone_with_children(&right_nodes));
                            tos.push(left);
                            tos.push(right);
                        }
                    }
                    if tos.len() < MAX_CHILDREN {
                        break;
                    }
                    n = self.pop()
                }
                Ordering::Greater => {
                    self.stack.push(vec![n]);
                    break;
                }
            }
        }
    }

    /// Push a subsequence of a rope.
    ///
    /// Pushes the subsequence of another tree `n` defined by the interval `iv`
    /// onto the builder.
    ///
    /// This is intended as an efficient operation. It is equivalent to taking
    /// the subsequence of `n` and pushing that, but attempts to minimize the
    /// allocation of intermediate results.
    pub fn push_slice(&mut self, n: &Node<N, L>, iv: Interval) {
        if iv.is_empty() {
            return;
        }
        if iv == n.interval() {
            self.push(n.clone());
            return;
        }
        match &n.body().val {
            NodeVal::Leaf(l) => {
                self.push_leaf_slice(l, iv);
            }
            NodeVal::Internal(v) => {
                let mut offset = 0;
                for child in v {
                    if iv.is_before(offset) {
                        break;
                    }
                    let child_iv = child.interval();
                    // easier just to use signed ints?
                    let rec_iv = iv.intersect(child_iv.translate(offset)).translate_neg(offset);
                    self.push_slice(child, rec_iv);
                    offset += child.len();
                }
            }
        }
    }

    /// Append a sequence of leaves.
    pub fn push_leaves(&mut self, leaves: impl IntoIterator<Item = L>) {
        for leaf in leaves.into_iter() {
            self.push(Node::from_leaf(leaf));
        }
    }

    /// Append a single leaf.
    pub fn push_leaf(&mut self, l: L) {
        self.push(Node::from_leaf(l))
    }

    /// Append a slice of a single leaf.
    pub fn push_leaf_slice(&mut self, l: &L, iv: Interval) {
        self.push(Node::from_leaf(l.subseq(iv)))
    }

    /// Build the final tree.
    ///
    /// The tree is the concatenation of all the nodes and leaves that have been pushed
    /// on the builder, in order.
    pub fn build(mut self) -> Node<N, L> {
        if self.stack.is_empty() {
            Node::from_leaf(L::default())
        } else {
            let mut n = self.pop();
            while !self.stack.is_empty() {
                n = Node::concat(self.pop(), n);
            }
            n
        }
    }

    /// Pop the last vec-of-nodes off the stack, resulting in a node.
    fn pop(&mut self) -> Node<N, L> {
        let nodes = self.stack.pop().unwrap();
        if nodes.len() == 1 {
            nodes.into_iter().next().unwrap()
        } else {
            Node::from_nodes(nodes)
        }
    }
}

const CURSOR_CACHE_SIZE: usize = 4;

/// A cached frame representing the relationship between a parent node and the
/// child traversed by the cursor when descending the tree.
///
/// Frames are stored from root to leaf and keep enough information to rebuild
/// cached offsets without walking sibling lengths again.
#[derive(Clone)]
pub struct PathFrame<N: NodeInfo<L>, L: Leaf> {
    node: Arc<NodeBody<N, L>>,
    child_index: usize,
    child_offset: usize,
}

impl<N: NodeInfo<L>, L: Leaf> PathFrame<N, L> {
    fn new(node: &Node<N, L>, child_index: usize, child_offset: usize) -> Self {
        PathFrame { node: clone_node_arc(node), child_index, child_offset }
    }

    pub fn ptr_eq(&self, other: &Node<N, L>) -> bool {
        Arc::ptr_eq(&self.node, other.shared.arc())
    }

    pub fn child_index(&self) -> usize {
        self.child_index
    }

    pub fn child_offset(&self) -> usize {
        self.child_offset
    }
}

/// A borrow-free snapshot of a cursor's cached state.
///
/// The descriptor can be used to rebuild a [`Cursor`] at the same position, as
/// long as the underlying nodes are still valid (checked with `Arc::ptr_eq`).
pub struct CursorDescriptor<N: NodeInfo<L>, L: Leaf> {
    position: usize,
    offset_of_leaf: usize,
    leaf: Option<Arc<NodeBody<N, L>>>,
    frames: SmallVec<[PathFrame<N, L>; CURSOR_CACHE_SIZE]>,
}

impl<N: NodeInfo<L>, L: Leaf> CursorDescriptor<N, L> {
    fn new_invalid(position: usize) -> Self {
        CursorDescriptor { position, offset_of_leaf: 0, leaf: None, frames: SmallVec::new() }
    }

    fn new(
        position: usize,
        offset_of_leaf: usize,
        leaf: Arc<NodeBody<N, L>>,
        frames: SmallVec<[PathFrame<N, L>; CURSOR_CACHE_SIZE]>,
    ) -> Self {
        CursorDescriptor { position, offset_of_leaf, leaf: Some(leaf), frames }
    }

    /// Returns the cached depth (number of parent frames) stored in the descriptor.
    pub fn depth(&self) -> usize {
        self.frames.len()
    }

    /// Returns whether the descriptor holds a valid leaf reference.
    pub fn is_valid(&self) -> bool {
        self.leaf.is_some()
    }

    /// Returns the absolute cursor position captured by this descriptor.
    pub fn position(&self) -> usize {
        self.position
    }

    /// Returns the absolute offset of the current leaf within the tree.
    pub fn offset_of_leaf(&self) -> usize {
        self.offset_of_leaf
    }

    /// Returns the frames describing the cached path from root to leaf.
    pub fn frames(&self) -> &[PathFrame<N, L>] {
        &self.frames
    }

    /// Restores a [`Cursor`] from this descriptor if the cached nodes still belong to `root`.
    pub fn restore<'a>(&self, root: &'a Node<N, L>) -> Option<Cursor<'a, N, L>> {
        if !self.is_valid() {
            return None;
        }
        let mut cursor = Cursor {
            root,
            position: self.position,
            cache: [None; CURSOR_CACHE_SIZE],
            leaf: None,
            offset_of_leaf: 0,
            #[cfg(feature = "cursor_state")]
            state: CursorState::new_invalid(self.position, 0),
        };
        if cursor.apply_descriptor(self) {
            Some(cursor)
        } else {
            None
        }
    }
}

#[cfg(feature = "cursor_state")]
#[derive(Clone)]
pub struct CursorState<N: NodeInfo<L>, L: Leaf> {
    position: usize,
    offset_of_leaf: usize,
    leaf: Option<Arc<NodeBody<N, L>>>,
    frames: SmallVec<[PathFrame<N, L>; CURSOR_CACHE_SIZE]>,
}

/// A data structure for traversing boundaries in a tree.
///
/// It is designed to be efficient both for random access and for iteration. The
/// cursor itself is agnostic to which [`Metric`] is used to determine boundaries, but
/// the methods to find boundaries are parametrized on the [`Metric`].
///
/// A cursor can be valid or invalid. It is always valid when created or after
/// [`set`](#method.set) is called, and becomes invalid after [`prev`](#method.prev)
/// or [`next`](#method.next) fails to find a boundary.
///
/// [`Metric`]: struct.Metric.html
pub struct Cursor<'a, N: NodeInfo<L> + 'a, L: Leaf> {
    /// The tree being traversed by this cursor.
    root: &'a Node<N, L>,
    /// The current position of the cursor.
    ///
    /// It is always less than or equal to the tree length.
    position: usize,
    /// The cache holds the tail of the path from the root to the current leaf.
    ///
    /// Each entry is a reference to the parent node and the index of the child. It
    /// is stored bottom-up; `cache[0]` is the parent of the leaf and the index of
    /// the leaf within that parent.
    ///
    /// The main motivation for this being a fixed-size array is to keep the cursor
    /// an allocation-free data structure.
    cache: [Option<(&'a Node<N, L>, usize)>; CURSOR_CACHE_SIZE],
    /// The leaf containing the current position, when the cursor is valid.
    ///
    /// The position is only at the end of the leaf when it is at the end of the tree.
    leaf: Option<&'a L>,
    /// The offset of `leaf` within the tree.
    offset_of_leaf: usize,
    #[cfg(feature = "cursor_state")]
    state: CursorState<N, L>,
}

impl<'a, N: NodeInfo<L>, L: Leaf> Cursor<'a, N, L> {
    /// Create a new cursor at the given position.
    pub fn new(n: &'a Node<N, L>, position: usize) -> Cursor<'a, N, L> {
        let mut result = Cursor {
            root: n,
            position,
            cache: [None; CURSOR_CACHE_SIZE],
            leaf: None,
            offset_of_leaf: 0,
            #[cfg(feature = "cursor_state")]
            state: CursorState::new_invalid(position, 0),
        };
        result.descend();
        result
    }

    /// The length of the tree.
    pub fn total_len(&self) -> usize {
        self.root.len()
    }

    /// Return a reference to the root node of the tree.
    pub fn root(&self) -> &'a Node<N, L> {
        self.root
    }

    #[cfg(feature = "cursor_state")]
    pub fn state(&self) -> CursorState<N, L> {
        self.state.clone()
    }

    /// Get the current leaf of the cursor.
    ///
    /// If the cursor is valid, returns the leaf containing the current position,
    /// and the offset of the current position within the leaf. That offset is equal
    /// to the leaf length only at the end, otherwise it is less than the leaf length.
    pub fn get_leaf(&self) -> Option<(&'a L, usize)> {
        self.leaf.map(|l| (l, self.position - self.offset_of_leaf))
    }

    /// Set the position of the cursor.
    ///
    /// The cursor is valid after this call.
    ///
    /// Precondition: `position` is less than or equal to the length of the tree.
    pub fn set(&mut self, position: usize) {
        self.position = position;
        if let Some(l) = self.leaf {
            if self.position >= self.offset_of_leaf && self.position < self.offset_of_leaf + l.len()
            {
                #[cfg(feature = "cursor_state")]
                self.update_state_position();
                return;
            }
        }
        // TODO: walk up tree to find leaf if nearby
        self.descend();
    }

    /// Get the position of the cursor.
    pub fn pos(&self) -> usize {
        self.position
    }

    /// Creates a [`CursorDescriptor`] snapshot of the current cursor state.
    ///
    /// The descriptor owns all cached path information, allowing the cursor to
    /// be reconstructed later without holding borrows into the tree. When the
    /// cursor is invalid, the returned descriptor will also be marked invalid
    /// and `restore`/`apply_descriptor` will return failure.
    pub fn to_descriptor(&self) -> CursorDescriptor<N, L> {
        if self.leaf.is_none() {
            return CursorDescriptor::new_invalid(self.position);
        }
        let (frames, leaf_arc, offset_of_leaf, offset_in_leaf) =
            build_descriptor_components(self.root, self.position);
        debug_assert_eq!(offset_of_leaf, self.offset_of_leaf);
        debug_assert_eq!(offset_in_leaf, self.position - self.offset_of_leaf);
        CursorDescriptor::new(self.position, offset_of_leaf, leaf_arc, frames)
    }

    /// Attempts to repopulate the cursor's cache from a descriptor.
    ///
    /// Returns `true` if the descriptor was still valid for the current tree and
    /// the cursor was updated. On failure the cursor is left unchanged so the
    /// caller can fall back to a fresh descent.
    pub fn apply_descriptor(&mut self, descriptor: &CursorDescriptor<N, L>) -> bool {
        if !descriptor.is_valid() {
            return false;
        }
        if descriptor.position > self.root.len() {
            return false;
        }
        if descriptor.offset_of_leaf > descriptor.position {
            return false;
        }

        let mut node = self.root;
        let mut nodes_for_cache: SmallVec<[&Node<N, L>; CURSOR_CACHE_SIZE]> = SmallVec::new();
        let mut accumulated_offset = 0usize;

        for frame in descriptor.frames.iter() {
            if !frame.ptr_eq(node) {
                return false;
            }
            let children = node.get_children();
            let child_index = frame.child_index();
            if child_index >= children.len() {
                return false;
            }
            let computed_offset: usize = children.iter().take(child_index).map(Node::len).sum();
            if computed_offset != frame.child_offset() {
                return false;
            }
            accumulated_offset += frame.child_offset();
            nodes_for_cache.push(node);
            node = &children[child_index];
        }

        let leaf_arc = descriptor.leaf.as_ref().unwrap();
        if !Arc::ptr_eq(leaf_arc, node.shared.arc()) {
            return false;
        }

        if accumulated_offset != descriptor.offset_of_leaf {
            return false;
        }
        let offset_in_leaf = descriptor.position - descriptor.offset_of_leaf;
        if offset_in_leaf > node.len() {
            return false;
        }

        let mut new_cache: [Option<(&Node<N, L>, usize)>; CURSOR_CACHE_SIZE] =
            [None; CURSOR_CACHE_SIZE];
        for (slot, (parent, frame)) in
            nodes_for_cache.iter().rev().zip(descriptor.frames.iter().rev()).enumerate()
        {
            if slot >= CURSOR_CACHE_SIZE {
                break;
            }
            new_cache[slot] = Some((*parent, frame.child_index()));
        }

        self.position = descriptor.position;
        self.cache = new_cache;
        self.set_leaf_from_node(node, descriptor.offset_of_leaf);
        true
    }

    /// Determine whether the current position is a boundary.
    ///
    /// Note: the beginning and end of the tree may or may not be boundaries, depending on the
    /// metric. If the metric is not `can_fragment`, then they always are.
    pub fn is_boundary<M: Metric<N, L>>(&mut self) -> bool {
        if self.leaf.is_none() {
            // not at a valid position
            return false;
        }
        if self.position == self.offset_of_leaf && !M::can_fragment() {
            return true;
        }
        if self.position == 0 || self.position > self.offset_of_leaf {
            return M::is_boundary(self.leaf.unwrap(), self.position - self.offset_of_leaf);
        }
        // tricky case, at beginning of leaf, need to query end of previous
        // leaf; TODO: would be nice if we could do it another way that didn't
        // make the method &mut self.
        let l = self.prev_leaf().unwrap().0;
        let result = M::is_boundary(l, l.len());
        let _ = self.next_leaf();
        result
    }

    /// Moves the cursor to the previous boundary.
    ///
    /// When there is no previous boundary, returns `None` and the cursor becomes invalid.
    ///
    /// Return value: the position of the boundary, if it exists.
    pub fn prev<M: Metric<N, L>>(&mut self) -> Option<usize> {
        if self.position == 0 || self.leaf.is_none() {
            self.leaf = None;
            self.offset_of_leaf = self.position.min(self.root.len());
            #[cfg(feature = "cursor_state")]
            self.invalidate_state();
            return None;
        }
        let orig_pos = self.position;
        let offset_in_leaf = orig_pos - self.offset_of_leaf;
        if offset_in_leaf > 0 {
            let l = self.leaf.unwrap();
            if let Some(offset_in_leaf) = M::prev(l, offset_in_leaf) {
                self.position = self.offset_of_leaf + offset_in_leaf;
                return Some(self.position);
            }
        }

        // not in same leaf, need to scan backwards
        self.prev_leaf()?;
        if let Some(offset) = self.last_inside_leaf::<M>(orig_pos) {
            return Some(offset);
        }

        // Not found in previous leaf, find using measurement.
        let measure = self.measure_leaf::<M>(self.position);
        if measure == 0 {
            self.leaf = None;
            self.position = 0;
            self.offset_of_leaf = 0;
            #[cfg(feature = "cursor_state")]
            self.invalidate_state();
            return None;
        }
        self.descend_metric::<M>(measure);
        self.last_inside_leaf::<M>(orig_pos)
    }

    /// Moves the cursor to the next boundary.
    ///
    /// When there is no next boundary, returns `None` and the cursor becomes invalid.
    ///
    /// Return value: the position of the boundary, if it exists.
    pub fn next<M: Metric<N, L>>(&mut self) -> Option<usize> {
        if self.position >= self.root.len() || self.leaf.is_none() {
            self.leaf = None;
            self.position = self.position.min(self.root.len());
            self.offset_of_leaf = self.position;
            #[cfg(feature = "cursor_state")]
            self.invalidate_state();
            return None;
        }

        if let Some(offset) = self.next_inside_leaf::<M>() {
            return Some(offset);
        }

        self.next_leaf()?;
        if let Some(offset) = self.next_inside_leaf::<M>() {
            return Some(offset);
        }

        // Leaf is 0-measure (otherwise would have already succeeded).
        let measure = self.measure_leaf::<M>(self.position);
        self.descend_metric::<M>(measure + 1);
        if let Some(offset) = self.next_inside_leaf::<M>() {
            return Some(offset);
        }

        // Not found, properly invalidate cursor.
        self.position = self.root.len();
        self.leaf = None;
        self.offset_of_leaf = self.position;
        #[cfg(feature = "cursor_state")]
        self.invalidate_state();
        None
    }

    /// Returns the current position if it is a boundary in this [`Metric`],
    /// else behaves like [`next`](#method.next).
    ///
    /// [`Metric`]: struct.Metric.html
    pub fn at_or_next<M: Metric<N, L>>(&mut self) -> Option<usize> {
        if self.is_boundary::<M>() {
            Some(self.pos())
        } else {
            self.next::<M>()
        }
    }

    /// Returns the current position if it is a boundary in this [`Metric`],
    /// else behaves like [`prev`](#method.prev).
    ///
    /// [`Metric`]: struct.Metric.html
    pub fn at_or_prev<M: Metric<N, L>>(&mut self) -> Option<usize> {
        if self.is_boundary::<M>() {
            Some(self.pos())
        } else {
            self.prev::<M>()
        }
    }

    /// Returns an iterator with this cursor over the given [`Metric`].
    ///
    /// # Examples:
    ///
    /// ```
    /// # use xi_rope::{Cursor, LinesMetric, Rope};
    /// #
    /// let text: Rope = "one line\ntwo line\nred line\nblue".into();
    /// let mut cursor = Cursor::new(&text, 0);
    /// let line_offsets = cursor.iter::<LinesMetric>().collect::<Vec<_>>();
    /// assert_eq!(line_offsets, vec![9, 18, 27]);
    ///
    /// ```
    /// [`Metric`]: struct.Metric.html
    pub fn iter<'c, M: Metric<N, L>>(&'c mut self) -> CursorIter<'c, 'a, N, L, M> {
        CursorIter { cursor: self, _metric: PhantomData }
    }

    /// Tries to find the last boundary in the leaf the cursor is currently in.
    ///
    /// If the last boundary is at the end of the leaf, it is only counted if
    /// it is less than `orig_pos`.
    #[inline]
    fn last_inside_leaf<M: Metric<N, L>>(&mut self, orig_pos: usize) -> Option<usize> {
        let l = self.leaf.expect("inconsistent, shouldn't get here");
        let len = l.len();
        if self.offset_of_leaf + len < orig_pos && M::is_boundary(l, len) {
            let _ = self.next_leaf();
            return Some(self.position);
        }
        let offset_in_leaf = M::prev(l, len)?;
        self.position = self.offset_of_leaf + offset_in_leaf;
        #[cfg(feature = "cursor_state")]
        self.update_state_position();
        Some(self.position)
    }

    /// Tries to find the next boundary in the leaf the cursor is currently in.
    #[inline]
    fn next_inside_leaf<M: Metric<N, L>>(&mut self) -> Option<usize> {
        let l = self.leaf.expect("inconsistent, shouldn't get here");
        let offset_in_leaf = self.position - self.offset_of_leaf;
        let offset_in_leaf = M::next(l, offset_in_leaf)?;
        if offset_in_leaf == l.len() && self.offset_of_leaf + offset_in_leaf != self.root.len() {
            let _ = self.next_leaf();
        } else {
            self.position = self.offset_of_leaf + offset_in_leaf;
            #[cfg(feature = "cursor_state")]
            self.update_state_position();
        }
        Some(self.position)
    }

    /// Move to beginning of next leaf.
    ///
    /// Return value: same as [`get_leaf`](#method.get_leaf).
    pub fn next_leaf(&mut self) -> Option<(&'a L, usize)> {
        let leaf = self.leaf?;
        let new_offset = self.offset_of_leaf + leaf.len();
        self.position = new_offset;
        for i in 0..CURSOR_CACHE_SIZE {
            if self.cache[i].is_none() {
                // this probably can't happen
                self.leaf = None;
                self.offset_of_leaf = self.position.min(self.root.len());
                #[cfg(feature = "cursor_state")]
                self.invalidate_state();
                return None;
            }
            let (node, j) = self.cache[i].unwrap();
            if j + 1 < node.get_children().len() {
                self.cache[i] = Some((node, j + 1));
                let mut node_down = &node.get_children()[j + 1];
                for k in (0..i).rev() {
                    self.cache[k] = Some((node_down, 0));
                    node_down = &node_down.get_children()[0];
                }
                self.set_leaf_from_node(node_down, new_offset);
                return self.get_leaf();
            }
        }
        if self.offset_of_leaf + leaf.len() == self.root.len() {
            self.leaf = None;
            self.offset_of_leaf = self.position.min(self.root.len());
            #[cfg(feature = "cursor_state")]
            self.invalidate_state();
            return None;
        }
        self.descend();
        #[cfg(feature = "cursor_state")]
        self.update_state_position();
        self.get_leaf()
    }

    /// Move to beginning of previous leaf.
    ///
    /// Return value: same as [`get_leaf`](#method.get_leaf).
    pub fn prev_leaf(&mut self) -> Option<(&'a L, usize)> {
        if self.offset_of_leaf == 0 {
            self.leaf = None;
            self.position = 0;
            #[cfg(feature = "cursor_state")]
            {
                self.offset_of_leaf = 0;
                self.invalidate_state();
            }
            return None;
        }
        for i in 0..CURSOR_CACHE_SIZE {
            if self.cache[i].is_none() {
                // this probably can't happen
                self.leaf = None;
                self.position = self.offset_of_leaf.saturating_sub(1);
                self.offset_of_leaf = self.position.min(self.root.len());
                #[cfg(feature = "cursor_state")]
                self.invalidate_state();
                return None;
            }
            let (node, j) = self.cache[i].unwrap();
            if j > 0 {
                self.cache[i] = Some((node, j - 1));
                let mut node_down = &node.get_children()[j - 1];
                for k in (0..i).rev() {
                    let last_ix = node_down.get_children().len() - 1;
                    self.cache[k] = Some((node_down, last_ix));
                    node_down = &node_down.get_children()[last_ix];
                }
                let new_offset = self.offset_of_leaf - node_down.len();
                self.position = new_offset;
                self.set_leaf_from_node(node_down, new_offset);
                return self.get_leaf();
            }
        }
        self.position = self.offset_of_leaf - 1;
        self.descend();
        self.position = self.offset_of_leaf;
        #[cfg(feature = "cursor_state")]
        self.update_state_position();
        self.get_leaf()
    }

    /// Go to the leaf containing the current position.
    ///
    /// Sets `leaf` to the leaf containing `position`, and updates `cache` and
    /// `offset_of_leaf` to be consistent.
    fn descend(&mut self) {
        let mut node = self.root;
        let mut offset = 0;
        while node.height() > 0 {
            let children = node.get_children();
            let mut i = 0;
            loop {
                if i + 1 == children.len() {
                    break;
                }
                let nextoff = offset + children[i].len();
                if nextoff > self.position {
                    break;
                }
                offset = nextoff;
                i += 1;
            }
            let cache_ix = node.height() - 1;
            if cache_ix < CURSOR_CACHE_SIZE {
                self.cache[cache_ix] = Some((node, i));
            }
            node = &children[i];
        }
        self.set_leaf_from_node(node, offset);
    }

    /// Returns the measure at the beginning of the leaf containing `pos`.
    ///
    /// This method is O(log n) no matter the current cursor state.
    fn measure_leaf<M: Metric<N, L>>(&self, mut pos: usize) -> usize {
        let mut node = self.root;
        let mut metric = 0;
        while node.height() > 0 {
            for child in node.get_children() {
                let len = child.len();
                if pos < len {
                    node = child;
                    break;
                }
                pos -= len;
                metric += child.measure::<M>();
            }
        }
        metric
    }

    /// Find the leaf having the given measure.
    ///
    /// This function sets `self.position` to the beginning of the leaf
    /// containing the smallest offset with the given metric, and also updates
    /// state as if [`descend`](#method.descend) was called.
    ///
    /// If `measure` is greater than the measure of the whole tree, then moves
    /// to the last node.
    fn descend_metric<M: Metric<N, L>>(&mut self, mut measure: usize) {
        let mut node = self.root;
        let mut offset = 0;
        while node.height() > 0 {
            let children = node.get_children();
            let mut i = 0;
            loop {
                if i + 1 == children.len() {
                    break;
                }
                let child = &children[i];
                let child_m = child.measure::<M>();
                if child_m >= measure {
                    break;
                }
                offset += child.len();
                measure -= child_m;
                i += 1;
            }
            let cache_ix = node.height() - 1;
            if cache_ix < CURSOR_CACHE_SIZE {
                self.cache[cache_ix] = Some((node, i));
            }
            node = &children[i];
        }
        self.position = offset;
        self.set_leaf_from_node(node, offset);
    }
    #[inline]
    fn set_leaf_from_node(&mut self, leaf_node: &'a Node<N, L>, offset: usize) {
        self.leaf = Some(leaf_node.get_leaf());
        self.offset_of_leaf = offset;
        #[cfg(feature = "cursor_state")]
        self.rebuild_state();
    }

    #[cfg(feature = "cursor_state")]
    fn rebuild_state(&mut self) {
        self.state = CursorState::from_cursor(self);
    }

    #[cfg(feature = "cursor_state")]
    fn update_state_position(&mut self) {
        if self.leaf.is_some() {
            self.state.set_position(self.position);
        } else {
            self.state.invalidate(self.position, self.offset_of_leaf);
        }
    }

    #[cfg(feature = "cursor_state")]
    fn invalidate_state(&mut self) {
        self.state.invalidate(self.position, self.offset_of_leaf);
    }
}

/// An iterator generated by a [`Cursor`], for some [`Metric`].
///
/// [`Cursor`]: struct.Cursor.html
/// [`Metric`]: struct.Metric.html
pub struct CursorIter<'c, 'a: 'c, N: NodeInfo<L> + 'a, L: Leaf, M: Metric<N, L> + 'a> {
    cursor: &'c mut Cursor<'a, N, L>,
    _metric: PhantomData<&'a M>,
}

impl<'c, 'a, N, L, M> Iterator for CursorIter<'c, 'a, N, L, M>
where
    N: NodeInfo<L> + 'a,
    L: Leaf,
    M: Metric<N, L> + 'a,
{
    type Item = usize;

    fn next(&mut self) -> Option<usize> {
        self.cursor.next::<M>()
    }
}

impl<'c, 'a, N, L, M> CursorIter<'c, 'a, N, L, M>
where
    N: NodeInfo<L> + 'a,
    L: Leaf,
    M: Metric<N, L> + 'a,
{
    /// Returns the current position of the underlying [`Cursor`].
    ///
    /// [`Cursor`]: struct.Cursor.html
    pub fn pos(&self) -> usize {
        self.cursor.pos()
    }
}

#[cfg(feature = "cursor_state")]
impl<N: NodeInfo<L>, L: Leaf> CursorState<N, L> {
    fn new(
        position: usize,
        offset_of_leaf: usize,
        leaf: Arc<NodeBody<N, L>>,
        frames: SmallVec<[PathFrame<N, L>; CURSOR_CACHE_SIZE]>,
    ) -> Self {
        CursorState { position, offset_of_leaf, leaf: Some(leaf), frames }
    }

    fn new_invalid(position: usize, offset_of_leaf: usize) -> Self {
        CursorState { position, offset_of_leaf, leaf: None, frames: SmallVec::new() }
    }

    pub fn is_valid(&self) -> bool {
        self.leaf.is_some()
    }

    pub fn position(&self) -> usize {
        self.position
    }

    pub fn offset_of_leaf(&self) -> usize {
        self.offset_of_leaf
    }

    pub fn frames(&self) -> &[PathFrame<N, L>] {
        &self.frames
    }

    pub fn to_descriptor(&self) -> CursorDescriptor<N, L> {
        if let Some(leaf) = &self.leaf {
            CursorDescriptor::new(
                self.position,
                self.offset_of_leaf,
                Arc::clone(leaf),
                self.frames.clone(),
            )
        } else {
            CursorDescriptor::new_invalid(self.position)
        }
    }

    pub fn from_descriptor(descriptor: &CursorDescriptor<N, L>) -> Self {
        if let Some(leaf) = &descriptor.leaf {
            CursorState::new(
                descriptor.position,
                descriptor.offset_of_leaf,
                Arc::clone(leaf),
                descriptor.frames.clone(),
            )
        } else {
            CursorState::new_invalid(descriptor.position, descriptor.offset_of_leaf)
        }
    }

    pub fn restore<'a>(&self, root: &'a Node<N, L>) -> Option<Cursor<'a, N, L>> {
        self.to_descriptor().restore(root)
    }

    pub fn from_cursor<'a>(cursor: &Cursor<'a, N, L>) -> Self {
        if cursor.leaf.is_none() {
            return CursorState::new_invalid(cursor.position, cursor.offset_of_leaf);
        }
        let (frames, leaf_arc, offset_of_leaf, _) =
            build_descriptor_components(cursor.root, cursor.position);
        debug_assert_eq!(offset_of_leaf, cursor.offset_of_leaf);
        CursorState::new(cursor.position, offset_of_leaf, leaf_arc, frames)
    }

    fn set_position(&mut self, position: usize) {
        self.position = position;
    }

    fn invalidate(&mut self, position: usize, offset_of_leaf: usize) {
        self.position = position;
        self.offset_of_leaf = offset_of_leaf;
        self.leaf = None;
        self.frames.clear();
    }
}

fn build_descriptor_components<N: NodeInfo<L>, L: Leaf>(
    root: &Node<N, L>,
    position: usize,
) -> (SmallVec<[PathFrame<N, L>; CURSOR_CACHE_SIZE]>, Arc<NodeBody<N, L>>, usize, usize) {
    let mut frames: SmallVec<[PathFrame<N, L>; CURSOR_CACHE_SIZE]> = SmallVec::new();
    let mut node = root;
    let clamped_position = position.min(root.len());
    let mut remaining = clamped_position;

    while node.height() > 0 {
        let children = node.get_children();
        let mut child_index = 0;
        let mut child_offset = 0;
        for (idx, child) in children.iter().enumerate() {
            let len = child.len();
            if remaining < len || idx + 1 == children.len() {
                child_index = idx;
                break;
            }
            remaining -= len;
            child_offset += len;
        }
        frames.push(PathFrame::new(node, child_index, child_offset));
        node = &children[child_index];
    }

    let leaf_arc = clone_node_arc(node);
    let offset_of_leaf = clamped_position - remaining;
    (frames, leaf_arc, offset_of_leaf, remaining)
}

fn clone_node_arc<N: NodeInfo<L>, L: Leaf>(node: &Node<N, L>) -> Arc<NodeBody<N, L>> {
    Arc::clone(node.shared.arc())
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::rope::*;

    fn build_triangle(n: u32) -> String {
        let mut s = String::new();
        let mut line = String::new();
        for _ in 0..n {
            s += &line;
            s += "\n";
            line += "a";
        }
        s
    }

    #[test]
    fn eq_rope_with_pieces() {
        let n = 2_000;
        let s = build_triangle(n);
        let mut builder_default = TreeBuilder::<RopeInfo, String>::new();
        let mut concat_rope = Rope::default();
        builder_default.push_str(&s);
        let mut i = 0;
        while i < s.len() {
            let j = (i + 1000).min(s.len());
            concat_rope = concat_rope + s[i..j].into();
            i = j;
        }
        let built_rope = builder_default.build();
        assert_eq!(built_rope, concat_rope);
    }

    #[test]
    fn cursor_next_triangle() {
        let n = 2_000;
        let text = Rope::from(build_triangle(n));

        let mut cursor = Cursor::new(&text, 0);
        let mut prev_offset = cursor.pos();
        for i in 1..(n + 1) as usize {
            let offset = cursor.next::<LinesMetric>().expect("arrived at the end too soon");
            assert_eq!(offset - prev_offset, i);
            prev_offset = offset;
        }
        assert_eq!(cursor.next::<LinesMetric>(), None);
    }

    #[test]
    fn node_is_empty() {
        let text = Rope::from(String::new());
        assert_eq!(text.is_empty(), true);
    }

    #[test]
    fn cursor_next_empty() {
        let text = Rope::from(String::new());
        let mut cursor = Cursor::new(&text, 0);
        assert_eq!(cursor.next::<LinesMetric>(), None);
        assert_eq!(cursor.pos(), 0);
    }

    #[test]
    fn cursor_iter() {
        let text: Rope = build_triangle(50).into();
        let mut cursor = Cursor::new(&text, 0);
        let mut manual = Vec::new();
        while let Some(nxt) = cursor.next::<LinesMetric>() {
            manual.push(nxt);
        }

        cursor.set(0);
        let auto = cursor.iter::<LinesMetric>().collect::<Vec<_>>();
        assert_eq!(manual, auto);
    }

    #[test]
    fn cursor_next_misc() {
        cursor_next_for("toto");
        cursor_next_for("toto\n");
        cursor_next_for("toto\ntata");
        cursor_next_for("歴史\n科学的");
        cursor_next_for("\n歴史\n科学的\n");
        cursor_next_for(&build_triangle(100));
    }

    fn cursor_next_for(s: &str) {
        let r = Rope::from(s.to_owned());
        for i in 0..r.len() {
            let mut c = Cursor::new(&r, i);
            let it = c.next::<LinesMetric>();
            let pos = c.pos();
            assert!(s.as_bytes()[i..pos - 1].iter().all(|c| *c != b'\n'), "missed linebreak");
            if pos < s.len() {
                assert!(it.is_some(), "must be Some(_)");
                assert!(s.as_bytes()[pos - 1] == b'\n', "not a linebreak");
            } else if s.as_bytes()[s.len() - 1] == b'\n' {
                assert!(it.is_some(), "must be Some(_)");
            } else {
                assert!(it.is_none());
                assert!(c.get_leaf().is_none());
            }
        }
    }

    #[test]
    fn cursor_prev_misc() {
        cursor_prev_for("toto");
        cursor_prev_for("a\na\n");
        cursor_prev_for("toto\n");
        cursor_prev_for("toto\ntata");
        cursor_prev_for("歴史\n科学的");
        cursor_prev_for("\n歴史\n科学的\n");
        cursor_prev_for(&build_triangle(100));
    }

    fn cursor_prev_for(s: &str) {
        let r = Rope::from(s.to_owned());
        for i in 0..r.len() {
            let mut c = Cursor::new(&r, i);
            let it = c.prev::<LinesMetric>();
            let pos = c.pos();

            //Should countain at most one linebreak
            assert!(
                s.as_bytes()[pos..i].iter().filter(|c| **c == b'\n').count() <= 1,
                "missed linebreak"
            );

            if i == 0 && s.as_bytes()[i] == b'\n' {
                assert_eq!(pos, 0);
            }

            if pos > 0 {
                assert!(it.is_some(), "must be Some(_)");
                assert!(s.as_bytes()[pos - 1] == b'\n', "not a linebreak");
            }
        }
    }

    #[test]
    fn at_or_next() {
        let text: Rope = "this\nis\nalil\nstring".into();
        let mut cursor = Cursor::new(&text, 0);
        assert_eq!(cursor.at_or_next::<LinesMetric>(), Some(5));
        assert_eq!(cursor.at_or_next::<LinesMetric>(), Some(5));
        cursor.set(1);
        assert_eq!(cursor.at_or_next::<LinesMetric>(), Some(5));
        assert_eq!(cursor.at_or_prev::<LinesMetric>(), Some(5));
        cursor.set(6);
        assert_eq!(cursor.at_or_prev::<LinesMetric>(), Some(5));
        cursor.set(6);
        assert_eq!(cursor.at_or_next::<LinesMetric>(), Some(8));
        assert_eq!(cursor.at_or_next::<LinesMetric>(), Some(8));
    }

    #[test]
    fn next_zero_measure_large() {
        let mut text = Rope::from("a");
        for _ in 0..24 {
            text = Node::concat(text.clone(), text);
            let mut cursor = Cursor::new(&text, 0);
            assert_eq!(cursor.next::<LinesMetric>(), None);
            // Test that cursor is properly invalidated and at end of text.
            assert_eq!(cursor.get_leaf(), None);
            assert_eq!(cursor.pos(), text.len());

            cursor.set(text.len());
            assert_eq!(cursor.prev::<LinesMetric>(), None);
            // Test that cursor is properly invalidated and at beginning of text.
            assert_eq!(cursor.get_leaf(), None);
            assert_eq!(cursor.pos(), 0);
        }
    }

    #[test]
    fn prev_line_large() {
        let s: String = format!("{}{}", "\n", build_triangle(1000));
        let rope = Rope::from(s);
        let mut expected_pos = rope.len();
        let mut cursor = Cursor::new(&rope, rope.len());

        for i in (1..1001).rev() {
            expected_pos -= i;
            assert_eq!(expected_pos, cursor.prev::<LinesMetric>().unwrap());
        }

        assert_eq!(None, cursor.prev::<LinesMetric>());
    }

    #[test]
    fn prev_line_small() {
        let empty_rope = Rope::from("\n");
        let mut cursor = Cursor::new(&empty_rope, empty_rope.len());
        assert_eq!(None, cursor.prev::<LinesMetric>());

        let rope = Rope::from("\n\n\n\n\n\n\n\n\n\n");
        cursor = Cursor::new(&rope, rope.len());
        let mut expected_pos = rope.len();
        for _ in (1..10).rev() {
            expected_pos -= 1;
            assert_eq!(expected_pos, cursor.prev::<LinesMetric>().unwrap());
        }

        assert_eq!(None, cursor.prev::<LinesMetric>());
    }

    #[test]
    fn balance_invariant() {
        let mut tb = TreeBuilder::<RopeInfo, String>::new();
        let leaves: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
        tb.push_leaves(leaves);
        let tree = tb.build();
        println!("height {}", tree.height());
    }
}
