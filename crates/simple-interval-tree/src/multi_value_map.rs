use std::{
    alloc::{Allocator, Global},
    collections::BTreeMap,
    fmt::Debug,
    ops::RangeBounds,
};

use smallvec::SmallVec;

#[derive(Clone)]
pub struct MultiValueBTreeMap<K, V, A: Allocator + Clone = Global> {
    map: BTreeMap<K, SmallVec<[V; 1]>, A>,
    len: usize,
}

impl<K, V> Default for MultiValueBTreeMap<K, V> {
    fn default() -> Self {
        Self {
            map: BTreeMap::default(),
            len: 0,
        }
    }
}

impl<K: Debug + Ord, V: Debug, A: Allocator + Clone> Debug for MultiValueBTreeMap<K, V, A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map().entries(self.iter()).finish()
    }
}

impl<K, V> MultiValueBTreeMap<K, V> {
    pub fn new() -> Self {
        Self {
            map: BTreeMap::new(),
            len: 0,
        }
    }
}

impl<K, V, A: Allocator + Clone> MultiValueBTreeMap<K, V, A> {
    pub fn new_in(alloc: A) -> Self {
        Self {
            map: BTreeMap::new_in(alloc),
            len: 0,
        }
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for MultiValueBTreeMap<K, V> {
    fn from_iter<T: IntoIterator<Item = (K, V)>>(iter: T) -> Self {
        let mut map = Self::default();
        map.extend(iter);
        map
    }
}

impl<K: Ord, V, A: Allocator + Clone> Extend<(K, V)> for MultiValueBTreeMap<K, V, A> {
    fn extend<T: IntoIterator<Item = (K, V)>>(&mut self, iter: T) {
        for (k, v) in iter {
            self.insert(k, v);
        }
    }
}

impl<K: Ord, V, A: Allocator + Clone> MultiValueBTreeMap<K, V, A> {
    pub fn insert(&mut self, k: K, v: V) {
        self.map.entry(k).or_default().push(v);
        self.len += 1;
    }

    // Implementing `IntoIterator` requires a concrete `Iterator` type which is not easily feasible
    // because of the use of closure.
    #[allow(clippy::should_implement_trait)]
    pub fn into_iter(self) -> impl Iterator<Item = (K, V)>
    where
        K: Clone,
    {
        self.map
            .into_iter()
            .flat_map(|(k, v)| v.into_iter().map(move |v| (k.clone(), v)))
    }

    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.map
            .iter()
            .flat_map(|(k, v)| v.iter().map(move |v| (k, v)))
    }

    pub fn range<G: RangeBounds<K>>(&self, range: G) -> impl Iterator<Item = (&K, &V)> {
        self.map
            .range(range)
            .flat_map(|(k, v)| v.iter().map(move |v| (k, v)))
    }

    pub fn retain(&mut self, mut f: impl FnMut(&K, &mut V) -> bool) {
        self.map.retain(|k, v| {
            v.retain(|v| {
                let keep = f(k, v);
                if !keep {
                    self.len -= 1;
                }
                keep
            });
            !v.is_empty()
        });
    }
}

impl<K, V, A: Allocator + Clone> MultiValueBTreeMap<K, V, A> {
    pub fn clear(&mut self) {
        self.map.clear();
        self.len = 0;
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}
