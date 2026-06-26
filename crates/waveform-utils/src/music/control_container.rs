use std::{alloc::Allocator, ops::RangeBounds};

use simple_interval_tree::MultiValueBTreeMap;

use super::Metric;

pub trait ControlContainer {
    type Control;
    type Pos = ();

    fn controls_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        G: RangeBounds<Metric>;

    fn controls(&self) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)> {
        self.controls_during(&..)
    }
}

impl<Control, A: Allocator + Clone> ControlContainer for MultiValueBTreeMap<Metric, Control, A> {
    type Control = Control;

    fn controls_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        G: RangeBounds<Metric>,
    {
        self.range((range.start_bound(), range.end_bound()))
            .map(|(&k, v)| ((), k, v))
    }
}
