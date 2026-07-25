mod simple;

use std::any::Any;

use crate::{freq::ToFrequency, music::PedalControl};
pub use simple::*;
use typed_floats::tf64;

type SynthSample = f32;

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MusicDirective<Pitch: ToFrequency = i8, Control = PedalControl> {
    Note(NoteDirective<Pitch>),
    Control(Control),
    PlayPause(bool),
    Stop,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteDirective<Pitch: ToFrequency> {
    pub pitch: Pitch,
    pub volume: tf64::PositiveFinite,
}

impl<Pitch: ToFrequency> NoteDirective<Pitch> {
    pub fn new_off(pitch: Pitch) -> Self {
        Self {
            pitch,
            volume: tf64::PositiveFinite::default(),
        }
    }
}

impl<Pitch: ToFrequency, Control> From<NoteDirective<Pitch>> for MusicDirective<Pitch, Control> {
    fn from(value: NoteDirective<Pitch>) -> Self {
        MusicDirective::Note(value)
    }
}

/// A trait for a synthesizer that can play notes and write the resulting sound to a buffer.
pub trait Synth<Directive = MusicDirective, Sample = f32>: Send + Sync + Any {
    /// Send directive to the synthesizer.
    fn directive(&mut self, directive: Directive);
    /// Write the currently generated sound to the audio buffer.
    fn write_to_buffer(&mut self, buffer: &mut [Sample]);
}

////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
////////////////////////////////////////////////////////////////////////////////
