use std::ops::{
    Bound::{self, *},
    RangeBounds,
};

/// Helper trait for bounds. Allows for conversion between inclusive and exclusive bounds.
pub trait BoundExt: Sized {
    type Output = Self;
    /// Convert inclusive bounds to exclusive ones, and vice versa.
    fn invert_inclusiveness(self) -> Self::Output;
}

impl<T> BoundExt for Bound<T> {
    fn invert_inclusiveness(self) -> Self::Output {
        match self {
            Included(x) => Excluded(x),
            Excluded(x) => Included(x),
            Unbounded => Unbounded,
        }
    }
}

impl<T> BoundExt for (Bound<T>, Bound<T>) {
    fn invert_inclusiveness(self) -> Self::Output {
        (self.0.invert_inclusiveness(), self.1.invert_inclusiveness())
    }
}

/// Extension trait for [`RangeBounds`].
pub trait RangeBoundsExt<T>: RangeBounds<T> {
    /// Get the start and end bounds of the range as a tuple.
    fn bounds(&self) -> (Bound<&T>, Bound<&T>) {
        (self.start_bound(), self.end_bound())
    }
}

impl<T, G: RangeBounds<T>> RangeBoundsExt<T> for G {}
