use std::{fs, path::PathBuf, sync::Arc};

use gpui::*;
use gpui_util::ResultExt;
use ranim::OutputFormat;
use tracing::{error, info};
use waveform_utils::music::{Music, parse_midi};

use crate::{VisualizerApp, component::playback_control::actions::*, state::*};

actions!(
    file,
    [
        ShowOpenDialog,
        CloseFile,
        ExportVideo,
        LoadStyle,
        SaveStyle,
        RevertToDefault,
        ClearRecentFiles
    ]
);

#[derive(
    Clone, PartialEq, Debug, Action, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
pub struct OpenFile(pub PathBuf);

impl VisualizerApp {
    pub(super) fn action_play_pause(
        &mut self,
        _action: &PlayPause,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.playback_state.update(cx, |v, cx| {
            if v.is_playing() {
                v.pause(cx);
            } else {
                v.play(cx);
            }
        })
    }

    pub(super) fn action_jump_to_start(
        &mut self,
        _action: &JumpToStart,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.playback_state.update(cx, |v, cx| {
            v.jump_to_time(0, cx);
        })
    }

    pub(super) fn action_jump_to_end(
        &mut self,
        _action: &JumpToEnd,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.playback_state.update(cx, |v, cx| {
            v.jump_to_time(v.max_time(), cx);
        })
    }

    pub(super) fn action_toggle_looping(
        &mut self,
        _action: &ToggleLooping,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.playback_state.update(cx, |v, cx| {
            v.set_looping(!v.looping(), cx);
        })
    }

    pub(super) fn action_step_frame(
        &mut self,
        &StepFrame(n): &StepFrame,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.playback_state.update(cx, |v, cx| {
            v.step_frame(n, cx);
        });
    }

    pub fn set_music(&mut self, music: Music, cx: &mut App) {
        info!(
            "Music loaded: {} notes, {} time units, time resolution {}.",
            music.note_count(),
            music.duration(),
            music.time_resolution,
        );
        self.playback_state.update(cx, |v, cx| {
            v.pause(cx);
            v.jump_to_time(0, cx);
            v.set_max_time(music.duration(), cx);
            v.set_time_resolution(music.time_resolution, cx);
        });
        self.music = Arc::new(music);
    }

    pub fn open_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut App) {
        match fs::read(&path).inspect_err(|v| error!("{}", v)) {
            Ok(src) => match parse_midi!(src.as_slice()).inspect_err(|v| error!("{}", v)) {
                Ok(music) => {
                    self.file_state.set_opened_file(Some(path), cx);
                    self.set_music(music, cx);
                    window.refresh();
                }
                Err(err) => {
                    error!("{}", err);
                    drop(window.prompt(
                        PromptLevel::Critical,
                        "Invalid MIDI file",
                        Some(err.to_string().as_str()),
                        &[PromptButton::ok("OK")],
                        cx,
                    ));
                }
            },
            Err(err) => {
                error!("{}", err);
                drop(window.prompt(
                    PromptLevel::Critical,
                    "Failed to open file",
                    Some(err.to_string().as_str()),
                    &[PromptButton::ok("OK")],
                    cx,
                ));
            }
        }
    }

    pub(super) fn action_open_file(
        &mut self,
        OpenFile(path): &OpenFile,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_file(path.clone(), window, cx);
    }

    pub(super) fn action_close_file(
        &mut self,
        _action: &CloseFile,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_state.set_opened_file(None, cx);
        self.set_music(Music::default(), cx);
    }

    pub(super) fn action_clear_recent_files(
        &mut self,
        _action: &ClearRecentFiles,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_state.clear_recent_files(cx);
    }

    pub(super) fn action_show_open_dialog(
        &mut self,
        _action: &ShowOpenDialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ch = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                match ch.await.anyhow().and_then(|res| res) {
                    Ok(Some(paths)) => {
                        if let Some(path) = paths.into_iter().next() {
                            info!("Chosen: {}", path.display());
                            cx.update(|window, cx| {
                                this.update(cx, |v, cx| v.open_file(path, window, cx));
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
                            drop(window.prompt(
                                PromptLevel::Critical,
                                "Error",
                                Some(err.to_string().as_str()),
                                &[PromptButton::ok("OK")],
                                cx,
                            ));
                        })
                        .log_err();
                    }
                }
            })
            .detach();
    }

    pub(super) fn action_revert_to_default_style(
        &mut self,
        _action: &RevertToDefault,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let confirmation = window.prompt(
            PromptLevel::Warning,
            "Revert to default style",
            Some("Are you sure you want to revert to default style?"),
            &[PromptButton::ok("Yes"), PromptButton::cancel("No")],
            cx,
        );
        let this = cx.entity();
        cx.spawn(async move |_v, cx| {
            if let Some(idx) = confirmation.await.log_err() {
                if idx == 0 {
                    cx.update(|cx| {
                        this.update(cx, |v, cx| v.video_config = VideoConfigState::new(cx));
                    });
                } else {
                    info!("Operation cancelled. Nothing changed.");
                }
            }
        })
        .detach();
    }

    pub(super) fn action_start_export(
        &mut self,
        _action: &ExportVideo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = self
            .video_config
            .export_config
            .read(cx)
            .name
            .clone()
            .unwrap_or("video".to_string());
        let format = self.video_config.export_config.read(cx).format;

        let suggested_name = format!("{}.{}", filename, format);
        let ch = cx.prompt_for_new_path(std::path::Path::new("./"), Some(suggested_name.as_str()));
        let this = cx.entity();
        window
            .spawn(cx, async move |cx| {
                match ch.await.anyhow().and_then(|v| v) {
                    Ok(Some(path)) => {
                        info!("Export path chosen: {}", path.display());
                        cx.update(|_window, cx| {
                            this.update(cx, |v, cx| {
                                v.video_config.export_config.update(cx, |v, _cx| {
                                    v.dir = path.parent().map_or_else(
                                        || "./".to_string(),
                                        |v| v.display().to_string(),
                                    );
                                    let filename = path.file_name().map_or_else(
                                        || "video".to_string(),
                                        |v| PathBuf::from(v).display().to_string(),
                                    );
                                    match filename.rfind('.') {
                                        Some(idx) => {
                                            let (format, strip_ext) = match &filename[idx + 1..] {
                                                "mp4" => (OutputFormat::Mp4, true),
                                                "mov" => (OutputFormat::Mov, true),
                                                "webm" => (OutputFormat::Webm, true),
                                                "gif" => (OutputFormat::Gif, true),
                                                _ => (OutputFormat::Mp4, false),
                                            };
                                            v.format = format;
                                            if strip_ext {
                                                v.name = Some(filename[..idx].to_string());
                                            } else {
                                                v.name = Some(filename);
                                            }
                                        }
                                        None => {
                                            v.name = Some(filename);
                                        }
                                    }
                                });
                                v.start_export(cx);
                            });
                        })
                        .log_err();
                    }
                    Ok(None) => {
                        info!("No file selected.");
                    }
                    Err(err) => {
                        error!("{}", err);
                        cx.update(|window, cx| {
                            drop(window.prompt(
                                PromptLevel::Critical,
                                "Error",
                                Some(err.to_string().as_str()),
                                &[PromptButton::ok("OK")],
                                cx,
                            ));
                        })
                        .log_err();
                    }
                }
            })
            .detach();
    }
}
