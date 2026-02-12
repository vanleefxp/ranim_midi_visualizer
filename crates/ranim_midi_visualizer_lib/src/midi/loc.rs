pub type Channel = u8;

#[derive(Debug, PartialOrd, Ord, PartialEq, Eq, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct MultiTrackLoc {
    pub track: usize,
    pub channel: Channel,
}
