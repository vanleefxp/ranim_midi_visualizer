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

use std::num::NonZeroU64;
use typed_floats::tf64;

pub(crate) type Metric = u64;
pub(crate) type Velocity = tf64::PositiveFinite;
pub(crate) type Tempo = NonZeroU64;
