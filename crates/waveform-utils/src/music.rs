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

use std::{num::NonZero, ops::RangeBounds};
use typed_floats::tf64;

pub type Metric = u64;
pub type Velocity = tf64::PositiveFinite;
pub type Tempo = NonZero<Metric>;
pub trait MetricRange = RangeBounds<Metric> + Clone;
