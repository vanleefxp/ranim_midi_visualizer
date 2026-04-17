use std::ops::{Index as _, IndexMut as _};

pub trait IndexCyc<Idx: ?Sized> {
    type Output: ?Sized;
    /// Performs cyclic indexing operation. Out-of-bounds index is taken modulus over the length of the slice so this
    /// method only panics if the slice is empty.
    fn index_cyc(&self, index: Idx) -> &Self::Output;
}

pub trait IndexCycMut<Idx: ?Sized> {
    type Output: ?Sized;
    /// Performs mutable cyclic indexing operation. Out-of-bounds index is taken modulus over the length of the slice
    /// so this method only panics if the slice is empty.
    fn index_cyc_mut(&mut self, index: Idx) -> &mut Self::Output;
}

impl<T> IndexCyc<usize> for [T] {
    type Output = T;

    fn index_cyc(&self, index: usize) -> &Self::Output {
        let index = index % self.len();
        self.index(index)
    }
}

impl<T> IndexCycMut<usize> for [T] {
    type Output = T;

    fn index_cyc_mut(&mut self, index: usize) -> &mut Self::Output {
        let index = index % self.len();
        self.index_mut(index)
    }
}
