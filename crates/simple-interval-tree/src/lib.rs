#![feature(allocator_api, associated_type_defaults, btreemap_alloc)]

pub mod multi_value_map;
pub(crate) mod range;
pub use multi_value_map::*;
pub use range::*;

use itertools::Itertools;
use smallvec::SmallVec;
use std::{
    alloc::{Allocator, Global},
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Debug,
    marker::PhantomData,
    mem,
    ops::{Bound::*, Deref, Range, RangeBounds},
    ptr::NonNull,
};

type NodePtr<R, V> = NonNull<(Range<R>, V)>;

struct EndpointRaw<'a, R, V> {
    pub is_end: bool,
    pub at: &'a R,
    pub node: NodePtr<R, V>,
}

pub struct Endpoint<'a, R, V> {
    pub is_end: bool,
    pub at: &'a R,
    pub pair: &'a (Range<R>, V),
}

impl<'a, R, V> From<EndpointRaw<'a, R, V>> for Endpoint<'a, R, V> {
    fn from(value: EndpointRaw<'a, R, V>) -> Self {
        let EndpointRaw { is_end, at, node } = value;
        let pair = unsafe { &*(node.as_ptr()) };
        Endpoint { is_end, at, pair }
    }
}

pub struct IntervalTree<R, V, A: Allocator + Clone = Global> {
    starts: BTreeMap<R, SmallVec<[NodePtr<R, V>; 1]>, A>,
    ends: BTreeMap<R, SmallVec<[NodePtr<R, V>; 1]>, A>,
    len: usize,
    alloc: A,
}

unsafe impl<R: Send, V: Send, A: Allocator + Clone + Send> Send for IntervalTree<R, V, A> {}
unsafe impl<R: Sync, V: Sync, A: Allocator + Clone + Sync> Sync for IntervalTree<R, V, A> {}

impl<R: Clone + Ord, V: Clone, A: Allocator + Clone> Clone for IntervalTree<R, V, A> {
    fn clone(&self) -> Self {
        let mut ptrs = HashMap::with_capacity(self.len());

        let mut starts = BTreeMap::new_in(self.alloc.clone());
        starts.extend(self.starts.iter().map(|(range, nodes)| {
            let nodes = nodes
                .iter()
                .map(|&ptr| {
                    let pair = unsafe { &*(ptr.as_ptr()) }.clone();
                    let new_ptr =
                        Box::into_non_null_with_allocator(Box::new_in(pair, &self.alloc)).0;
                    ptrs.insert(ptr, new_ptr);
                    new_ptr
                })
                .collect();
            (range.clone(), nodes)
        }));

        let mut ends = BTreeMap::new_in(self.alloc.clone());
        ends.extend(self.ends.iter().map(|(range, nodes)| {
            let nodes = nodes
                .iter()
                .map(|ptr| ptrs.get(ptr).copied().unwrap())
                .collect();
            (range.clone(), nodes)
        }));

        Self {
            starts,
            ends,
            len: self.len,
            alloc: self.alloc.clone(),
        }
    }
}

impl<R, V, A: Allocator + Clone> Drop for IntervalTree<R, V, A> {
    fn drop(&mut self) {
        self.clear();
    }
}

/// Type representing an entry in the interval tree.
#[allow(unused)]
pub struct Entry<'a, R, V, A: Allocator + Clone = Global> {
    /// Which tree the entry is from. Used for verification before deletion.
    /// Deleting an entry from the wrong tree will cause the program to panic.
    tree: *const IntervalTree<R, V, A>,
    /// The underlying node in the tree, owned by the tree.
    node: NodePtr<R, V>,
    _marker: PhantomData<&'a (Range<R>, V)>,
}

impl<R, V, A: Allocator + Clone> Deref for Entry<'_, R, V, A> {
    type Target = (Range<R>, V);

    fn deref(&self) -> &Self::Target {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { &*(self.node.as_ptr()) }
    }
}

impl<R, V, A> IntervalTree<R, V, A>
where
    A: Allocator + Clone,
{
    pub fn new_in(alloc: A) -> Self {
        IntervalTree {
            starts: BTreeMap::new_in(alloc.clone()),
            ends: BTreeMap::new_in(alloc.clone()),
            len: 0,
            alloc,
        }
    }
}

impl<R, V> IntervalTree<R, V> {
    pub fn new() -> Self {
        Self::new_in(Global)
    }
}

impl<R, V> Default for IntervalTree<R, V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<R, V> FromIterator<(Range<R>, V)> for IntervalTree<R, V>
where
    R: Ord + Clone,
{
    fn from_iter<T: IntoIterator<Item = (Range<R>, V)>>(iter: T) -> Self {
        let mut tree = Self::new();
        tree.extend(iter);
        tree
    }
}

impl<R, V, A> Extend<(Range<R>, V)> for IntervalTree<R, V, A>
where
    R: Ord + Clone,
    A: Allocator + Clone,
{
    fn extend<T: IntoIterator<Item = (Range<R>, V)>>(&mut self, iter: T) {
        for (range, value) in iter {
            self.insert(range, value);
        }
    }
}

#[inline(always)]
unsafe fn nodes_to_tuples<'a, R: 'a, V: 'a>(
    nodes: impl Iterator<Item = NodePtr<R, V>>,
) -> impl Iterator<Item = &'a (Range<R>, V)> {
    nodes.map(|v| unsafe { &*(v.as_ptr()) })
}

#[inline(always)]
unsafe fn nodes_to_ref_tuples<'a, R: 'a, V: 'a>(
    nodes: impl Iterator<Item = NodePtr<R, V>>,
) -> impl Iterator<Item = (&'a Range<R>, &'a V)> {
    unsafe { nodes_to_tuples(nodes).map(|(a, b)| (a, b)) }
}

#[inline(always)]
unsafe fn nodes_to_tuples_double_ended<'a, R: 'a, V: 'a>(
    nodes: impl DoubleEndedIterator<Item = NodePtr<R, V>>,
) -> impl DoubleEndedIterator<Item = &'a (Range<R>, V)> {
    nodes.map(|v| unsafe { &*(v.as_ptr()) })
}

#[inline(always)]
unsafe fn nodes_to_ref_tuples_double_ended<'a, R: 'a, V: 'a>(
    nodes: impl DoubleEndedIterator<Item = NodePtr<R, V>>,
) -> impl DoubleEndedIterator<Item = (&'a Range<R>, &'a V)> {
    unsafe { nodes_to_tuples_double_ended(nodes).map(|(a, b)| (a, b)) }
}

impl<R, V, A> IntervalTree<R, V, A>
where
    A: Allocator + Clone,
{
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn clear(&mut self) {
        let starts = mem::replace(&mut self.starts, BTreeMap::new_in(self.alloc.clone()));
        starts.values().flatten().for_each(|&v| unsafe {
            let _ = Box::from_non_null_in(v, &self.alloc);
        });
        self.ends.clear();
        self.len = 0;
    }
}

impl<R, V, A> IntervalTree<R, V, A>
where
    R: Ord,
    A: Allocator + Clone,
{
    pub fn insert(&mut self, range: Range<R>, value: V)
    where
        R: Clone,
    {
        let Range { start, end } = range.clone();
        let node = Box::into_non_null_with_allocator(Box::new_in((range, value), &self.alloc)).0;
        self.starts.entry(start).or_default().push(node);
        self.ends.entry(end).or_default().push(node);
        self.len += 1;
    }

    pub fn retain(&mut self, mut f: impl FnMut(&Range<R>, &mut V) -> bool) {
        let mut removed = HashSet::new();
        self.starts.retain(|_, v| {
            v.retain(|v| {
                // SAFETY: all pointers should point to valid nodes.
                let (range, value) = unsafe { v.as_mut() };
                if f(range, value) {
                    true
                } else {
                    removed.insert(*v);
                    self.len -= 1;
                    // SAFETY: all pointers are extracted from boxes when created.
                    let _ = unsafe { Box::from_non_null_in(*v, &self.alloc) };
                    false
                }
            });
            !v.is_empty()
        });
        self.ends.retain(|_, v| {
            v.retain(|v| !removed.contains(v));
            !v.is_empty()
        });
    }

    /// Removes a node from the tree.
    /// # Safety
    /// The node must be a valid pointer to a node in the tree.
    #[allow(unused)]
    unsafe fn remove_node(&mut self, node: NodePtr<R, V>) -> (Range<R>, V) {
        // SAFETY: we are removing a valid node from the tree.
        let entry: Box<(Range<R>, V), _> = unsafe { Box::from_non_null_in(node, &self.alloc) };
        let (range, _) = entry.as_ref();
        let IntervalTree { starts, ends, .. } = self;

        let (remove_start, remove_end) = if let Some(start_nodes) = starts.get_mut(&range.start)
            && let Some(end_nodes) = ends.get_mut(&range.end)
        {
            start_nodes.retain(|v| *v != node);
            end_nodes.retain(|v| *v != node);
            self.len -= 1;
            (start_nodes.is_empty(), end_nodes.is_empty())
        } else {
            unreachable!()
        };

        if remove_start {
            starts.remove(&range.start);
        }
        if remove_end {
            ends.remove(&range.end);
        }

        *entry
    }

    #[allow(unused)]
    unsafe fn node_to_entry<'a>(&'a self, node: NodePtr<R, V>) -> Entry<'a, R, V, A> {
        Entry {
            tree: self as *const Self,
            node,
            _marker: PhantomData,
        }
    }

    fn nodes_by_start(&self) -> impl DoubleEndedIterator<Item = NodePtr<R, V>> {
        self.starts.values().flat_map(|v| v.iter().copied())
    }

    fn nodes_by_end(&self) -> impl DoubleEndedIterator<Item = NodePtr<R, V>> {
        self.ends.values().flat_map(|v| v.iter().copied())
    }

    fn nodes_overlaps<G>(&self, range: &G) -> impl Iterator<Item = NodePtr<R, V>>
    where
        G: RangeBounds<R>,
    {
        // start < range.end && end > range.start
        self.starts
            .range((Unbounded, range.end_bound()))
            .flat_map(|(_, nodes)| nodes.iter().copied())
            .filter(|&node| {
                // SAFETY: all pointers should point to valid nodes.
                let Range::<R> { end, .. } = unsafe { &(*(node.as_ptr())).0 };
                (range.start_bound(), Unbounded)
                    .invert_inclusiveness()
                    .contains(end)
            })
    }

    fn nodes_during<G>(&self, range: &G) -> impl Iterator<Item = NodePtr<R, V>>
    where
        G: RangeBounds<R>,
    {
        // start >= range.start && end <= range.end
        self.starts
            .range(range.bounds())
            .flat_map(|(_, nodes)| nodes.iter().copied())
            .filter(|&node| {
                // SAFETY: all pointers should point to valid nodes.
                let Range::<R> { end, .. } = unsafe { &(*(node.as_ptr())).0 };
                range.bounds().invert_inclusiveness().contains(end)
            })
    }

    fn nodes_starts_during<G>(&self, range: &G) -> impl DoubleEndedIterator<Item = NodePtr<R, V>>
    where
        G: RangeBounds<R>,
    {
        self.starts
            .range((range.start_bound(), range.end_bound()))
            .flat_map(|(_, v)| v.iter().copied())
    }

    fn nodes_ends_during<G>(&self, range: &G) -> impl DoubleEndedIterator<Item = NodePtr<R, V>>
    where
        G: RangeBounds<R>,
    {
        self.ends
            .range((range.start_bound(), range.end_bound()).invert_inclusiveness())
            .flat_map(|(_, v)| v.iter().copied())
    }

    fn endpoints_raw_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = EndpointRaw<'a, R, V>>
    where
        G: RangeBounds<R>,
    {
        let starts_range = self.starts.range(range.bounds()).flat_map(|(r, v)| {
            v.iter().map(move |&node| EndpointRaw {
                is_end: false,
                at: r,
                node,
            })
        });
        let ends_range = self.ends.range(range.bounds()).flat_map(|(r, v)| {
            v.iter().map(move |&node| EndpointRaw {
                is_end: true,
                at: r,
                node,
            })
        });
        starts_range.merge_by(ends_range, |ep1, ep2| ep1.at < ep2.at)
    }

    // // FIXME: deletion via a node obtained from an iterator is not supported now.
    // // the design needs to be considered for safety reasons
    // pub fn remove(&mut self, entry: &Entry<R, V, A>) -> (Range<R>, V) {
    //     if !(self as *const Self).eq(&entry.tree) {
    //         panic!("Entry is not from this tree!");
    //     }
    //     // SAFETY: we are removing a valid node from the tree.
    //     unsafe { self.remove_node(entry.node) }
    // }

    pub fn iter_endpoints_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = Endpoint<'a, R, V>>
    where
        G: RangeBounds<R>,
    {
        self.endpoints_raw_during(range).map(Into::into)
    }

    pub fn iter_endpoints<'a>(&'a self) -> impl Iterator<Item = Endpoint<'a, R, V>> {
        self.iter_endpoints_during(&..)
    }

    // pub fn endpoints_with_entries_during<'a, G>(
    //     &'a self,
    //     range: &G,
    // ) -> impl Iterator<Item = (bool, &'a R, Entry<'a, R, V, A>)>
    // where
    //     G: RangeBounds<R>,
    // {
    //     self.endpoints_with_nodes_during(range)
    //         .map(|(is_end, r, node)| {
    //             // SAFETY: entry is from this tree.
    //             let entry = unsafe { self.node_to_entry(node) };
    //             (is_end, r, entry)
    //         })
    // }

    // pub fn endpoints_with_entries<'a>(
    //     &'a self,
    // ) -> impl Iterator<Item = (bool, &'a R, Entry<'a, R, V, A>)> {
    //     self.endpoints_with_entries_during(&..)
    // }

    // pub fn entries_by_start<'a>(&'a self) -> impl Iterator<Item = Entry<'a, R, V, A>> {
    //     self.nodes_by_start().map(|node| {
    //         // SAFETY: entry is from this tree.
    //         unsafe { self.node_to_entry(node) }
    //     })
    // }

    // pub fn entries_by_end<'a>(&'a self) -> impl Iterator<Item = Entry<'a, R, V, A>> {
    //     self.nodes_by_end().map(|node| {
    //         // SAFETY: entry is from this tree.
    //         unsafe { self.node_to_entry(node) }
    //     })
    // }

    /// Returns an iterator over all entries in the tree, ordered by interval starts.
    pub fn iter_by_start(&self) -> impl DoubleEndedIterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_ref_tuples_double_ended(self.nodes_by_start()) }
    }

    pub fn into_iter_by_start(mut self) -> impl DoubleEndedIterator<Item = (Range<R>, V)>
    where
        A: 'static + Clone,
    {
        let alloc = self.alloc.clone();
        let starts = mem::replace(&mut self.starts, BTreeMap::new_in(self.alloc.clone()));
        mem::forget(self);

        starts
            .into_values()
            .flat_map(|v| v.into_iter())
            .map(move |node| {
                // SAFETY: all pointers should point to valid nodes.
                let node: Box<(Range<R>, V), _> = unsafe { Box::from_non_null_in(node, &alloc) };
                *node
            })
    }

    /// Returns an iterator over all entries in the tree, ordered by interval ends.
    pub fn iter_by_end(&self) -> impl DoubleEndedIterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_ref_tuples_double_ended(self.nodes_by_end()) }
    }

    pub fn into_iter_by_end(mut self) -> impl DoubleEndedIterator<Item = (Range<R>, V)>
    where
        A: 'static + Clone,
    {
        let alloc = self.alloc.clone();
        let ends = mem::replace(&mut self.ends, BTreeMap::new_in(self.alloc.clone()));
        mem::forget(self);

        ends.into_values()
            .flat_map(|v| v.into_iter())
            .map(move |node| {
                // SAFETY: all pointers should point to valid nodes.
                let node: Box<(Range<R>, V), _> = unsafe { Box::from_non_null_in(node, &alloc) };
                *node
            })
    }

    pub fn iter_overlaps<G>(&self, range: &G) -> impl Iterator<Item = (&Range<R>, &V)>
    where
        G: RangeBounds<R>,
    {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_ref_tuples(self.nodes_overlaps(range)) }
    }

    pub fn iter_during<G>(&self, range: &G) -> impl Iterator<Item = (&Range<R>, &V)>
    where
        G: RangeBounds<R>,
    {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_ref_tuples(self.nodes_during(range)) }
    }

    /// Returns an iterator over entries whose starting point is within the given range.
    pub fn iter_starts_during<G: RangeBounds<R>>(
        &self,
        range: &G,
    ) -> impl DoubleEndedIterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_ref_tuples_double_ended(self.nodes_starts_during(range)) }
    }

    /// Returns an iterator over entries whose ending point is within the given range.
    pub fn iter_ends_during<G: RangeBounds<R>>(
        &self,
        range: &G,
    ) -> impl DoubleEndedIterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_ref_tuples_double_ended(self.nodes_ends_during(range)) }
    }
}

impl<R, V, A> Debug for IntervalTree<R, V, A>
where
    R: Debug + Ord,
    V: Debug,
    A: Allocator + Clone,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter_by_start()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::debug;
    use tracing_test::traced_test;

    const EXAMPLE_TREE_DATA: &[(Range<u32>, &str)] = [
        (0..3, "a"),
        (1..4, "b"),
        (1..2, "c"), // overlapping interval start
        (6..7, "d"),
        (3..10, "e"),
        (5..6, "f"),
        (8..10, "g"), // overlapping interval end
        (0..12, "h"),
        (1..2, "i"), // overlapping interval start and end
    ]
    .as_slice();

    fn build_example_tree() -> IntervalTree<u32, &'static str> {
        IntervalTree::from_iter(EXAMPLE_TREE_DATA.iter().cloned())
    }

    #[traced_test]
    #[test]
    fn test_build_tree() {
        let tree = build_example_tree();
        debug!("{:?}", tree);
        assert_eq!(tree.len(), EXAMPLE_TREE_DATA.len());
    }

    #[traced_test]
    #[test]
    fn test_iter_by_start() {
        let tree = build_example_tree();

        let mut count = 0usize;
        let mut iter = tree.iter_by_start();

        if let Some((range, value)) = iter.next() {
            debug!("{:?}: {:?}", range, value);
            count += 1;
            let mut prev_start = range.start;
            for (range, value) in iter {
                debug!("{:?}: {:?}", range, value);
                assert!(range.start >= prev_start);
                count += 1;
                prev_start = range.start;
            }
        }
        debug!("{:?} elements in the tree", count);
        assert_eq!(count, tree.len());
    }

    #[traced_test]
    #[test]
    fn test_into_iter_by_start() {
        let tree = build_example_tree();
        let len = tree.len();
        let mut count = 0usize;
        let mut iter = tree.into_iter_by_start();

        if let Some((range, value)) = iter.next() {
            debug!("{:?}: {:?}", range, value);
            count += 1;
            let mut prev_start = range.start;
            for (range, value) in iter {
                debug!("{:?}: {:?}", range, value);
                assert!(range.start >= prev_start);
                count += 1;
                prev_start = range.start;
            }
        }
        debug!("{:?} elements in the tree", count);
        assert_eq!(count, len);
    }

    #[traced_test]
    #[test]
    fn test_iter_by_end() {
        let tree = build_example_tree();

        let mut count = 0usize;
        let mut iter = tree.iter_by_end();
        if let Some((range, value)) = iter.next() {
            debug!("{:?}: {:?}", range, value);
            count += 1;
            let mut prev_end = range.end;
            for (range, value) in iter {
                debug!("{:?}: {:?}", range, value);
                assert!(range.end >= prev_end);
                count += 1;
                prev_end = range.end;
            }
        }
        debug!("{:?} elements in the tree", count);
        assert_eq!(count, tree.len());
    }

    #[traced_test]
    #[test]
    fn test_into_iter_by_end() {
        let tree = build_example_tree();
        let len = tree.len();
        let mut count = 0usize;
        let mut iter = tree.into_iter_by_end();

        if let Some((range, value)) = iter.next() {
            debug!("{:?}: {:?}", range, value);
            count += 1;
            let mut prev_end = range.end;
            for (range, value) in iter {
                debug!("{:?}: {:?}", range, value);
                assert!(range.end >= prev_end);
                count += 1;
                prev_end = range.end;
            }
        }
        debug!("{:?} elements in the tree", count);
        assert_eq!(count, len);
    }

    #[traced_test]
    #[test]
    fn test_iter_during() {
        let tree = build_example_tree();
        let query_range = 1..6;

        debug!("using `iter_during`");
        let mut count1 = 0usize;
        for (range, value) in tree.iter_during(&query_range) {
            debug!("{:?}: {:?}", range, value);
            assert!(range.start >= query_range.start);
            assert!(range.end <= query_range.end);
            count1 += 1;
        }

        debug!("Traversing");
        let mut count2 = 0usize;
        for (range, value) in tree.iter_by_start() {
            if range.start >= query_range.start && range.end <= query_range.end {
                debug!("{:?}: {:?}", range, value);
                count2 += 1;
            }
        }

        assert_eq!(count1, count2);
    }

    #[traced_test]
    #[test]
    fn test_iter_overlaps() {
        let tree = build_example_tree();
        let query_range = 1..6;

        debug!("using `iter_overlaps`");
        let mut count1 = 0usize;
        for (range, value) in tree.iter_overlaps(&query_range) {
            debug!("{:?}: {:?}", range, value);
            assert!(range.start < query_range.end);
            assert!(range.end > query_range.start);
            count1 += 1;
        }

        debug!("Traversing");
        let mut count2 = 0usize;
        for (range, value) in tree.iter_by_start() {
            if range.start < query_range.end && range.end > query_range.start {
                debug!("{:?}: {:?}", range, value);
                count2 += 1;
            }
        }

        assert_eq!(count1, count2);
    }

    #[traced_test]
    #[test]
    fn test_iter_starts_during() {
        let tree = build_example_tree();
        let query_range = 4..7;
        for (range, value) in tree.iter_starts_during(&query_range) {
            debug!("{:?}: {:?}", range, value);
            assert!(query_range.contains(&range.start));
        }
    }

    #[traced_test]
    #[test]
    fn test_iter_ends_during() {
        let tree = build_example_tree();
        let query_range = 1..=4;
        for (range, value) in tree.iter_ends_during(&query_range) {
            debug!("{:?}: {:?}", range, value);
            assert!(query_range.contains(&range.end));
        }
    }

    #[traced_test]
    #[test]
    fn test_retain() {
        let mut tree = build_example_tree();
        tree.retain(|range, value| {
            if let Some(ch) = value.chars().next() {
                ch <= 'd' && range.start >= 2
            } else {
                false
            }
        });
        for (range, value) in tree.iter_by_start() {
            debug!("{:?}: {:?}", range, value);
            assert!(range.start >= 2);
            if let Some(ch) = value.chars().next() {
                assert!(ch <= 'd');
            } else {
                unreachable!()
            }
        }
    }

    #[traced_test]
    #[test]
    fn test_clear() {
        let mut tree = build_example_tree();
        tree.clear();
        debug!("{:?}", tree);
        assert_eq!(tree.len(), 0);
    }

    #[traced_test]
    #[test]
    fn test_clone() {
        let tree = build_example_tree();
        let mut cloned_tree = tree.clone();
        debug!("Before clear");
        debug!("{:?}", tree);
        debug!("{:?}", cloned_tree);
        assert_eq!(tree.len(), cloned_tree.len());
        let tree_len = tree.len();

        debug!("After clear");
        cloned_tree.clear();
        debug!("{:?}", tree);
        debug!("{:?}", cloned_tree);
        assert_eq!(tree.len(), tree_len);
        assert_eq!(cloned_tree.len(), 0);
    }
}
