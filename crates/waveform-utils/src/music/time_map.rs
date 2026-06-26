use std::{
    iter,
    num::NonZero,
    ops::{Deref, Range},
};

use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};
use tracing::{debug, info};

use super::{ControlContainer, Metric, MetricRange, Note, NoteContainer, NoteInstant, Tempo};

pub type TimeMap = SegmentedLinearFn<Metric, Metric>;

pub struct MappedNoteContainer<'a, Container, TimeMapRef: Deref<Target = TimeMap> + 'a> {
    pub(crate) orig: &'a Container,
    pub(crate) time_map: TimeMapRef,
}

pub struct MappedControlContainer<'a, Container, TimeMapRef: Deref<Target = TimeMap> + 'a> {
    pub(crate) orig: &'a Container,
    pub(crate) time_map: TimeMapRef,
}

macro to_mapped($iter:expr,$time_map:expr) {
    $iter.map(|(pos, range, note)| {
        let Range { start, end } = range;
        let start = $time_map.eval(&start, true);
        let end = $time_map.eval(&end, true);
        (pos, start..end, note)
    })
}

impl<Container: NoteContainer, T: Deref<Target = TimeMap>> NoteContainer
    for MappedNoteContainer<'_, Container, T>
{
    type Pitch = Container::Pitch;
    type Pos = Container::Pos;

    fn notes_by_start(
        &self,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)> {
        to_mapped!(self.orig.notes_by_start(), self.time_map)
    }

    fn notes_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        let range = self.time_map.x_range(range, true);
        to_mapped!(self.orig.notes_during(range), self.time_map)
    }

    fn notes_overlaps<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        let range = self.time_map.x_range(range, true);
        to_mapped!(self.orig.notes_overlaps(range), self.time_map)
    }

    fn notes_start_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        let range = self.time_map.x_range(range, true);
        to_mapped!(self.orig.notes_start_during(range), self.time_map)
    }

    fn notes_end_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        let range = self.time_map.x_range(range, true);
        to_mapped!(self.orig.notes_end_during(range), self.time_map)
    }

    fn note_instants_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, super::NoteInstant<'a, Self::Pitch>)>
    where
        R: MetricRange,
    {
        let range = self.time_map.x_range(range, true);
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

impl<Container: ControlContainer, T: Deref<Target = TimeMap>> ControlContainer
    for MappedControlContainer<'_, Container, T>
{
    type Control = Container::Control;
    type Pos = Container::Pos;

    fn controls_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        R: MetricRange,
    {
        self.orig
            .controls_during(range)
            .map(|(pos, time, control)| {
                let time = self.time_map.eval(&time, true);
                (pos, time, control)
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

    for (tick, tempo) in tempo
        .iter()
        .map(|(&k, &v)| (k, v))
        .chain(iter::once((tick_duration, last_tempo)))
    // add a last point at the end of the song
    {
        let n_beats = (tick - cur_tick) as f64 / beat_resolution.get() as f64;
        let dt = (cur_tempo.get() as f64 * n_beats) as u64;
        cur_time_units += dt;
        cur_tick = tick;
        debug!(
            "Tempo change: {} -> {} time units per beat",
            cur_tempo, tempo
        );
        debug!(
            "Inserted point at tick {}, time unit {}",
            cur_tick, cur_time_units
        );
        if time_to_beat {
            time_map.insert(cur_time_units, cur_tick);
        } else {
            time_map.insert(cur_tick, cur_time_units);
        }
        cur_tempo = tempo;
    }
    info!("Time map generated.");

    time_map
}

pub struct MappedNoteControlContainer<'a, Container: NoteContainer + ControlContainer, TimeMapRef: Deref<Target = TimeMap> + 'a> {
    pub notes: MappedNoteContainer<'a, Container, TimeMapRef>,
    pub controls: MappedControlContainer<'a, Container, TimeMapRef>,
}

impl<Container: NoteContainer + ControlContainer, TimeMapRef: Deref<Target = TimeMap>> NoteContainer
    for MappedNoteControlContainer<'_, Container, TimeMapRef>
{
    type Pitch = Container::Pitch;
    type Pos = <Container as NoteContainer>::Pos;

    fn notes_by_start(
        &self,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &super::Note<Self::Pitch>)> {
        self.notes.notes_by_start()
    }

    fn notes_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &super::Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        self.notes.notes_during(range)
    }

    fn notes_overlaps<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &super::Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        self.notes.notes_overlaps(range)
    }

    fn notes_start_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &super::Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        self.notes.notes_start_during(range)
    }

    fn notes_end_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Range<Metric>, &super::Note<Self::Pitch>)>
    where
        R: MetricRange,
    {
        self.notes.notes_end_during(range)
    }

    fn note_instants_during<'a, R>(
        &'a self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, super::NoteInstant<'a, Self::Pitch>)>
    where
        R: MetricRange,
    {
        self.notes.note_instants_during(range)
    }
}

impl<'a, Container: NoteContainer + ControlContainer, TimeMapRef: Deref<Target = TimeMap> + 'a> ControlContainer
    for MappedNoteControlContainer<'a, Container, TimeMapRef>
{
    type Control = Container::Control;
    type Pos = <Container as ControlContainer>::Pos;

    fn controls_during<R>(
        &self,
        range: R,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        R: MetricRange,
    {
        self.controls.controls_during(range)
    }
}