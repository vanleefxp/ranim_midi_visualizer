use std::{
    fmt::Debug,
    num::NonZeroU64,
    ops::{Bound::*, IntoBounds as _, Range, RangeBounds},
};

use simple_interval_tree::{Endpoint, IntervalTree};

use crate::music::TimeMap;

use super::{Metric, Note, NoteContainer};

#[derive(Clone)]
pub struct Voice<Pitch = i8> {
    pub notes: IntervalTree<Metric, Note<Pitch>>,
}

impl<Pitch> Debug for Voice<Pitch>
where
    Pitch: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Voice").field("notes", &self.notes).finish()
    }
}

impl<Pitch> Default for Voice<Pitch> {
    fn default() -> Self {
        Self {
            notes: IntervalTree::new(),
        }
    }
}

impl<Pitch> Voice<Pitch> {
    pub(crate) fn remap(self, time_map: &TimeMap) -> Self {
        let timed_notes = self
            .notes
            .into_iter_by_start()
            .map(|(tick_range, note)| {
                let Range { start, end } = tick_range;
                let start_time = time_map.eval(&start, true);
                let end_time = time_map.eval(&end, true);
                (start_time..end_time, note)
            })
            .collect();
        Self { notes: timed_notes }
    }
}

impl<'a, Pitch: 'a> NoteContainer<'a, Pitch> for Voice<Pitch> {
    fn notes_during<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.notes.iter_during(range)
    }

    fn notes_overlaps<G>(
        &'_ self,
        range: &G,
    ) -> impl Iterator<Item = &'_ (Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.notes.iter_overlaps(range)
    }

    fn notes_starts_during<G>(
        &'_ self,
        range: &G,
    ) -> impl Iterator<Item = &'_ (Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.notes.iter_starts_during(range)
    }

    fn notes_ends_during<G>(
        &'_ self,
        range: &G,
    ) -> impl Iterator<Item = &'_ (Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.notes.iter_ends_during(range)
    }

    fn note_instants_during<G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = Endpoint<'a, Metric, Note<Pitch>>>
    where
        G: RangeBounds<Metric>,
    {
        self.notes.iter_endpoints_during(range)
    }

    fn notes_by_start(&self) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)> {
        self.notes.iter_by_start()
    }

    fn note_count(&self) -> usize {
        self.notes.len()
    }

    fn note_rate(&self, time: u64, window: NonZeroU64) -> usize {
        let time_range = time.saturating_sub(window.get())..time;
        self.notes_starts_during(&time_range).count()
    }

    fn legato_index(&self, time: u64, window: NonZeroU64) -> f64 {
        let time_range = time.saturating_sub(window.get())..time;
        let duration_sum: u64 = {
            self.notes_overlaps(&time_range)
                .map(|(r, _)| r.clone())
                .map(|r| {
                    let (start, end) = time_range.clone().intersect(r);
                    match (start, end) {
                        (Included(a) | Excluded(a), Included(b) | Excluded(b)) => a.abs_diff(b),
                        _ => unreachable!(),
                    }
                })
                .sum()
        };
        duration_sum as f64 / window.get() as f64
    }
}
