use std::{collections::HashMap, num::NonZero};

use smallvec::SmallVec;
use thiserror::Error;
use tracing::{debug, error};
use typed_floats::tf64;

use crate::music::Metric;

use super::{FrameRate, Music, Note, Pedal, PedalControl, RawMusic, Staff};

fn get_beat_resolution(timing: midly::Timing) -> FrameRate {
    NonZero::try_from({
        use midly::Timing::*;
        match timing {
            Metrical(n) => n.as_int() as u64,
            Timecode(fps, subframes) => {
                let fps = fps.as_int();
                fps as u64 * subframes as u64
            }
        }
    })
    .expect("Beat resolution should be strictly positive.")
}

#[derive(Debug, Error)]
pub enum ParseMidiError {
    #[error("Failed to parse MIDI file.")]
    MidlyError(#[from] midly::Error),
    #[error("Time overflow: {0} + {1}.")]
    TimeOverflow(i64, i64),
}

pub fn parse_midi_raw(src: &[u8], time_resolution: FrameRate) -> Result<RawMusic, ParseMidiError> {
    let (header, track_iter) = midly::parse(src).inspect_err(|err| error!("{:?}", err))?;

    // [TODO] handle sequential multiple tracks
    // [FIXME] multi-track MIDI's tempo information is incorrect
    let beat_resolution = get_beat_resolution(header.timing);
    let mut music = RawMusic::new(time_resolution);

    for event_iter in track_iter {
        let event_iter = event_iter.inspect_err(|err| error!("{:?}", err))?;
        debug!("New track!");

        let mut cur_time = 0 as Metric;
        let mut time_units_per_beat = time_resolution;

        let mut staff = Staff::default();
        let mut note_states =
            HashMap::<(u8, i8), SmallVec<[(i64, Note<i8>); 1]>>::with_capacity(10);

        for event in event_iter {
            let event = event.inspect_err(|err| error!("{:?}", err))?;
            debug!("{:?}", event);

            // advance time
            let dt = (event.delta.as_int() as u128 * time_units_per_beat.get() as u128
                / beat_resolution.get() as u128) as Metric;
            cur_time = cur_time
                .checked_add(dt)
                .ok_or(ParseMidiError::TimeOverflow(cur_time, dt))?;

            use midly::TrackEventKind::*;
            match event.kind {
                Midi {
                    message,
                    channel: voice,
                } => {
                    use midly::MidiMessage::*;
                    let voice = voice.as_int();
                    match message {
                        NoteOn {
                            key: pitch,
                            vel: velocity,
                        } if velocity > 0 => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;

                            // regularize velocity to 0.0..=1.0 range
                            let velocity =
                                tf64::PositiveFinite::new(velocity.as_int() as f64 / 127.0)
                                    .unwrap();
                            // insert note into the note states table
                            let note = Note { pitch, velocity };
                            note_states
                                .entry((voice, pitch))
                                .or_default()
                                .push((cur_time, note));
                        }
                        NoteOn { key: pitch, .. } | NoteOff { key: pitch, .. } => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;

                            let should_remove =
                                if let Some(notes) = note_states.get_mut(&(voice, pitch)) {
                                    // SAFETY: `notes` must be non-empty at this point, so we can unwrap it safely.
                                    let (start_time, note) = notes.pop().unwrap();
                                    let voice = voice as usize;

                                    // insert note into the voice's interval tree
                                    if staff.voices.len() < voice + 1 {
                                        staff.voices.resize_with(voice + 1, Default::default);
                                    }
                                    staff.voices[voice].notes.insert(start_time..cur_time, note);
                                    notes.is_empty()
                                } else {
                                    false
                                };

                            // remove the `SmallVec` associated with the note state if it's empty
                            if should_remove {
                                note_states.remove(&(voice, pitch));
                            }
                        }
                        Controller { controller, value } => {
                            use Pedal::*;
                            let pedal = match controller.as_int() {
                                64 => Some(Sustain),
                                66 => Some(Sostenuto),
                                67 => Some(Soft),
                                _ => None,
                            };
                            if let Some(pedal) = pedal {
                                let depth =
                                    tf64::PositiveFinite::new(value.as_int() as f64 / 127.0)
                                        .unwrap();
                                let control = PedalControl { pedal, depth };
                                staff.controls.insert(cur_time, control);
                            }
                        }
                        _ => (),
                    }
                }
                Meta(message) => {
                    use midly::MetaMessage::*;
                    match message {
                        Tempo(tempo) => {
                            time_units_per_beat = ((tempo.as_int() as f64
                                * (time_resolution.get() as f64 / 1e6))
                                as u64)
                                .try_into()
                                .expect("Tempo should be strictly positive.");
                        }
                        EndOfTrack => {
                            music.duration = music.duration.max(cur_time);
                        }
                        _ => (),
                    }
                }
                _ => (),
            }
        }

        music.staves.push(staff);
    }

    Ok(music)
}

pub fn parse_midi(src: &[u8], time_resolution: FrameRate) -> Result<Music, ParseMidiError> {
    let (header, track_iter) = midly::parse(src).inspect_err(|err| error!("{:?}", err))?;

    let beat_resolution = get_beat_resolution(header.timing);
    let mut music = Music::new(beat_resolution, time_resolution);
    music.tempo.insert(0, time_resolution); // default BPM 60

    for event_iter in track_iter {
        let event_iter = event_iter.inspect_err(|err| error!("{:?}", err))?;
        debug!("New track!");

        let mut cur_tick = 0 as Metric;
        // let mut cur_time = 0u64;
        let mut time_units_per_beat;

        let mut staff = Staff::default();
        let mut note_states =
            HashMap::<(u8, i8), SmallVec<[(Metric, Note<i8>); 1]>>::with_capacity(10);

        for event in event_iter {
            let event = event.inspect_err(|err| error!("{:?}", err))?;
            debug!("{:?}", event);

            // advance tick
            let delta_tick = event.delta.as_int() as Metric;
            cur_tick = cur_tick
                .checked_add(delta_tick)
                .ok_or(ParseMidiError::TimeOverflow(cur_tick, delta_tick))?;

            use midly::TrackEventKind::*;
            match event.kind {
                Midi {
                    message,
                    channel: voice,
                } => {
                    use midly::MidiMessage::*;
                    let voice = voice.as_int();
                    match message {
                        // Start of note
                        NoteOn {
                            key: pitch,
                            vel: velocity,
                        } if velocity > 0 => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;

                            // regularize velocity to 0.0..=1.0 range
                            let velocity =
                                tf64::PositiveFinite::new(velocity.as_int() as f64 / 127.0)
                                    .unwrap();
                            // insert note into the note states table
                            let note = Note { pitch, velocity };
                            note_states
                                .entry((voice, pitch))
                                .or_default()
                                .push((cur_tick, note));
                        }
                        // End of note
                        // Sometimes expressed as a `NoteOn` event with velocity 0.
                        // Sometimes expressed as a `NoteOff` event with the same velocity as a previous `NoteOn` event.
                        // Both cases are handled here.
                        NoteOn { key: pitch, .. } | NoteOff { key: pitch, .. } => {
                            // regularize pitch to middle C as 0
                            let pitch = pitch.as_int() as i8 - 60;

                            let should_remove =
                                if let Some(notes) = note_states.get_mut(&(voice, pitch)) {
                                    // SAFETY: `notes` must be non-empty at this point, so we can unwrap it safely.
                                    let (start_tick, note) = notes.pop().unwrap();
                                    let voice = voice as usize;

                                    // insert note into the voice's interval tree
                                    if staff.voices.len() < voice + 1 {
                                        staff.voices.resize_with(voice + 1, Default::default);
                                    }
                                    staff.voices[voice].notes.insert(start_tick..cur_tick, note);
                                    notes.is_empty()
                                } else {
                                    false
                                };

                            // remove the `SmallVec` associated with the note state if it's empty
                            if should_remove {
                                note_states.remove(&(voice, pitch));
                            }
                        }
                        Controller { controller, value } => {
                            use Pedal::*;
                            let pedal = match controller.as_int() {
                                64 => Some(Sustain),
                                66 => Some(Sostenuto),
                                67 => Some(Soft),
                                _ => None,
                            };
                            if let Some(pedal) = pedal {
                                let depth =
                                    tf64::PositiveFinite::new(value.as_int() as f64 / 127.0)
                                        .unwrap();
                                let control = PedalControl { pedal, depth };
                                staff.controls.insert(cur_tick, control);
                            }
                        }
                        _ => (),
                    }
                }
                Meta(message) => {
                    use midly::MetaMessage::*;
                    match message {
                        Tempo(microsecs_per_beat) => {
                            time_units_per_beat = ((microsecs_per_beat.as_int() as f64
                                * (time_resolution.get() as f64 / 1e6))
                                as u64)
                                .try_into()
                                .expect("Tempo should be strictly positive.");
                            music.tempo.insert(cur_tick, time_units_per_beat);
                        }
                        // `EndOfTrack` determines the duration of the track
                        EndOfTrack => {
                            music.inner.duration = music.inner.duration.max(cur_tick);
                        }
                        _ => (),
                    }
                }
                _ => (),
            }
        }

        music.inner.staves.push(staff);
    }

    Ok(music)
}

pub macro parse_midi_raw {
    ($src: expr, $time_resolution: expr) => {
        $crate::music::parse_midi_raw($src, $time_resolution)
    },
    ($src: expr) => {
        $crate::music::parse_midi_raw($src, $crate::music::DEFAULT_TIME_RESOLUTION)
    }
}

pub macro parse_midi {
    ($src: expr, $time_resolution: expr) => {
        $crate::music::parse_midi($src, $time_resolution)
    },
    ($src: expr) => {
        $crate::music::parse_midi($src, $crate::music::DEFAULT_TIME_RESOLUTION)
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use tracing::info;
    use tracing_test::traced_test;

    use super::super::{ControlContainer as _, NoteContainer as _};
    use super::*;

    const DURATION_TOL: u64 = 1000000; // 1000000 nanoseconds = 1 millisecond

    #[traced_test]
    #[test]
    fn test_parse_midi_simple() {
        let src = include_bytes!("tests/little-star.mid").as_slice();
        let music = parse_midi_raw!(src);
        assert!(music.is_ok());
        let music = music.unwrap();
        assert!(music.notes_by_start().next().is_some());
        println!("{:?}", music);
    }

    macro test_parse_midi($name:literal) {
        let src = include_bytes!($name).as_slice();

        let music_raw = parse_midi_raw!(src).unwrap();
        let music = parse_midi!(src).unwrap();
        assert!(music_raw.notes_by_start().next().is_some());
        assert!(music_raw.controls().next().is_some());
        assert!(music.inner.notes_by_start().next().is_some());
        assert!(music.inner.controls().next().is_some());

        let duration_raw = music_raw.duration;
        let duration = music.duration();
        let duration_diff = duration.abs_diff(duration_raw);

        // Checking if all notes match
        for ((pos1, range1, note1), (pos2, range2, note2)) in music_raw
            .notes_by_start()
            .zip(music.as_mapped().notes_by_start())
        {
            assert_eq!(pos1, pos2);
            assert_eq!(note1, note2);

            debug!(
                "Note {:?} at {:?}. Time range of note using `RawMusic`: {:?}",
                note1, pos1, range1
            );
            debug!(
                "Note {:?} at {:?}. Time range of note using `Music`: {:?}",
                note2, pos2, range2
            );

            let Range {
                start: start1,
                end: end1,
            } = range1;
            let Range {
                start: start2,
                end: end2,
            } = range2;
            let start_diff = start1.abs_diff(start2);
            let end_diff = end1.abs_diff(end2);

            debug!("Time difference: start {}, end {}", start_diff, end_diff);

            assert!(start_diff < DURATION_TOL);
            assert!(end_diff < DURATION_TOL);
        }

        // an insignificant difference might be noticed
        debug!("Duration using `RawMusic`: {}", duration_raw);
        debug!("Duration using `Music`: {}", duration);
        debug!("Duration difference: {}", duration_diff);
        assert!(duration_diff < DURATION_TOL); // 1000000 nanoseconds = 1 millisecond
    }

    #[traced_test]
    #[test]
    fn test_time_map_cache() {
        let src = include_bytes!("tests/song_2.mid").as_slice();
        let music = parse_midi!(src).unwrap();

        // dead lock should not happen

        info!("Accessing time map for the first time.");
        let _ = music.time_map();
        info!("Access completed.");

        info!("Accessing time map for the second time.");
        let _ = music.time_map();
        info!("Access completed.");
    }

    #[traced_test]
    #[test]
    fn test_parse_midi_complicated_1() {
        test_parse_midi!("tests/song_2.mid");
    }

    #[traced_test]
    #[test]
    fn test_parse_midi_complicated_2() {
        test_parse_midi!("tests/the-egg-of-our-hearts.mid");
    }

    #[traced_test]
    #[test]
    fn test_parse_not_midi() {
        let src = include_bytes!("tests/not-midi.txt").as_slice();
        let music = parse_midi_raw!(src);
        assert!(music.is_err());
    }

    #[traced_test]
    #[test]
    fn test_parse_midi_corrupted() {
        let mut src = include_bytes!("tests/little-star.mid").to_vec();
        // overwrite some bytes to corrupt the file
        src.iter_mut()
            .skip(4)
            .step_by(2)
            .take(10)
            .for_each(|v| *v = 255);
        let music = parse_midi_raw!(src.as_slice());
        assert!(music.is_err());
    }
}
