use typed_floats::tf64;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Pedal {
    Soft = 0,
    Sostenuto = 1,
    #[default]
    Sustain = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PedalControl {
    /// Which pedal is being controlled.
    pub pedal: Pedal,
    /// The depth of the pedal being held. Should be a value between 0 and 1.
    pub depth: tf64::PositiveFinite,
}
