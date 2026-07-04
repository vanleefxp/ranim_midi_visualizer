use std::{
    num::NonZero,
    sync::{MappedRwLockReadGuard, RwLock, RwLockReadGuard},
};

use derivative::Derivative;
use ranim_midi_visualizer_math::func::{LadderFn, SegmentedLinearFn};
use tracing::info;

use super::{
    DEFAULT_BEAT_RESOLUTION, FrameRate, MappedControlContainer, MappedNoteContainer,
    MappedNoteControlContainer, Metric, RawMusic, TimeMap, control::PedalControl,
    time_map::generate_time_map,
};

/// Default number of time divisions per second.
pub const DEFAULT_TIME_RESOLUTION: FrameRate = NonZero::new(1_000_000_000).unwrap(); // nanosecond-level resolution

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Music<Pitch = i8, Control = PedalControl> {
    pub(crate) inner: RawMusic<Pitch, Control>,
    /// Number of divisions per second
    pub time_resolution: FrameRate,
    /// The tempo curve of the music. Maps beats to tempo values in seconds per beat.
    pub tempo: LadderFn<Metric, FrameRate>,

    // cache fields
    /// A mapping from metric beats to time in seconds.
    #[derivative(Debug = "ignore")]
    pub(crate) time_map: RwLock<Option<SegmentedLinearFn<Metric, Metric>>>,
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

impl<Pitch, Control> AsRef<RawMusic<Pitch, Control>> for Music<Pitch, Control> {
    fn as_ref(&self) -> &RawMusic<Pitch, Control> {
        &self.inner
    }
}

impl<Pitch, Control> AsMut<RawMusic<Pitch, Control>> for Music<Pitch, Control> {
    fn as_mut(&mut self) -> &mut RawMusic<Pitch, Control> {
        &mut self.inner
    }
}

pub type TimeMapRef<'a> = MappedRwLockReadGuard<'a, SegmentedLinearFn<Metric, Metric>>;
pub type MappedMusic<'a, Pitch, Control, TimeMapRef = &'a TimeMap> =
    MappedNoteControlContainer<'a, RawMusic<Pitch, Control>, TimeMapRef>;

impl<Pitch, Control> Music<Pitch, Control> {
    pub fn from_beat_resolution(beat_resolution: FrameRate) -> Self {
        Self::new(beat_resolution, DEFAULT_TIME_RESOLUTION)
    }

    pub fn new(beat_resolution: FrameRate, time_resolution: FrameRate) -> Self {
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
                    self.inner.resolution,
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
