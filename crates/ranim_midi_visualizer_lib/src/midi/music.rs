use std::{collections::{BTreeSet, HashMap}, ops::Range};

use crate::midi::{
    MidiNoteInstant, MultiTrackLoc, MultiTrackMidiNote, MultiTrackMidiNoteInstant, MultiTrackPedalInstant, PedalInstant, PedalType, note::MidiNote, track::MidiTrack
};
use derive_more::{Deref, DerefMut, Index, IndexMut, IntoIterator};
use interavl::IntervalTree;
use itertools::Itertools as _;

use super::track::GenericMidiTrack;

#[derive(Debug, Default, Clone, IntoIterator, Deref, DerefMut, Index, IndexMut)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct MidiMusic {
    tracks: Vec<MidiTrack>,
}

impl TryFrom<&[u8]> for MidiMusic {
    type Error = midly::Error;
    fn try_from(src: &[u8]) -> Result<Self, Self::Error> {
        let (header, track_iter) = midly::parse(src)?;

        let ticks_per_beat = {
            use midly::Timing::*;
            match header.timing {
                Metrical(n) => n.as_int(),
                Timecode(fps, subframes) => {
                    let fps = fps.as_int();
                    fps as u16 * subframes as u16
                }
            }
        };
        let mut nanosec_per_beat = 1_000_000_000u64;
        let mut global_time = 0u64;
        let mut tracks = Vec::new();

        for event_iter in track_iter {
            let event_iter = event_iter?;

            let mut notes = IntervalTree::default();
            let mut pedals = BTreeSet::new();
            let mut note_states = HashMap::<(u8, u8), (u64, MidiNote)>::with_capacity(10);

            for event in event_iter {
                let event = event?;

                // update global time
                let dt = event.delta.as_int() as u64 * nanosec_per_beat / ticks_per_beat as u64;
                global_time += dt;

                use midly::TrackEventKind::*;
                match event.kind {
                    Midi { message, channel } => {
                        use midly::MidiMessage::*;
                        let channel = channel.as_int();
                        match message {
                            NoteOn { key, vel } => {
                                let key = key.as_int();
                                let vel = vel.as_int();
                                if let Some((start_time, note)) =
                                    note_states.get_mut(&(channel, key))
                                {
                                    // overlapping note
                                    // cut off current note and restart it
                                    notes.insert(*start_time..global_time, *note);
                                    *note = MidiNote {
                                        loc: channel,
                                        key,
                                        vel,
                                    };
                                } else {
                                    note_states.insert(
                                        (channel, key),
                                        (
                                            global_time,
                                            MidiNote {
                                                loc: channel,
                                                key,
                                                vel,
                                            },
                                        ),
                                    );
                                }
                            }
                            NoteOff { key, .. } => {
                                let key = key.as_int();
                                if let Some((start_time, note)) =
                                    note_states.remove(&(channel, key))
                                {
                                    // stop of note
                                    notes.insert(start_time..global_time, note);
                                }
                            }
                            Controller { controller, value } => {
                                use PedalType::*;
                                let pedal_type = match controller.as_int() {
                                    64 => Some(Sustain),
                                    66 => Some(Sostenuto),
                                    67 => Some(Soft),
                                    _ => None,
                                };
                                if let Some(pedal_type) = pedal_type {
                                    let instant = PedalInstant {
                                        time: global_time,
                                        loc: channel,
                                        pedal_type,
                                        value: value.as_int(),
                                    };
                                    pedals.insert(instant);
                                }
                            }
                            _ => {}
                        }
                    }
                    Meta(meta_msg) => {
                        use midly::MetaMessage::*;
                        match meta_msg {
                            Tempo(value) => {
                                // tempo change
                                nanosec_per_beat = value.as_int() as u64 * 1000;
                            }
                            _ => {}
                        }
                    }
                    _ => {}
                }
            }
            tracks.push(GenericMidiTrack { notes, pedals });
        }
        Ok(Self { tracks })
    }
}

impl MidiMusic {
    pub fn instants(&self) -> impl Iterator<Item = MultiTrackMidiNoteInstant> {
        self.tracks.iter()
        .enumerate()
        .map(|(idx, track)| track.instants().into_iter().map(move |instant| (idx, instant)))
        .kmerge_by(|(_, a), (_, b)| a.time < b.time)
        .map(|(idx, instant)| {
            let MidiNoteInstant {
                time,
                loc: channel,
                key,
                vel,
            } = instant;
            let loc = MultiTrackLoc { track: idx, channel };
            MultiTrackMidiNoteInstant { time, loc, key, vel }
        })
    }

    pub fn notes(&self) -> impl Iterator<Item = (Range<u64>, MultiTrackMidiNote)> {
        self.tracks.iter()
        .enumerate()
        .map(|(idx, track)| track.notes.iter().map(move |v| (idx, v)))
        .kmerge_by(|(_, a), (_, b)| a.range().start < b.range().start)
        .map(|(idx, v)| {
            let range = v.range().clone();
            let &MidiNote { loc: channel, key, vel } = v.value();
            let loc = MultiTrackLoc { track: idx, channel };
            let note = MultiTrackMidiNote { loc, key, vel };
            (range, note)
        })
    }

    pub fn pedals(&self) -> impl Iterator<Item = MultiTrackPedalInstant> {
        self.tracks.iter()
        .enumerate()
        .map(|(idx, track)| track.pedals.iter().map(move |instant| (idx, instant)))
        .kmerge_by(|(_, a), (_, b)| a.time < b.time)
        .map(|(idx, instant)| {
            let &PedalInstant {
                time,
                loc: channel,
                pedal_type,
                value,
            } = instant;
            let loc = MultiTrackLoc { track: idx, channel };
            MultiTrackPedalInstant {
                time,
                loc,
                pedal_type,
                value,
            }
        })
    }

    pub fn snap_pos(&self, time: u64) -> u64 {
        self.iter()
            .map(|track| track.snap_pos(time))
            .min_by_key(|&snap_pos| snap_pos.abs_diff(time))
            .unwrap_or_default()
    }

    pub fn next_snap_pos(&self, time: u64) -> Option<u64> {
        self.iter()
            .map(|track| track.next_snap_pos(time))
            .flatten()
            .min()
    }

    pub fn prev_snap_pos(&self, time: u64) -> Option<u64> {
        self.iter()
            .map(|track| track.prev_snap_pos(time))
            .flatten()
            .max()
    }

    pub fn duration(&self) -> u64 {
        self.iter()
            .map(|track| track.duration())
            .max()
            .unwrap_or_default()
    }

    pub fn nps(&self, time: u64, window: u64) -> f64 {
        self.tracks.iter().map(|track| track.nps(time, window)).sum()
    }

    pub fn legato_index(&self, time: u64, window: u64) -> f64 {
        self.tracks.iter().map(|track| track.legato_index(time, window)).sum()
    }
}

pub type MergedMidiMusic = GenericMidiTrack<MultiTrackLoc>;

impl From<MidiMusic> for MergedMidiMusic {
    fn from(value: MidiMusic) -> Self {
        let mut notes = IntervalTree::new();
        let mut pedals = BTreeSet::new();
        for (track_idx, track) in value.tracks.iter().enumerate() {
            notes.extend(track.notes.iter().map(|node| {
                let note = node.value();
                let interval = node.interval().clone();
                let loc = MultiTrackLoc {
                    track: track_idx,
                    channel: note.loc,
                };
                (
                    interval,
                    MultiTrackMidiNote {
                        loc,
                        key: note.key,
                        vel: note.vel,
                    },
                )
            }));
            pedals.extend(track.pedals.iter().map(|instant| {
                let &PedalInstant {
                    time,
                    loc,
                    pedal_type,
                    value,
                } = instant;
                let loc = MultiTrackLoc {
                    track: track_idx,
                    channel: loc,
                };
                MultiTrackPedalInstant {
                    time,
                    loc,
                    pedal_type,
                    value,
                }
            }));
        }
        MergedMidiMusic { notes, pedals }
    }
}

impl From<MergedMidiMusic> for MidiMusic {
    fn from(value: MergedMidiMusic) -> Self {
        let mut tracks = Vec::with_capacity(10);
        for entry in value.notes.iter() {
            let note = entry.value();
            let track_idx = note.loc.track;
            if track_idx >= tracks.len() {
                tracks.resize_with(track_idx + 1, MidiTrack::default);
            }
            let track = &mut tracks[track_idx];
            track.notes.insert(entry.range().clone(), (*note).into());
        }
        for instant in value.pedals.iter() {
            let &MultiTrackPedalInstant {
                time,
                loc,
                pedal_type,
                value,
            } = instant;
            let track_idx = loc.track;
            if track_idx >= tracks.len() {
                tracks.resize_with(track_idx + 1, MidiTrack::default);
            }
            let track = &mut tracks[track_idx];
            track.pedals.insert(PedalInstant {
                time,
                loc: loc.channel,
                pedal_type,
                value,
            });
        }
        Self { tracks }
    }
}
