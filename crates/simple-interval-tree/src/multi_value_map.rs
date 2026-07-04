use std::{
    alloc::{Allocator, Global},
    collections::BTreeMap,
    fmt::Debug,
    iter,
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

pub struct WithKey<K> {
    key: K,
}

impl<K: Clone, V> FnOnce<(V,)> for WithKey<K> {
    type Output = (K, V);
    extern "rust-call" fn call_once(self, args: (V,)) -> Self::Output {
        self.call(args)
    }
}

impl<K: Clone, V> FnMut<(V,)> for WithKey<K> {
    extern "rust-call" fn call_mut(&mut self, args: (V,)) -> Self::Output {
        self.call(args)
    }
}

impl<K: Clone, V> Fn<(V,)> for WithKey<K> {
    extern "rust-call" fn call(&self, args: (V,)) -> Self::Output {
        (self.key.clone(), args.0)
    }
}

type Bucket<V> = SmallVec<[V; 1]>;
type BucketIter<K, V> = iter::Map<<Bucket<V> as IntoIterator>::IntoIter, WithKey<K>>;

pub struct ToBucketIter;

impl<K: Clone, V> FnOnce<((K, Bucket<V>),)> for ToBucketIter {
    type Output = BucketIter<K, V>;
    extern "rust-call" fn call_once(self, args: ((K, Bucket<V>),)) -> Self::Output {
        self.call(args)
    }
}

impl<K: Clone, V> FnMut<((K, Bucket<V>),)> for ToBucketIter {
    extern "rust-call" fn call_mut(&mut self, args: ((K, Bucket<V>),)) -> Self::Output {
        self.call(args)
    }
}

impl<K: Clone, V> Fn<((K, Bucket<V>),)> for ToBucketIter {
    extern "rust-call" fn call(&self, args: ((K, Bucket<V>),)) -> Self::Output {
        let ((k, bucket),) = args;
        bucket.into_iter().map(WithKey { key: k })
    }
}

type MapIntoIter<K, V, A> = <BTreeMap<K, Bucket<V>, A> as IntoIterator>::IntoIter;
pub type IntoIter<K, V, A> = iter::FlatMap<MapIntoIter<K, V, A>, BucketIter<K, V>, ToBucketIter>;

impl<K: Ord + Clone, V, A: Allocator + Clone> IntoIterator for MultiValueBTreeMap<K, V, A> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V, A>;

    fn into_iter(self) -> Self::IntoIter {
        self.map.into_iter().flat_map(ToBucketIter)
    }
}

impl<K: Ord, V, A: Allocator + Clone> MultiValueBTreeMap<K, V, A> {
    pub fn insert(&mut self, k: K, v: V) {
        self.map.entry(k).or_default().push(v);
        self.len += 1;
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
