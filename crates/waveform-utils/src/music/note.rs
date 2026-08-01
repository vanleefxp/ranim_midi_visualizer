use super::Velocity;
use derivative::Derivative;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Derivative)]
#[derivative(Default)]
pub struct Note<Pitch = i8> {
    /// The pitch of the note.
    pub pitch: Pitch,
    /// The velocity of the note. Should be a value between 0 and 1.
    #[derivative(Default(value = "0.75.try_into().unwrap()"))]
    pub velocity: Velocity,
    // /// A slight offset from the standard start and end positions of the note.
    // pub offset: [Metric; 2],
}

impl<Pitch> Note<Pitch> {
    pub fn new_off(pitch: Pitch) -> Self {
        Note {
            pitch,
            velocity: 0.0.try_into().unwrap(),
        }
    }
}
