use std::alloc::Allocator;

use simple_interval_tree::MultiValueBTreeMap;

use crate::music::MetricRange;

use super::Metric;

pub trait ControlContainer {
    type Control;
    type Pos = ();

    fn controls_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        R: MetricRange;

    fn controls(&self) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)> {
        self.controls_during(..)
    }
}

impl<Control, A: Allocator + Clone> ControlContainer for MultiValueBTreeMap<Metric, Control, A> {
    type Control = Control;

    fn controls_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        R: MetricRange,
    {
        self.range(range).map(|(&k, v)| ((), k, v))
    }
}
