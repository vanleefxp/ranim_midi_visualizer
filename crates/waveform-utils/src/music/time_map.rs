use std::{iter, num::NonZero, ops::Range};

use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};

use crate::music::{Metric, NoteContainer, NoteInstant, Tempo};

pub type TimeMap = SegmentedLinearFn<Metric, Metric>;

pub struct MappedNoteContainer<'a, Container>
where
    Container: NoteContainer,
{
    pub(crate) orig: &'a Container,
    pub(crate) time_map: &'a TimeMap,
}

macro to_mapped($iter:expr,$time_map:expr) {
    $iter.map(|(pos, range, note)| {
        let Range { start, end } = range;
        let start = $time_map.eval(&start, true);
        let end = $time_map.eval(&end, true);
        (pos, start..end, note)
    })
}

impl<Container: NoteContainer> NoteContainer for MappedNoteContainer<'_, Container> {
    type Pitch = Container::Pitch;
    type Pos = Container::Pos;

    fn notes_by_start(
        &self,
    ) -> impl Iterator<
        Item = (
            Self::Pos,
            std::ops::Range<Metric>,
            &super::Note<Self::Pitch>,
        ),
    > {
        to_mapped!(self.orig.notes_by_start(), self.time_map)
    }

    fn notes_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<
        Item = (
            Self::Pos,
            std::ops::Range<Metric>,
            &super::Note<Self::Pitch>,
        ),
    >
    where
        G: std::ops::RangeBounds<Metric>,
    {
        to_mapped!(self.orig.notes_during(range), self.time_map)
    }

    fn notes_overlaps<G>(
        &self,
        range: &G,
    ) -> impl Iterator<
        Item = (
            Self::Pos,
            std::ops::Range<Metric>,
            &super::Note<Self::Pitch>,
        ),
    >
    where
        G: std::ops::RangeBounds<Metric>,
    {
        to_mapped!(self.orig.notes_overlaps(range), self.time_map)
    }

    fn notes_start_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<
        Item = (
            Self::Pos,
            std::ops::Range<Metric>,
            &super::Note<Self::Pitch>,
        ),
    >
    where
        G: std::ops::RangeBounds<Metric>,
    {
        to_mapped!(self.orig.notes_start_during(range), self.time_map)
    }

    fn notes_end_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<
        Item = (
            Self::Pos,
            std::ops::Range<Metric>,
            &super::Note<Self::Pitch>,
        ),
    >
    where
        G: std::ops::RangeBounds<Metric>,
    {
        to_mapped!(self.orig.notes_end_during(range), self.time_map)
    }

    fn note_instants_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, super::NoteInstant<'a, Self::Pitch>)>
    where
        G: std::ops::RangeBounds<Metric>,
    {
        self.orig.note_instants_during(range).map(|(pos, instant)| {
            let NoteInstant {
                is_end,
                at,
                pair: (range, note),
            } = instant;
            let start = self.time_map.eval(&range.start, true);
            let end = self.time_map.eval(&range.end, true);
            let instant = NoteInstant {
                is_end,
                at: self.time_map.eval(&at, true),
                pair: (start..end, note),
            };
            (pos, instant)
        })
    }
}

pub(crate) fn generate_time_map(
    tempo: &LadderFn<Metric, Tempo>,
    tick_duration: Metric,            // duration of the song in ticks
    beat_resolution: NonZero<Metric>, // ticks per beat
    time_resolution: NonZero<Metric>, // time units per second
    time_to_beat: bool,               // forward or inverse map
) -> SegmentedLinearFn<Metric, Metric> {
    let default_tempo = time_resolution;

    let mut time_map = SegmentedLinearFn::from_iter([(0, 0)]);
    let mut cur_time_units = 0;
    let mut cur_tick = 0;

    // tempo, measured in time units per beat
    let mut cur_tempo = default_tempo;
    let last_tempo = tempo
        .last_key_value()
        .map(|(_, &v)| v)
        .unwrap_or(default_tempo);

    for (tick, time_units_per_beat) in tempo
        .iter()
        .map(|(&k, &v)| (k, v))
        .chain(iter::once((tick_duration, last_tempo)))
    // add a last point at the end of the song
    {
        let n_beats = (tick - cur_tick) as f64 / beat_resolution.get() as f64;
        let dt = (cur_tempo.get() as f64 * n_beats) as u64;
        cur_time_units += dt;
        cur_tick = tick;
        if time_to_beat {
            time_map.insert(cur_time_units, cur_tick);
        } else {
            time_map.insert(cur_tick, cur_time_units);
        }
        cur_tempo = time_units_per_beat;
    }

    time_map
}
