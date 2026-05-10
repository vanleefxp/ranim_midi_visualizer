#![feature(allocator_api)]

use smallvec::SmallVec;
use std::{
    alloc::{Allocator, Global},
    collections::BTreeMap,
    fmt::Debug,
    ops::{Deref, Range},
    ptr::NonNull,
};

#[derive(Clone)]
pub struct IntervalTree<R, V, A: Allocator = Global> {
    starts: BTreeMap<R, SmallVec<[NonNull<(Range<R>, V)>; 1]>>,
    ends: BTreeMap<R, SmallVec<[NonNull<(Range<R>, V)>; 1]>>,
    alloc: A,
}

pub struct Entry<R, V, A: Allocator = Global> {
    tree: *const IntervalTree<R, V, A>,
    node: NonNull<(Range<R>, V)>,
}

impl<R, V, A: Allocator> Deref for Entry<R, V, A> {
    type Target = (Range<R>, V);

    fn deref(&self) -> &Self::Target {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { &*(self.node.as_ptr()) }
    }
}

impl<R, V, A> IntervalTree<R, V, A>
where
    A: Allocator,
{
    pub fn new_in(alloc: A) -> Self {
        IntervalTree {
            starts: BTreeMap::new(),
            ends: BTreeMap::new(),
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
    A: Allocator,
{
    fn extend<T: IntoIterator<Item = (Range<R>, V)>>(&mut self, iter: T) {
        for (range, value) in iter {
            self.insert(range, value);
        }
    }
}

#[inline(always)]
unsafe fn nodes_to_tuples<'a, R: 'a, V: 'a>(
    nodes: impl Iterator<Item = NonNull<(Range<R>, V)>>,
) -> impl Iterator<Item = (&'a Range<R>, &'a V)> {
    nodes.map(|v| unsafe {
        let (range, value) = &*(v.as_ptr());
        (range, value)
    })
}

impl<R, V, A> IntervalTree<R, V, A> where A: Allocator {
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }
}

impl<R, V, A> IntervalTree<R, V, A>
where
    R: Ord,
    A: Allocator,
{
    pub fn insert(&mut self, range: Range<R>, value: V)
    where
        R: Clone,
    {
        let Range { start, end } = range.clone();
        let node = Box::into_non_null_with_allocator(Box::new_in((range, value), &self.alloc)).0;
        self.starts
            .entry(start)
            .or_insert_with(SmallVec::new)
            .push(node);
        self.ends
            .entry(end)
            .or_insert_with(SmallVec::new)
            .push(node);
    }

    pub fn retain(&mut self, mut f: impl FnMut(&Range<R>, &mut V) -> bool) {
        for tree in [&mut self.starts, &mut self.ends] {
            tree.retain(|_, v| {
                v.retain(|v| {
                    // SAFETY: all pointers should point to valid nodes.
                    let (range, value) = unsafe { v.as_mut() };
                    f(range, value)
                });
                !v.is_empty()
            });
        }
    }

    /// Removes a node from the tree.
    /// #Safety
    /// The node must be a valid pointer to a node in the tree.
    #[allow(unused)]
    unsafe fn remove_node(&mut self, node: NonNull<(Range<R>, V)>) {
        // SAFETY: we are removing a valid node from the tree.
        let entry: Box<(Range<R>, V), _> = unsafe { Box::from_non_null_in(node, &self.alloc) };
        let (range, _) = entry.as_ref();
        let IntervalTree { starts, ends, .. } = self;
        let (remove_start, remove_end) = if let Some(start_nodes) = starts.get_mut(&range.start)
            && let Some(end_nodes) = ends.get_mut(&range.end)
        {
            start_nodes.retain(|v| *v != node);
            end_nodes.retain(|v| *v != node);
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
    }

    pub fn remove(&mut self, entry: &Entry<R, V, A>) {
        if self as *const Self != entry.tree {
            panic!("Entry is not from this tree!")
        }
        // SAFETY: we are removing a valid node from the tree.
        unsafe { self.remove_node(entry.node); }
    }

    fn nodes_by_start(&self) -> impl Iterator<Item = NonNull<(Range<R>, V)>> {
        self.starts.values().flat_map(|v| v.iter().copied())
    }

    fn nodes_by_end(&self) -> impl Iterator<Item = NonNull<(Range<R>, V)>> {
        self.ends.values().flat_map(|v| v.iter().copied())
    }

    fn nodes_overlaps(&self, range: &Range<R>) -> impl Iterator<Item = NonNull<(Range<R>, V)>>
    where
        R: Clone,
    {
        self.starts
            .range(range.clone())
            .flat_map(|(_, nodes)| nodes.iter().copied())
            .chain(
                self.ends
                    .range(range.clone())
                    .flat_map(|(_, nodes)| nodes.iter().copied())
                    .filter(|&node| {
                        // SAFETY: all pointers should point to valid nodes.
                        #[rustfmt::skip]
                        let Range::<R> { start, .. } = unsafe { &(*(node.as_ptr())).0 };
                        // only take nodes whose starting point is before the start of the range
                        // to avoid repetition
                        start < &range.start
                    }),
            )
    }

    fn nodes_during(&self, range: &Range<R>) -> impl Iterator<Item = NonNull<(Range<R>, V)>>
    where
        R: Clone,
    {
        self.starts
            .range(range.clone())
            .flat_map(|(_, nodes)| nodes.iter().copied())
            .filter(|&node| {
                // SAFETY: all pointers should point to valid nodes.
                #[rustfmt::skip]
                let Range::<R> { end, .. } = unsafe { &(*(node.as_ptr())).0 };
                end <= &range.end
            })
    }

    pub fn entries_by_start(&self) -> impl Iterator<Item = Entry<R, V, A>> {
        self.nodes_by_start().map(|node| Entry { tree: self as *const Self, node })
    }

    /// Returns an iterator over all entries in the tree, ordered by interval starts.
    pub fn iter_by_start(&self) -> impl Iterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_tuples(self.nodes_by_start()) }
    }

    /// Returns an iterator over all entries in the tree, ordered by interval ends.
    pub fn iter_by_end(&self) -> impl Iterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_tuples(self.nodes_by_end()) }
    }

    pub fn iter_overlaps(&self, range: &Range<R>) -> impl Iterator<Item = (&Range<R>, &V)>
    where
        R: Clone,
    {
        unsafe { nodes_to_tuples(self.nodes_overlaps(range)) }
    }

    pub fn iter_during(&self, range: &Range<R>) -> impl Iterator<Item = (&Range<R>, &V)>
    where
        R: Clone,
    {
        unsafe { nodes_to_tuples(self.nodes_during(range)) }
    }
}

impl<R, V, A> Debug for IntervalTree<R, V, A>
where
    R: Debug + Ord,
    V: Debug,
    A: Allocator,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut ds = f.debug_struct("IntervalTree");
        for (range, value) in self.iter_by_start() {
            ds.field(format!("{:?}", range).as_str(), value);
        }
        ds.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_interval_tree() {
        let mut tree = IntervalTree::from_iter([
            (0..3, "a"),
            (1..2, "b"),
            (6..7, "c"),
            (3..10, "d"),
            (5..6, "e"),
        ]);
        println!("{:?}", tree);

        let subtree = tree
            .iter_during(&(1..6))
            .map(|(r, &v)| (r.clone(), v))
            .collect::<IntervalTree<_, _>>();
        println!("{:?}", subtree);

        let subtree = tree
            .iter_overlaps(&(1..6))
            .map(|(r, &v)| (r.clone(), v))
            .collect::<IntervalTree<_, _>>();
        println!("{:?}", subtree);

        let to_remove = tree.entries_by_start().nth(2).unwrap();
        tree.remove(&to_remove);
        println!("{:?}", tree);
    }
}
