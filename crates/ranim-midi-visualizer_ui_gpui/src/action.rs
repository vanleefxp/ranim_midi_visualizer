use std::{fs, path::PathBuf};

use gpui::*;
use gpui_util::ResultExt;
use tracing::{error, info};
use waveform_utils::music::parse_midi;

use crate::state::*;

actions!(
    file,
    [
        ShowOpenDialog,
        ExportVideo,
        LoadStyle,
        SaveStyle,
        RevertToDefault,
        ClearRecentFiles
    ]
);
actions!(
    playback,
    [
        PlayPause,
        JumpToStart,
        JumpToEnd,
        StepForward,
        StepBack,
        ToggleLooping,
    ]
);

#[derive(
    Clone, PartialEq, Debug, Action, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct OpenFile(pub PathBuf);

pub mod base {
    use std::path::PathBuf;

    use waveform_utils::music::Metric;

    use super::*;

    pub fn open_file(window: &mut Window, cx: &mut App, path: PathBuf) {
        match fs::read(&path).inspect_err(|v| error!("{}", v)) {
            Ok(src) => match parse_midi!(src.as_slice()).inspect_err(|v| error!("{}", v)) {
                Ok(music) => {
                    file::add_recent_file(cx, path.clone());
                    file::set_opened_file(cx, path);
                    music_data::set_music(cx, music);
                    window.refresh();
                }
                Err(err) => {
                    error!("{}", err);
                    let _ = window.prompt(
                        PromptLevel::Critical,
                        "Invalid MIDI file",
                        Some(err.to_string().as_str()),
                        &[PromptButton::ok("OK")],
                        cx,
                    );
                }
            },
            Err(err) => {
                error!("{}", err);
                let _ = window.prompt(
                    PromptLevel::Critical,
                    "Failed to open file",
                    Some(err.to_string().as_str()),
                    &[PromptButton::ok("OK")],
                    cx,
                );
            }
        }
    }

    pub fn show_open_dialog(window: &mut Window, cx: &mut App) {
        let ch = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        window
            .spawn(cx, async move |cx| {
                match ch.await.anyhow().and_then(|res| res) {
                    Ok(Some(paths)) => {
                        if let Some(path) = paths.into_iter().next() {
                            info!("Chosen: {}", path.display());
                            cx.update(|window, cx| {
                                open_file(window, cx, path);
                            })
                            .log_err();
                        } else {
                            info!("No file selected.");
                        }
                    }
                    Ok(None) => {
                        info!("No file selected.");
                    }
                    Err(err) => {
                        error!("{}", err);
                        cx.update(|window, cx| {
                            let _ = window.prompt(
                                PromptLevel::Critical,
                                "Error",
                                Some(err.to_string().as_str()),
                                &[PromptButton::ok("OK")],
                                cx,
                            );
                        })
                        .log_err();
                    }
                }
            })
            .detach();
    }

    pub fn load_style(_window: &mut Window, _cx: &mut App) {
        let opened_file = rfd::FileDialog::new()
            .add_filter("Style config file", &["toml"])
            .add_filter("All files", &["*"])
            .pick_file();
        if let Some(path) = &opened_file {
            info!("Opened: {}", path.display());
        } else {
            info!("No file selected.");
        }
    }

    pub fn save_style(_window: &mut Window, _cx: &mut App) {
        let saved_file = rfd::FileDialog::new()
            .add_filter("Style config file", &["toml"])
            .add_filter("All files", &["*"])
            .save_file();
        if let Some(path) = &saved_file {
            info!("Saved: {}", path.display());
        } else {
            info!("No file selected.");
        }
    }

    pub fn revert_to_default_style(window: &mut Window, cx: &mut App) {
        let confirmation = window.prompt(
            PromptLevel::Warning,
            "Revert to default style",
            Some("Are you sure you want to revert to default style?"),
            &[PromptButton::ok("Yes"), PromptButton::cancel("No")],
            cx,
        );
        cx.spawn(async |cx| {
            if let Some(idx) = confirmation.await.log_err() {
                if idx == 0 {
                    cx.update(|cx| {
                        video_config::revert_to_default(cx);
                    });
                } else {
                    info!("Operation cancelled. Nothing changed.");
                }
            }
        })
        .detach();
    }

    fn playback_loop(window: &mut Window, _cx: &mut App) {
        window.on_next_frame(|window, cx| {
            if playback::is_playing(cx) {
                playback::update_playback(cx);
                playback_loop(window, cx);
            }
        });
        window.refresh();
    }

    pub fn play(window: &mut Window, cx: &mut App) {
        playback::play(cx);
        window.refresh();
        playback_loop(window, cx);
    }

    pub fn pause(window: &mut Window, cx: &mut App) {
        playback::pause(cx);
        window.refresh();
    }

    pub fn jump_to_time(window: &mut Window, cx: &mut App, time: Metric) {
        playback::jump_to_time(cx, time);
        window.refresh();
    }

    pub fn toggle_looping(window: &mut Window, cx: &mut App) {
        cx.update_global::<PlaybackState, _>(|g, _cx| {
            g.looping = !g.looping;
            if g.looping {
                info!("Looping on.");
            } else {
                info!("Looping off.")
            }
        });
        window.refresh();
    }

    pub fn step_frame(window: &mut Window, cx: &mut App, n_frames: isize) {
        playback::step_frame(cx, n_frames);
        window.refresh();
    }
}

pub fn show_open_dialog(_action: &ShowOpenDialog, window: &mut Window, cx: &mut App) {
    base::show_open_dialog(window, cx);
}

pub fn open_file(OpenFile(path): &OpenFile, window: &mut Window, cx: &mut App) {
    base::open_file(window, cx, path.clone());
}

pub fn clear_recent_files(_action: &ClearRecentFiles, _window: &mut Window, cx: &mut App) {
    file::clear_recent_files(cx);
}

pub fn load_style(_action: &LoadStyle, window: &mut Window, cx: &mut App) {
    base::load_style(window, cx);
}

pub fn save_style(_action: &SaveStyle, window: &mut Window, cx: &mut App) {
    base::save_style(window, cx);
}

pub fn revert_to_default_style(_action: &RevertToDefault, window: &mut Window, cx: &mut App) {
    base::revert_to_default_style(window, cx);
}

pub fn play_pause(_action: &PlayPause, window: &mut Window, cx: &mut App) {
    if playback::is_playing(cx) {
        base::pause(window, cx);
    } else {
        base::play(window, cx);
    }
}

pub fn jump_to_start(_action: &JumpToStart, window: &mut Window, cx: &mut App) {
    base::jump_to_time(window, cx, 0);
}

pub fn jump_to_end(_action: &JumpToEnd, window: &mut Window, cx: &mut App) {
    base::jump_to_time(window, cx, playback::max_time(cx));
}

pub fn step_forward(_action: &StepForward, window: &mut Window, cx: &mut App) {
    base::step_frame(window, cx, 1);
}

pub fn step_back(_action: &StepBack, window: &mut Window, cx: &mut App) {
    base::step_frame(window, cx, -1);
}

pub fn toggle_looping(_action: &ToggleLooping, window: &mut Window, cx: &mut App) {
    base::toggle_looping(window, cx);
}
