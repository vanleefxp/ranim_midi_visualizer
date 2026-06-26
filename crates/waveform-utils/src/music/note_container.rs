use std::{
    alloc::Allocator,
    collections::BTreeMap,
    num::NonZeroU64,
    ops::{Bound::*, IntoBounds as _, Range},
};

use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};
use simple_interval_tree::{Endpoint, IntervalTree};

use super::{Metric, MetricRange, Note};

pub struct NoteInstant<'a, Pitch: 'a> {
    pub is_end: bool,
    pub at: Metric,
    pub pair: (Range<Metric>, &'a Note<Pitch>),
}

impl<'a, Pitch: 'a> From<Endpoint<'a, Metric, Note<Pitch>>> for NoteInstant<'a, Pitch> {
    fn from(value: Endpoint<'a, Metric, Note<Pitch>>) -> Self {
        let Endpoint {
            is_end,
            at,
            pair: (range, note),
        } = value;
        let at = *at;
        let range = range.clone();
        NoteInstant {
            is_end,
            at,
            pair: (range, note),
        }
    }
}

pub trait NoteContainer {
    type Pitch;
    type Pos = ();
    // notes_*_with_pos: generate position (e.g. on which staff / voice are the notes)
    // together with notes and their corresponding time ranges

    fn notes_by_start(
        &self,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>;

    fn notes_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange;

    fn notes_overlaps<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange;

    fn notes_start_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange;

    fn notes_end_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange;

    fn note_instants_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, NoteInstant<'a, Self::Pitch>)>
    where
        R: MetricRange;

    fn note_instants(&self) -> impl Iterator<Item = (Self::Pos, NoteInstant<'_, Self::Pitch>)> {
        self.note_instants_during(..)
    }

    fn note_count(&self) -> usize {
        self.notes_by_start().count()
    }

    fn note_count_iter(&self) -> impl Iterator<Item = (Metric, usize)> {
        self.notes_by_start().scan(0usize, |count, (_, range, _)| {
            *count += 1;
            Some((range.start, *count))
        })
    }

    fn note_count_fn(&self) -> LadderFn<Metric, usize> {
        self.note_count_iter().collect()
    }

    fn note_rate_iter(&self, window: NonZeroU64) -> impl Iterator<Item = (Metric, usize)> {
        // instants where the start of notes enter or exit the time window
        // and how many notes flows in or out at the instant
        // NPS value only changes at these instants
        let mut nps_changes: BTreeMap<Metric, isize> = BTreeMap::new();
        for (_, range, _) in self.notes_by_start() {
            let enter_time = range.start;
            let exit_time = range.start + window.get();
            nps_changes
                .entry(enter_time)
                .and_modify(|cnt| *cnt += 1)
                .or_insert(1);
            nps_changes
                .entry(exit_time)
                .and_modify(|cnt| *cnt -= 1)
                .or_insert(-1);
        }

        // accumulate the number of notes in window and divide it by the window length to get NPS values
        nps_changes
            .into_iter()
            .scan(0usize, move |n_in_window, (time, n_enter)| {
                if n_enter > 0 {
                    *n_in_window += n_enter as usize;
                } else {
                    *n_in_window -= (-n_enter) as usize;
                }
                Some((time, *n_in_window))
            })
    }

    fn note_rate_fn(&self, window: NonZeroU64) -> LadderFn<Metric, usize> {
        self.note_rate_iter(window).collect()
    }

    /// Number of notes played in the time window.
    fn note_rate(&self, time: u64, window: NonZeroU64) -> usize {
        let time_range = time.saturating_sub(window.get())..time;
        self.notes_start_during(time_range).count()
    }

    /// **Legato index** is a measure describing how continuously a series of notes are played.
    /// This index was put forward by Wiwi Kuan in his Pianometer program.
    /// See: https://nicechord.com/pianometer/
    ///
    /// The calculation of legato index in a certain time window is done as follows:
    ///
    /// + take the intersection of the time window and note ranges
    /// + sum the lengths of the intersecting parts of the notes and the time window
    /// + divide the sum by the length of the time window
    ///
    fn legato_index(&self, time: u64, window: NonZeroU64) -> f64 {
        let time_range = time.saturating_sub(window.get())..time;
        let duration_sum: u64 = {
            self.notes_overlaps(time_range.clone())
                .map(|(_, range, _)| range.clone())
                .map(|range| {
                    let (start, end) = time_range.clone().intersect(range);
                    match (start, end) {
                        (Included(a) | Excluded(a), Included(b) | Excluded(b)) => a.abs_diff(b),
                        _ => unreachable!(),
                    }
                })
                .sum()
        };
        duration_sum as f64 / window.get() as f64
    }

    /// Calculates the legato index of the whole song. The returned result is a callable function.
    fn legato_fn(&self, window: NonZeroU64) -> SegmentedLinearFn<u64, f64> {
        // `legato_index` calculate the legato index directly by definition,
        // However, for the computation of legato index of the whole song, this approach can be optimized given the
        // observation that the changing of legato index is a segmented linear function to time.
        //
        // the legato score function is _additive_, meaning that we can sum the legato score functions of each note
        // to get the total legato score function of the song.
        // So the first step is to create the legato score function for each note.
        self.notes_by_start()
            .map(|(_, Range { start, end }, _)| {
                // When it comes to the calculation of single-note legato score function, there are two cases:
                let duration = end - start;
                let window = window.get();

                SegmentedLinearFn::from_iter(if duration > window {
                    // Case 1: the note is longer than the time window
                    //
                    //                  =========                     window
                    //                           -----------------    t = start             legato = 0
                    //                  -----------------             t = start + window    legato = 1
                    //          -----------------                     t = end               legato = 1
                    // -----------------                              t = end + window      legato = 0
                    //
                    [
                        (start, 0.),
                        (start + window, 1.),
                        (end, 1.),
                        (end + window, 0.),
                    ]
                } else {
                    // Case 2: the note is shorter than the time window
                    //
                    //                  ========                      window
                    //                          -----                 t = start             legato = 0
                    //                     -----                      t = end               legato = duration / window
                    //                  -----                         t = start + window    legato = duration / window
                    //             -----                              t = end + window      legato = 0
                    //
                    let max_value = duration as f64 / window as f64;
                    [
                        (start, 0.),
                        (end, max_value),
                        (start + duration, max_value),
                        (end + window, 0.),
                    ]
                })
            })
            .sum()
    }

    fn note_rate_max_iter(&self, window: NonZeroU64) -> impl Iterator<Item = (Metric, usize)> {
        self.note_rate_iter(window)
            .scan(0, |nps_max, (time, nps)| {
                if nps > *nps_max {
                    *nps_max = nps;
                    Some(Some((time, nps)))
                } else {
                    Some(None)
                }
            })
            .flatten()
    }

    fn note_rate_max_fn(&self, window: NonZeroU64) -> LadderFn<Metric, usize> {
        self.note_rate_max_iter(window).collect()
    }
}

macro with_pos($iter:expr) {
    $iter.map(|(range, note)| ((), range.clone(), note))
}

macro instants_with_pos($iter:expr) {
    $iter.map(|instant| ((), instant.into()))
}

impl<Pitch, A: Allocator + Clone> NoteContainer for IntervalTree<Metric, Note<Pitch>, A> {
    type Pitch = Pitch;

    fn notes_by_start(&self) -> impl Iterator<Item = ((), Range<Metric>, &Note<Pitch>)> {
        with_pos!(self.iter_by_start())
    }

    fn notes_during<R>(&self, range: R) -> impl Iterator<Item = ((), Range<Metric>, &Note<Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.iter_during(range))
    }

    fn notes_overlaps<R>(&self, range: R) -> impl Iterator<Item = ((), Range<Metric>, &Note<Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.iter_overlaps(range))
    }

    fn notes_start_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = ((), Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.iter_starts_during(range))
    }

    fn notes_end_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = ((), Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        with_pos!(self.iter_ends_during(range))
    }

    fn note_instants_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = ((), NoteInstant<'a, Self::Pitch>)>
    where
        R: MetricRange,
    {
        instants_with_pos!(self.iter_endpoints_during(range))
    }
}
