use std::{
    num::NonZero,
    ops::{Deref, DerefMut},
    sync::{MappedRwLockReadGuard, RwLock, RwLockReadGuard},
};

use derivative::Derivative;
use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};
use tracing::info;

use crate::music::{
    ControlContainer, DEFAULT_BEAT_RESOLUTION, MappedControlContainer, MappedNoteContainer, Metric,
    NoteContainer, RawMusic, Tempo, TimeMap, control::PedalControl, time_map::generate_time_map,
};

/// Default number of time divisions per second.
// SAFETY: non zero literal
pub const DEFAULT_TIME_RESOLUTION: NonZero<Metric> = NonZero::new(1_000_000_000).unwrap(); // nanosecond-level resolution

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Music<Pitch = i8, Control = PedalControl> {
    pub(crate) inner: RawMusic<Pitch, Control>,
    /// Number of divisions per second
    pub time_resolution: NonZero<Metric>,
    /// The tempo curve of the music. Maps beats to tempo values in seconds per beat.
    pub tempo: LadderFn<Metric, Tempo>,

    // cache fields
    /// A mapping from metric beats to time in seconds.
    #[derivative(Debug = "ignore")]
    pub(crate) time_map: RwLock<Option<SegmentedLinearFn<Metric, Metric>>>,
}

pub struct MappedMusic<'a, Pitch, Control, TimeMapRef: Deref<Target = TimeMap> + 'a> {
    pub notes: MappedNoteContainer<'a, RawMusic<Pitch, Control>, TimeMapRef>,
    pub controls: MappedControlContainer<'a, RawMusic<Pitch, Control>, TimeMapRef>,
}

impl<Pitch, Control, TimeMapRef: Deref<Target = TimeMap>> NoteContainer
    for MappedMusic<'_, Pitch, Control, TimeMapRef>
{
    type Pitch = Pitch;
    type Pos = [usize; 2];

    fn notes_by_start(
        &self,
    ) -> impl Iterator<
        Item = (
            Self::Pos,
            std::ops::Range<Metric>,
            &super::Note<Self::Pitch>,
        ),
    > {
        self.notes.notes_by_start()
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
        self.notes.notes_during(range)
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
        self.notes.notes_overlaps(range)
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
        self.notes.notes_start_during(range)
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
        self.notes.notes_end_during(range)
    }

    fn note_instants_during<'a, G>(
        &'a self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, super::NoteInstant<'a, Self::Pitch>)>
    where
        G: std::ops::RangeBounds<Metric>,
    {
        self.notes.note_instants_during(range)
    }
}

impl<'a, Pitch, Control, TimeMapRef: Deref<Target = TimeMap> + 'a> ControlContainer
    for MappedMusic<'a, Pitch, Control, TimeMapRef>
{
    type Control = Control;
    type Pos = usize;

    fn controls_during<G>(
        &self,
        range: &G,
    ) -> impl Iterator<Item = (Self::Pos, Metric, &Self::Control)>
    where
        G: std::ops::RangeBounds<Metric>,
    {
        self.controls.controls_during(range)
    }
}

impl<Pitch, Control> Clone for Music<Pitch, Control>
where
    Pitch: Clone,
    Control: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            time_resolution: self.time_resolution,
            tempo: self.tempo.clone(),
            // do not clone cache fields
            time_map: RwLock::new(None),
        }
    }
}

impl<Pitch, Control> Default for Music<Pitch, Control> {
    fn default() -> Self {
        Self::new(DEFAULT_BEAT_RESOLUTION, DEFAULT_TIME_RESOLUTION)
    }
}

impl<Pitch, Control> Deref for Music<Pitch, Control> {
    type Target = RawMusic<Pitch, Control>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<Pitch, Control> DerefMut for Music<Pitch, Control> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

type TimeMapRef<'a> = MappedRwLockReadGuard<'a, SegmentedLinearFn<Metric, Metric>>;

impl<Pitch, Control> Music<Pitch, Control> {
    pub fn from_beat_resolution(beat_resolution: NonZero<Metric>) -> Self {
        Self::new(beat_resolution, DEFAULT_TIME_RESOLUTION)
    }

    pub fn new(beat_resolution: NonZero<Metric>, time_resolution: NonZero<Metric>) -> Self {
        Self {
            inner: RawMusic::new(beat_resolution),
            time_resolution,
            tempo: LadderFn::from_iter([(0, time_resolution)]),
            time_map: RwLock::new(None),
        }
    }

    pub fn time_map(&self) -> TimeMapRef<'_> {
        {
            let read_guard = self.time_map.read().unwrap();
            if read_guard.is_some() {
                info!("Time map already cached.");
                return RwLockReadGuard::map(
                    read_guard,
                    |time_map: &Option<SegmentedLinearFn<Metric, Metric>>| {
                        time_map.as_ref().expect("Value should already be present.")
                    },
                );
            }
        }
        {
            let mut write_guard = self.time_map.write().unwrap();
            if write_guard.is_none() {
                info!("No cached time map, generating one now.");
                *write_guard = Some(generate_time_map(
                    &self.tempo,
                    self.inner.duration,
                    self.resolution,
                    self.time_resolution,
                    false,
                ));
            }
        }
        {
            let read_guard = self.time_map.read().unwrap();
            RwLockReadGuard::map(
                read_guard,
                |time_map: &Option<SegmentedLinearFn<Metric, Metric>>| {
                    time_map.as_ref().expect("Value should already be present.")
                },
            )
        }
    }

    pub fn duration(&self) -> Metric {
        self.time_map().eval(&self.inner.duration, true)
    }

    pub fn as_mapped(&self) -> MappedMusic<'_, Pitch, Control, TimeMapRef<'_>> {
        let notes = MappedNoteContainer {
            orig: &self.inner,
            time_map: self.time_map(),
        };
        let controls = MappedControlContainer {
            orig: &self.inner,
            time_map: self.time_map(),
        };
        MappedMusic { notes, controls }
    }
}

// [FIXME] result not consistent with `parse_midi_raw`
impl<Pitch, Control> From<Music<Pitch, Control>> for RawMusic<Pitch, Control> {
    // Convert music with metric-based timing to linear timing, erasing all metric information.
    fn from(value: Music<Pitch, Control>) -> Self {
        let Music {
            inner:
                RawMusic {
                    duration,
                    resolution: beat_resolution,
                    staves,
                },
            time_resolution,
            tempo,
            time_map,
        } = value;
        let time_map = match time_map.into_inner().unwrap() {
            Some(v) => v,
            None => generate_time_map(&tempo, duration, beat_resolution, time_resolution, false),
        };
        let staves = staves.into_iter().map(|v| v.remap(&time_map)).collect();
        let duration = time_map.eval(&duration, true);
        Self {
            duration,
            resolution: time_resolution,
            staves,
        }
    }
}
