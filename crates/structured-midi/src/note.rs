use super::loc::{Channel, MultiTrackLoc};

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct GenericMidiNote<L> {
    pub loc: L,
    pub key: u8,
    pub vel: u8,
}

pub type MidiNote = GenericMidiNote<Channel>;
pub type MultiTrackMidiNote = GenericMidiNote<MultiTrackLoc>;

impl From<MultiTrackMidiNote> for MidiNote {
    fn from(note: MultiTrackMidiNote) -> Self {
        MidiNote {
            loc: note.loc.channel,
            key: note.key,
            vel: note.vel,
        }
    }
}
