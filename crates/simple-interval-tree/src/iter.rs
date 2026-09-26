use std::{
    iter::FusedIterator,
    ops::{Bound::*, Range, RangeBounds},
};

use itertools::Itertools as _;

use crate::{
    BoundExt as _, Endpoint, EndpointRaw, IntervalTree, IntervalTreeData, Node, RangeBoundsExt as _,
};

#[derive(Debug, Clone)]
pub struct IntoIter<K, V>(<IntervalTreeData<K, V> as IntoIterator>::IntoIter);
pub struct Iter<'a, K, V>(slotmap::basic::Iter<'a, Node, (Range<K>, V)>);

impl<K, V> Iterator for IntoIter<K, V> {
    type Item = (Range<K>, V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|v| v.1)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<'a, K, V> Iterator for Iter<'a, K, V> {
    type Item = (&'a Range<K>, &'a V);
    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(|(_, (range, value))| (range, value))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl<K, V> FusedIterator for IntoIter<K, V> {}
impl<K, V> FusedIterator for Iter<'_, K, V> {}

impl<K, V> ExactSizeIterator for IntoIter<K, V> {}
impl<K, V> ExactSizeIterator for Iter<'_, K, V> {}

impl<K, V> IntoIterator for IntervalTree<K, V> {
    type IntoIter = IntoIter<K, V>;
    type Item = (Range<K>, V);
    fn into_iter(self) -> Self::IntoIter {
        IntoIter(self.data.into_iter())
    }
}

impl<K, V> IntervalTree<K, V> {
    pub fn iter(&self) -> Iter<'_, K, V> {
        Iter(self.data.iter())
    }
}

impl<K, V> IntervalTree<K, V>
where
    K: Ord,
{
    pub fn nodes_by_start(&self) -> impl DoubleEndedIterator<Item = Node> {
        self.starts.values().flat_map(|v| v.iter().copied())
    }

    pub fn nodes_by_end(&self) -> impl DoubleEndedIterator<Item = Node> {
        self.ends.values().flat_map(|v| v.iter().copied())
    }

    pub fn nodes_overlaps<R>(&self, range: R) -> impl Iterator<Item = Node>
    where
        R: RangeBounds<K>,
    {
        // start < range.end && end > range.start
        self.starts
            .range((Unbounded, range.end_bound()))
            .flat_map(|(_, nodes)| nodes.iter().copied())
            .filter(move |&node| {
                // SAFETY: `node` must be contained in `data`
                let end = unsafe { &self.data.get_unchecked(node).0.end };
                (range.start_bound(), Unbounded)
                    .invert_inclusiveness()
                    .contains(end)
            })
    }

    pub fn nodes_during<R>(&self, range: R) -> impl Iterator<Item = Node>
    where
        R: RangeBounds<K>,
    {
        // start >= range.start && end <= range.end
        self.starts
            .range(range.bounds())
            .flat_map(|(_, nodes)| nodes.iter().copied())
            .filter(move |&node| {
                // SAFETY: `node` must be contained in `data`
                let end = unsafe { &self.data.get_unchecked(node).0.end };
                range.bounds().invert_inclusiveness().contains(end)
            })
    }

    pub fn nodes_starts_during<R>(&self, range: R) -> impl DoubleEndedIterator<Item = Node>
    where
        R: RangeBounds<K>,
    {
        self.starts
            .range((range.start_bound(), range.end_bound()))
            .flat_map(|(_, v)| v.iter().copied())
    }

    pub fn nodes_ends_during<R>(&self, range: R) -> impl DoubleEndedIterator<Item = Node>
    where
        R: RangeBounds<K>,
    {
        self.ends
            .range((range.start_bound(), range.end_bound()).invert_inclusiveness())
            .flat_map(|(_, v)| v.iter().copied())
    }

    fn endpoints_raw_during<'a, R>(&'a self, range: R) -> impl Iterator<Item = EndpointRaw<'a, K>>
    where
        R: RangeBounds<K>,
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

    pub fn iter_endpoints_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = Endpoint<'a, K, V>>
    where
        R: RangeBounds<K>,
    {
        self.endpoints_raw_during(range)
            .map(|EndpointRaw { is_end, at, node }| Endpoint {
                is_end,
                at,
                // SAFETY: `node` must be contained in `data`
                data: unsafe { self.data.get_unchecked(node) },
            })
    }

    pub fn iter_endpoints<'a>(&'a self) -> impl Iterator<Item = Endpoint<'a, K, V>> {
        self.iter_endpoints_during(..)
    }

    /// Returns an iterator over all entries in the tree, ordered by interval starts.
    pub fn iter_by_start(&self) -> impl DoubleEndedIterator<Item = (&Range<K>, &V)> {
        self.nodes_by_start().map(|node| {
            // SAFETY: `node` must be contained in `data`
            let (k, v) = unsafe { self.data.get_unchecked(node) };
            (k, v)
        })
    }

    pub fn into_iter_by_start(self) -> impl DoubleEndedIterator<Item = (Range<K>, V)> {
        let Self {
            starts, mut data, ..
        } = self;
        starts
            .into_values()
            .flat_map(|v| v.into_iter())
            .map(move |node| {
                // SAFETY: `node` must be contained in `data`
                unsafe { data.remove(node).unwrap_unchecked() }
            })
    }

    /// Returns an iterator over all entries in the tree, ordered by interval ends.
    pub fn iter_by_end(&self) -> impl DoubleEndedIterator<Item = (&Range<K>, &V)> {
        self.nodes_by_end().map(|node| {
            // SAFETY: `node` must be contained in `data`
            let (k, v) = unsafe { self.data.get_unchecked(node) };
            (k, v)
        })
    }

    pub fn into_iter_by_end(self) -> impl DoubleEndedIterator<Item = (Range<K>, V)> {
        let Self { ends, mut data, .. } = self;
        ends.into_values()
            .flat_map(|v| v.into_iter())
            .map(move |node| {
                // SAFETY: `node` must be contained in `data`
                unsafe { data.remove(node).unwrap_unchecked() }
            })
    }

    pub fn iter_overlaps<R>(&self, range: R) -> impl Iterator<Item = (&Range<K>, &V)>
    where
        R: RangeBounds<K>,
    {
        self.nodes_overlaps(range).map(|node| {
            // SAFETY: `node` must be contained in `data`
            let (k, v) = unsafe { self.data.get_unchecked(node) };
            (k, v)
        })
    }

    pub fn iter_during<R>(&self, range: R) -> impl Iterator<Item = (&Range<K>, &V)>
    where
        R: RangeBounds<K>,
    {
        self.nodes_during(range).map(|node| {
            // SAFETY: `node` must be contained in `data`
            let (k, v) = unsafe { self.data.get_unchecked(node) };
            (k, v)
        })
    }

    /// Returns an iterator over entries whose starting point is within the given range.
    pub fn iter_starts_during<R: RangeBounds<K>>(
        &self,
        range: R,
    ) -> impl DoubleEndedIterator<Item = (&Range<K>, &V)> {
        self.nodes_starts_during(range).map(|node| {
            // SAFETY: `node` must be contained in `data`
            let (k, v) = unsafe { self.data.get_unchecked(node) };
            (k, v)
        })
    }

    /// Returns an iterator over entries whose ending point is within the given range.
    pub fn iter_ends_during<R: RangeBounds<K>>(
        &self,
        range: R,
    ) -> impl DoubleEndedIterator<Item = (&Range<K>, &V)> {
        self.nodes_ends_during(range).map(|node| {
            // SAFETY: `node` must be contained in `data`
            let (k, v) = unsafe { self.data.get_unchecked(node) };
            (k, v)
        })
    }
}
