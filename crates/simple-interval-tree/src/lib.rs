#![feature(allocator_api)]

use itertools::Itertools;
use smallvec::SmallVec;
use std::{
    alloc::{Allocator, Global}, cell::UnsafeCell, collections::{BTreeMap, HashMap, HashSet}, fmt::Debug, mem, ops::{Deref, Range, RangeBounds}, ptr::NonNull
};

pub struct IntervalTree<R, V, A: Allocator = Global> {
    starts: BTreeMap<R, SmallVec<[NonNull<(Range<R>, V)>; 1]>>,
    ends: BTreeMap<R, SmallVec<[NonNull<(Range<R>, V)>; 1]>>,
    len: UnsafeCell<usize>,
    alloc: A,
}

unsafe impl<R: Send, V: Send, A: Allocator + Send> Send for IntervalTree<R, V, A> {}
unsafe impl<R: Sync, V: Sync, A: Allocator + Sync> Sync for IntervalTree<R, V, A> {}

impl<R: Clone + Ord, V: Clone, A: Allocator + Clone> Clone for IntervalTree<R, V, A> {
    fn clone(&self) -> Self {
        let mut ptrs = HashMap::with_capacity(self.len());

        let alloc = self.alloc.clone();
        let starts = self
            .starts
            .iter()
            .map(|(range, nodes)| {
                let nodes = nodes
                    .iter()
                    .map(|&ptr| {
                        let pair = unsafe { &*(ptr.as_ptr()) }.clone();
                        let new_ptr =
                            Box::into_non_null_with_allocator(Box::new_in(pair, &alloc)).0;
                        ptrs.insert(ptr, new_ptr);
                        new_ptr
                    })
                    .collect();
                (range.clone(), nodes)
            })
            .collect();
        let ends = self
            .ends
            .iter()
            .map(|(range, nodes)| {
                let nodes = nodes
                    .iter()
                    .map(|ptr| ptrs.get(ptr).copied().unwrap())
                    .collect();
                (range.clone(), nodes)
            })
            .collect();

        Self {
            starts,
            ends,
            len: UnsafeCell::new(unsafe { *(self.len.get()) }),
            alloc,
        }
    }
}

impl<R, V, A: Allocator> Drop for IntervalTree<R, V, A> {
    fn drop(&mut self) {
        self.clear();
    }
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
            len: UnsafeCell::default(),
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

impl<R, V, A> IntervalTree<R, V, A>
where
    A: Allocator,
{
    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    pub fn len(&self) -> usize {
        // SAFETY: readonly access.
        unsafe { *(self.len.get()) }
    }

    pub fn clear(&mut self) {
        let starts = mem::take(&mut self.starts);
        starts.values().flatten().for_each(|&v| unsafe {
            let _ = Box::from_non_null_in(v, &self.alloc);
        });
        self.ends.clear();
        *self.len.get_mut() = 0;
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
        *self.len.get_mut() += 1;
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
                    *self.len.get_mut() -= 1;
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
    unsafe fn remove_node(&mut self, node: NonNull<(Range<R>, V)>) -> (Range<R>, V) {
        // SAFETY: we are removing a valid node from the tree.
        let entry: Box<(Range<R>, V), _> = unsafe { Box::from_non_null_in(node, &self.alloc) };
        let (range, _) = entry.as_ref();
        let IntervalTree { starts, ends, .. } = self;
        
        let (remove_start, remove_end) = if let Some(start_nodes) = starts.get_mut(&range.start)
            && let Some(end_nodes) = ends.get_mut(&range.end)
        {
            start_nodes.retain(|v| *v != node);
            end_nodes.retain(|v| *v != node);
            *self.len.get_mut() -= 1;
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
                let Range::<R> { end, .. } = unsafe { &(*(node.as_ptr())).0 };
                end <= &range.end
            })
    }

    fn endpoints_with_nodes_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (bool, &R, NonNull<(Range<R>, V)>)>
    where
        G: RangeBounds<R> + Clone,
    {
        let starts_range = self
            .starts
            .range(range.clone())
            .flat_map(|(r, v)| v.iter().map(move |&v| (false, r, v)));
        let ends_range = self
            .ends
            .range(range.clone())
            .flat_map(|(r, v)| v.iter().map(move |&v| (true, r, v)));
        starts_range.merge_by(ends_range, |(_, r1, _), (_, r2, _)| r1 < r2)
    }

    pub fn remove(&mut self, entry: &Entry<R, V, A>) -> (Range<R>, V) {
        if !(self as *const Self).eq(&entry.tree) {
            panic!("Entry is not from this tree!");
        }
        // SAFETY: we are removing a valid node from the tree.
        unsafe {
            self.remove_node(entry.node)
        }
    }

    pub fn endpoints_with_values_during<G>(&self, range: &G) -> impl Iterator<Item = (bool, &R, &V)>
    where
        G: RangeBounds<R> + Clone,
    {
        self.endpoints_with_nodes_during(range)
            .map(|(is_end, r, node)| {
                // SAFETY: all pointers should point to valid nodes.
                let value: &V = unsafe { &(*(node.as_ptr())).1 };
                (is_end, r, value)
            })
    }

    pub fn endpoints_with_values(&self) -> impl Iterator<Item = (bool, &R, &V)> {
        self.endpoints_with_values_during(&..)
    }

    pub fn endpoints_with_entries_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (bool, &R, Entry<R, V, A>)>
    where
        G: RangeBounds<R> + Clone,
    {
        self.endpoints_with_nodes_during(range)
            .map(|(is_end, r, node)| {
                let entry = Entry {
                    tree: self as *const Self,
                    node,
                };
                (is_end, r, entry)
            })
    }

    pub fn endpoints_with_entries(&self) -> impl Iterator<Item = (bool, &R, Entry<R, V, A>)> {
        self.endpoints_with_entries_during(&..)
    }

    pub fn entries_by_start(&self) -> impl Iterator<Item = Entry<R, V, A>> {
        self.nodes_by_start().map(|node| Entry {
            tree: self as *const Self,
            node,
        })
    }

    pub fn entries_by_end(&self) -> impl Iterator<Item = Entry<R, V, A>> {
        self.nodes_by_end().map(|node| Entry {
            tree: self as *const Self,
            node,
        })
    }

    /// Returns an iterator over all entries in the tree, ordered by interval starts.
    pub fn iter_by_start(&self) -> impl Iterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_tuples(self.nodes_by_start()) }
    }

    pub fn into_iter_by_start(mut self) -> impl Iterator<Item = (Range<R>, V)>
    where
        A: 'static + Clone,
    {
        let alloc = self.alloc.clone();
        let starts = mem::take(&mut self.starts);
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
    pub fn iter_by_end(&self) -> impl Iterator<Item = (&Range<R>, &V)> {
        // SAFETY: all pointers should point to valid nodes.
        unsafe { nodes_to_tuples(self.nodes_by_end()) }
    }

    pub fn into_iter_by_end(mut self) -> impl Iterator<Item = (Range<R>, V)>
    where
        A: 'static + Clone,
    {
        let alloc = self.alloc.clone();
        let ends = mem::take(&mut self.ends);
        mem::forget(self);

        ends.into_values()
            .flat_map(|v| v.into_iter())
            .map(move |node| {
                // SAFETY: all pointers should point to valid nodes.
                let node: Box<(Range<R>, V), _> = unsafe { Box::from_non_null_in(node, &alloc) };
                *node
            })
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
            (1..4, "b"),
            (1..2, "c"),
            (6..7, "d"),
            (3..10, "e"),
            (5..6, "f"),
            (8..10, "g")
        ]);
        println!("{:?}", tree);
        assert_eq!(tree.len(), 7);

        let subtree = tree
            .iter_during(&(1..6))
            .map(|(r, &v)| (r.clone(), v))
            .collect::<IntervalTree<_, _>>();
        println!("{:?}", subtree);
        assert_eq!(subtree.len(), 3);

        let subtree = tree
            .iter_overlaps(&(1..6))
            .map(|(r, &v)| (r.clone(), v))
            .collect::<IntervalTree<_, _>>();
        println!("{:?}", subtree);
        assert_eq!(subtree.len(), 5);

        let to_remove = tree.entries_by_start().nth(2).unwrap();
        let removed = tree.remove(&to_remove);
        println!("Removed: {:?}", removed);
        println!("{:?}", tree);
        assert_eq!(tree.len(), 6);

        tree.retain(|k, v| {
            if let Some(ch) = v.chars().next() {
                ch <= 'd' && k.start >= 2
            } else {
                false
            }
        });
        println!("{:?}", tree);
        assert_eq!(tree.len(), 1);

        tree.clear();
        println!("{:?}", tree);
        assert_eq!(tree.len(), 0);
    }
}
