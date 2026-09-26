#![feature(
    allocator_api,
    associated_type_defaults,
    btreemap_alloc,
    unboxed_closures,
    fn_traits
)]

mod iter;
pub mod multi_value_map;
pub(crate) mod range;
pub use iter::*;
pub use multi_value_map::MultiValueBTreeMap;
pub use range::*;

use slotmap::{SlotMap, new_key_type};
use smallvec::SmallVec;
use std::{
    collections::BTreeMap,
    fmt::Debug,
    ops::{Index, IndexMut, Range},
};

type IntervalTreeData<K, V> = SlotMap<Node, (Range<K>, V)>;

struct EndpointRaw<'a, K> {
    pub is_end: bool,
    pub at: &'a K,
    pub node: Node,
}

pub struct Endpoint<'a, K, V> {
    pub is_end: bool,
    pub at: &'a K,
    pub data: &'a (Range<K>, V),
}

new_key_type! {
    /// Indicating an [`IntervalTree`] node to access value from.
    pub struct Node;
}

#[derive(Clone)]
pub struct IntervalTree<K, V> {
    starts: BTreeMap<K, SmallVec<[Node; 1]>>,
    ends: BTreeMap<K, SmallVec<[Node; 1]>>,
    data: IntervalTreeData<K, V>,
}

impl<K, V> IntervalTree<K, V> {
    #[inline(always)]
    pub fn new() -> Self {
        Self::default()
    }
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.data.len()
    }
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }
    #[inline(always)]
    pub fn get(&self, node: Node) -> Option<&<Self as Index<Node>>::Output> {
        self.data.get(node)
    }
    #[inline(always)]
    pub fn get_mut(&mut self, node: Node) -> Option<&mut <Self as Index<Node>>::Output> {
        self.data.get_mut(node)
    }
    #[inline(always)]
    pub unsafe fn get_unchecked(&self, node: Node) -> &<Self as Index<Node>>::Output {
        unsafe { self.data.get_unchecked(node) }
    }
    #[inline(always)]
    pub unsafe fn get_unchecked_mut(&mut self, node: Node) -> &mut <Self as Index<Node>>::Output {
        unsafe { self.data.get_unchecked_mut(node) }
    }

    pub fn insert(&mut self, range: Range<K>, value: V)
    where
        K: Ord + Clone,
    {
        let Range { start, end } = range.clone();
        let node = self.data.insert((range, value));
        self.starts.entry(start).or_default().push(node);
        self.ends.entry(end).or_default().push(node);
    }

    pub fn remove(&mut self, node: Node) -> Option<V>
    where
        K: Ord,
    {
        if let Some((Range { start, end }, value)) = self.data.remove(node) {
            self.starts
                .get_mut(&start)
                .map(|v| v.retain(|v| *v != node));
            self.ends.get_mut(&end).map(|v| v.retain(|v| *v != node));
            Some(value)
        } else {
            None
        }
    }

    pub fn retain(&mut self, mut f: impl FnMut(&Range<K>, &mut V) -> bool)
    where
        K: Ord,
    {
        self.data.retain(|_, (range, value)| f(range, value));
        for map in [&mut self.starts, &mut self.ends] {
            map.retain(|_, nodes| {
                nodes.retain(|node| self.data.contains_key(*node));
                !nodes.is_empty()
            });
        }
    }

    pub fn clear(&mut self) {
        self.starts.clear();
        self.ends.clear();
        self.data.clear();
    }
}

impl<K, V> Default for IntervalTree<K, V> {
    fn default() -> Self {
        Self {
            starts: BTreeMap::default(),
            ends: BTreeMap::default(),
            data: SlotMap::default(),
        }
    }
}

impl<K, V> Index<Node> for IntervalTree<K, V> {
    type Output = (Range<K>, V);
    fn index(&self, index: Node) -> &Self::Output {
        &self.data[index]
    }
}

impl<K, V> IndexMut<Node> for IntervalTree<K, V> {
    fn index_mut(&mut self, index: Node) -> &mut Self::Output {
        &mut self.data[index]
    }
}

impl<K, V> FromIterator<(Range<K>, V)> for IntervalTree<K, V>
where
    K: Ord + Clone,
{
    fn from_iter<T: IntoIterator<Item = (Range<K>, V)>>(iter: T) -> Self {
        let mut tree = Self::new();
        tree.extend(iter);
        tree
    }
}

impl<K, V> Extend<(Range<K>, V)> for IntervalTree<K, V>
where
    K: Ord + Clone,
{
    fn extend<T: IntoIterator<Item = (Range<K>, V)>>(&mut self, iter: T) {
        for (range, value) in iter {
            self.insert(range, value);
        }
    }
}

impl<K: Debug, V: Debug> Debug for IntervalTree<K, V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tracing::debug;
    use tracing_test::traced_test;

    const EXAMPLE_TREE_DATA: &[(Range<u32>, &str)] = &[
        (0..3, "a"),
        (1..4, "b"),
        (1..2, "c"), // overlapping interval start
        (6..7, "d"),
        (3..10, "e"),
        (5..6, "f"),
        (8..10, "g"), // overlapping interval end
        (0..12, "h"),
        (1..2, "i"), // overlapping interval start and end
    ];

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
        for (range, value) in tree.iter_during(query_range.clone()) {
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
        for (range, value) in tree.iter_overlaps(query_range.clone()) {
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
        for (range, value) in tree.iter_starts_during(query_range.clone()) {
            debug!("{:?}: {:?}", range, value);
            assert!(query_range.contains(&range.start));
        }
    }

    #[traced_test]
    #[test]
    fn test_iter_ends_during() {
        let tree = build_example_tree();
        let query_range = 1..=4;
        for (range, value) in tree.iter_ends_during(query_range.clone()) {
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
