mod control;
mod control_container;
#[allow(clippy::module_inception)]
mod music;
mod note;
mod note_container;
mod parsing;
mod raw_music;
mod staff;
mod time_map;
mod voice;

pub use control::*;
pub use control_container::*;
pub use music::*;
pub use note::*;
pub use note_container::*;
pub use parsing::*;
pub use raw_music::*;
pub use staff::*;
pub use time_map::*;
pub use voice::*;

use std::{num::NonZeroU64, ops::RangeBounds};
use typed_floats::tf64;

pub type Metric = i64;
pub type Velocity = tf64::PositiveFinite;
pub type FrameRate = NonZeroU64;
pub type Window = NonZeroU64; // time window in metric units
pub trait MetricRange = RangeBounds<Metric> + Clone;
