use int_enum::IntEnum;

use super::loc::{Channel, MultiTrackLoc};
use std::cmp::Ordering;

/// Start or end of a MIDI note
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[non_exhaustive]
pub struct GenericMidiNoteInstant<L> {
    pub time: u64,
    pub loc: L,
    pub key: u8,
    pub(crate) vel: u8,
}

/// Midi note instant for single MIDI track, where `loc` represents the note's channel
pub type MidiNoteInstant = GenericMidiNoteInstant<Channel>;
pub type MultiTrackMidiNoteInstant = GenericMidiNoteInstant<MultiTrackLoc>;

impl<L> GenericMidiNoteInstant<L> {
    pub fn new_start(time: u64, loc: L, key: u8, vel: u8) -> Self {
        assert!(vel & 0x80 == 0);
        GenericMidiNoteInstant {
            time,
            loc,
            key,
            vel,
        }
    }

    pub fn new_end(time: u64, loc: L, key: u8) -> Self {
        GenericMidiNoteInstant {
            time,
            loc,
            key,
            vel: 255,
        }
    }

    #[inline(always)]
    pub fn is_start(&self) -> bool {
        self.vel & 0x80 == 0
    }

    #[inline(always)]
    pub fn is_end(&self) -> bool {
        self.vel & 0x80 != 0
    }

    pub fn time(&self) -> u64 {
        self.time
    }

    pub fn key(&self) -> u8 {
        self.key
    }

    pub fn vel(&self) -> u8 {
        assert!(self.is_start(), "Velocity is only valid for start events");
        self.vel
    }

    pub fn loc(&self) -> L
    where
        L: Copy,
    {
        self.loc
    }
}

impl<L> PartialOrd for GenericMidiNoteInstant<L>
where
    L: Ord,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<L> Ord for GenericMidiNoteInstant<L>
where
    L: Ord,
{
    fn cmp(&self, other: &Self) -> Ordering {
        use Ordering::*;
        self.time.cmp(&other.time).then_with(|| {
            if !self.is_start() {
                if !other.is_start() {
                    self.loc
                        .cmp(&other.loc)
                        .then_with(|| self.key.cmp(&other.key))
                } else {
                    Greater
                }
            } else if !other.is_start() {
                Less
            } else {
                self.loc
                    .cmp(&other.loc)
                    .then_with(|| self.key.cmp(&other.key))
                    .then_with(|| self.vel.cmp(&other.vel))
            }
        })
    }
}

impl MidiNoteInstant {
    pub fn at_track(self, track: usize) -> MultiTrackMidiNoteInstant {
        let MidiNoteInstant {
            time,
            loc: channel,
            key,
            vel,
        } = self;
        MultiTrackMidiNoteInstant {
            time,
            loc: MultiTrackLoc { track, channel },
            key,
            vel,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy, PartialOrd, Ord, IntEnum)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub enum PedalType {
    Soft = 0,
    Sostenuto = 1,
    #[default]
    Sustain = 2,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, PartialOrd, Ord)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct GenericPedalInstant<L> {
    pub time: u64,
    pub loc: L,
    pub pedal_type: PedalType,
    pub value: u8,
}

pub type PedalInstant = GenericPedalInstant<Channel>;
pub type MultiTrackPedalInstant = GenericPedalInstant<MultiTrackLoc>;
