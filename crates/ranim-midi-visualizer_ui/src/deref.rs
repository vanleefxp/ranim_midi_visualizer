use std::ops::{Deref, DerefMut};

use crate::{MidiVisualizerAppInner, MidiVisualizerAppInner2};

impl Deref for MidiVisualizerAppInner {
    type Target = MidiVisualizerAppInner2;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for MidiVisualizerAppInner {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
