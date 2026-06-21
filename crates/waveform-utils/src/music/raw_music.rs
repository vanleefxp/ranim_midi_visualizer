use std::{
    num::{NonZero, NonZeroU64},
    ops::{Range, RangeBounds},
};

use itertools::Itertools as _;
use simple_interval_tree::Endpoint;

use crate::music::{ControlContainer, Metric, Note, NoteContainer, PedalControl, Staff};

/// Beat resolution in divisions per time unit.
pub const DEFAULT_BEAT_RESOLUTION: NonZero<Metric> = NonZero::new(480).unwrap();

/// Music without metric information.
#[derive(Debug, Clone)]
pub struct RawMusic<Pitch = i8, Control = PedalControl> {
    /// Total duration of the music in beats.
    pub duration: Metric,
    /// The staves / tracks of the music.
    pub staves: Vec<Staff<Pitch, Control>>,
    /// Number of divisions per second.
    pub resolution: NonZero<Metric>,
}

impl<Pitch, Control> Default for RawMusic<Pitch, Control> {
    fn default() -> Self {
        Self::new(DEFAULT_BEAT_RESOLUTION)
    }
}

impl<'a, Pitch: 'a, Control> NoteContainer<'a, Pitch> for RawMusic<Pitch, Control> {
    fn note_instants_during<G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = Endpoint<'a, Metric, Note<Pitch>>>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .map(|v| v.note_instants_during(range))
            .kmerge_by(|ep1, ep2| ep1.at < ep2.at)
    }

    fn notes_by_start(&self) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)> {
        self.staves
            .iter()
            .map(|v| v.notes_by_start())
            .kmerge_by(|(range1, _), (range2, _)| range1.start < range2.start)
    }

    fn note_count(&self) -> usize {
        self.staves.iter().map(|v| v.note_count()).sum()
    }

    fn notes_during<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .flat_map(|voice| voice.notes_during(range))
    }

    fn notes_overlaps<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .flat_map(|voice| voice.notes_overlaps(range))
    }

    fn notes_starts_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .map(|voice| voice.notes_starts_during(range))
            .kmerge_by(|(range1, _), (range2, _)| range1.start < range2.start)
    }

    fn notes_ends_during<G>(&self, range: &G) -> impl Iterator<Item = &(Range<Metric>, Note<Pitch>)>
    where
        G: RangeBounds<Metric>,
    {
        self.staves
            .iter()
            .map(|voice| voice.notes_ends_during(range))
            .kmerge_by(|(range1, _), (range2, _)| range1.start < range2.start)
    }

    fn note_rate(&self, time: u64, window: NonZeroU64) -> usize {
        self.staves.iter().map(|v| v.note_rate(time, window)).sum()
    }

    fn legato_index(&self, time: u64, window: NonZeroU64) -> f64 {
        self.staves
            .iter()
            .map(|v| v.legato_index(time, window))
            .sum::<f64>()
            .copysign(1.0) // prevent negative zero
    }
}

impl<Pitch, Control> RawMusic<Pitch, Control> {
    pub fn new(resolution: NonZero<Metric>) -> Self {
        Self {
            duration: Default::default(),
            staves: Default::default(),
            resolution,
        }
    }

    pub fn notes_by_start_with_pos(
        &self,
    ) -> impl Iterator<Item = (&(Range<Metric>, Note<Pitch>), [usize; 2])> {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, voice)| {
                voice
                    .notes_by_start_with_pos()
                    .map(move |(pair, voice_idx)| (pair, [staff_idx, voice_idx]))
            })
            .kmerge_by(|((range1, _), _), ((range2, _), _)| range1.start < range2.start)
    }

    pub fn notes_during_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (&(Range<Metric>, Note<Pitch>), [usize; 2])>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, staff)| {
                staff
                    .notes_during_with_pos(range)
                    .map(move |(pair, voice_idx)| (pair, [staff_idx, voice_idx]))
            })
            .kmerge_by(|((range1, _), _), ((range2, _), _)| range1.start < range2.start)
    }

    pub fn notes_overlaps_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (&(Range<Metric>, Note<Pitch>), [usize; 2])>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, staff)| {
                staff
                    .notes_overlaps_with_pos(range)
                    .map(move |(pair, voice_idx)| (pair, [staff_idx, voice_idx]))
            })
            .kmerge_by(|((range1, _), _), ((range2, _), _)| range1.start < range2.start)
    }

    pub fn note_instants_during_with_pos<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Endpoint<'a, Metric, Note<Pitch>>, [usize; 2])>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, voice)| {
                voice
                    .note_instants_during_with_pos(range)
                    .map(move |(ep, voice_idx)| (ep, [staff_idx, voice_idx]))
            })
            .kmerge_by(|(ep1, _), (ep2, _)| ep1.at < ep2.at)
    }

    pub fn note_instants_with_pos<'a>(
        &'a self,
    ) -> impl Iterator<Item = (Endpoint<'a, Metric, Note<Pitch>>, [usize; 2])> {
        self.note_instants_during_with_pos(&..)
    }

    pub fn controls_during_with_pos<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = ((Metric, &Control), [usize; 2])>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.staves
            .iter()
            .enumerate()
            .map(|(staff_idx, voice)| {
                voice
                    .controls_during_with_pos(range)
                    .map(move |(pair, voice_idx)| (pair, [staff_idx, voice_idx]))
            })
            .kmerge_by(|((beat1, _), _), ((beat2, _), _)| beat1 < beat2)
    }

    pub fn controls_with_pos(&self) -> impl Iterator<Item = ((Metric, &Control), [usize; 2])> {
        self.controls_during_with_pos(&..)
    }
}

impl<'a, Pitch, Control: 'a> ControlContainer<'a, Control> for RawMusic<Pitch, Control> {
    fn controls_during<G>(&self, range: &G) -> impl Iterator<Item = (Metric, &Control)>
    where
        G: RangeBounds<Metric> + Clone,
    {
        self.staves
            .iter()
            .map(|v| v.controls_during(range))
            .kmerge_by(|(beat1, _), (beat2, _)| beat1 < beat2)
    }
}
