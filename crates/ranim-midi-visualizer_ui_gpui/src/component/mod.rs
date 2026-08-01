pub mod playback_control;
pub mod preview_area;

pub use playback_control::PlaybackControl;
pub use preview_area::PreviewArea;

use gpui::App;

pub fn init(cx: &mut App) {
    playback_control::init(cx);
}
