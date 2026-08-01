#![feature(
    decl_macro,
    fn_traits,
    unboxed_closures,
    thread_id_value,
    const_trait_impl,
    const_convert
)]

#[macro_use]
extern crate rust_i18n;

mod action;
mod component;
mod state;
mod utils;

use std::{
    ops::Range,
    panic::{AssertUnwindSafe, catch_unwind},
    path::PathBuf,
    sync::Arc,
    thread,
    time::Duration,
};

use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, IconName, Root, Sizable as _, StyledExt as _, Theme, ThemeMode, ThemeRegistry, button::Button, label::Label, menu::AppMenuBar, progress::Progress, resizable::{h_resizable, resizable_panel}, v_flex,
};
use gpui_util::ResultExt as _;
use ranim::{
    Output, RanimScene, SceneConfig,
    cmd::{preview::Resolution, render_scene_output_with_progress},
};
use ranim_midi_visualizer_lib::{config::MidiVisualizerConfig, midi_visualizer_scene};
use tracing::{error, info, warn};
use waveform_utils::{
    music::{ControlContainer as _, Metric, Music, Note, NoteContainer as _},
    synth::MusicDirective,
};

use crate::{
    action::*,
    component::{
        PlaybackControl, PreviewArea,
        playback_control::{PlaybackEvent, PlaybackState, PlaybackStateInner},
    },
    state::*,
    utils::rgba_to_string,
};

i18n!("locales", fallback = "en");

pub struct VisualizerApp {
    menu_bar: Entity<AppMenuBar>,
    music: Arc<Music>,
    playback_state: PlaybackState,
    video_config: VideoConfigState,
    file_state: FileState,
    audio_state: AudioState,
    export_state: Entity<ExportWindowView>,
}

impl VisualizerApp {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let mut result = Self {
            menu_bar: AppMenuBar::new(cx),
            music: Arc::new(Music::default()),
            playback_state: PlaybackState::new(cx),
            video_config: VideoConfigState::new(cx),
            file_state: FileState::new(cx),
            audio_state: AudioState::new(),
            export_state: cx.new(|_cx| ExportWindowView::default()),
        };

        cx.spawn(async move |v, cx| {
            const PLAYBACK_LOOP_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 60);
            loop {
                let res = v.update(cx, |v, cx| {
                    v.playback_state.update(cx, |v, cx| {
                        v.update_playback(cx);
                    });
                });
                if let Err(err) = res {
                    // App has been released so playback loop ends
                    error!("{}", err);
                    break;
                }
                cx.background_executor().timer(PLAYBACK_LOOP_PERIOD).await;
            }
        })
        .detach();

        cx.subscribe(&result.playback_state, Self::on_playback_event)
            .detach();
        cx.observe(
            &result.file_state.recent_files(),
            Self::on_recent_files_changed,
        )
        .detach();
        cx.observe(
            &result.file_state.opened_file(),
            Self::on_opened_file_changed,
        )
        .detach();
        cx.subscribe(&result.export_state, Self::on_export_start_end)
            .detach();
        result.update_menus(cx);

        result
    }

    fn on_opened_file_changed(
        &mut self,
        _opened_file: Entity<Option<PathBuf>>,
        cx: &mut Context<Self>,
    ) {
        self.update_menus(cx);
    }

    fn on_recent_files_changed(
        &mut self,
        _recent_files: Entity<RecentFiles>,
        cx: &mut Context<Self>,
    ) {
        self.update_menus(cx);
    }

    fn on_export_start_end(
        &mut self,
        _export_state: Entity<ExportWindowView>,
        _event: &ExportEvent,
        cx: &mut Context<Self>,
    ) {
        self.update_menus(cx);
    }

    fn update_menus(&mut self, cx: &mut Context<Self>) {
        info!("Updating menus...");
        self.menu_bar.update(cx, |v, cx| {
            let menus = self.build_app_menus(cx);
            gpui_component::GlobalState::global_mut(cx).set_app_menus(menus);
            v.reload(cx);
        });
    }

    fn build_app_menus(&self, cx: &App) -> Vec<OwnedMenu> {
        vec![
            Menu::new(t!("menu.file"))
                .items([
                    MenuItem::action(t!("menu.file.open"), ShowOpenDialog),
                    MenuItem::action(t!("menu.file.close"), CloseFile)
                        .disabled(self.file_state.opened_file().read(cx).is_none()),
                    MenuItem::submenu(Menu::new(t!("menu.file.recent")).items({
                        let mut items = {
                            let recent_files = self.file_state.recent_files().read(cx);
                            if recent_files.is_empty() {
                                vec![
                                    MenuItem::action(t!("menu.file.recent.no-files"), NoAction)
                                        .disabled(true),
                                ]
                            } else {
                                recent_files
                                    .iter()
                                    .map(|v| {
                                        MenuItem::action(
                                            v.display().to_string(),
                                            OpenFile(v.clone()),
                                        )
                                    })
                                    .collect::<Vec<_>>()
                            }
                        };
                        items.extend([
                            MenuItem::separator(),
                            MenuItem::action(t!("menu.file.recent.clear"), ClearRecentFiles),
                        ]);
                        items
                    })),
                    MenuItem::separator(),
                    MenuItem::action(t!("menu.file.export"), ExportVideo)
                        .disabled(self.export_state.read(cx).is_exporting()),
                ])
                .owned(),
            Menu::new(t!("menu.style"))
                .items([
                    MenuItem::action(t!("menu.style.save"), SaveStyle),
                    MenuItem::action(t!("menu.style.load"), LoadStyle),
                    MenuItem::separator(),
                    MenuItem::action(t!("menu.style.revert"), RevertToDefault),
                ])
                .owned(),
        ]
    }

    fn on_playback_event(
        &mut self,
        _playback_state: Entity<PlaybackStateInner>,
        event: &PlaybackEvent,
        _cx: &mut Context<Self>,
    ) {
        match event {
            PlaybackEvent::Update(time_range) => {
                self.send_note_directives(time_range.clone());
            }
            PlaybackEvent::Play => {
                self.audio_state.play();
                self.audio_state
                    .synth
                    .lock()
                    .unwrap()
                    .directive(MusicDirective::Play);
            }
            PlaybackEvent::Pause => {
                self.audio_state
                    .synth
                    .lock()
                    .unwrap()
                    .directive(MusicDirective::Pause);
                self.audio_state.pause();
            }
            PlaybackEvent::Stop => {
                self.audio_state
                    .synth
                    .lock()
                    .unwrap()
                    .directive(MusicDirective::Stop);
                self.audio_state.pause();
            }
            _ => (),
        }
    }

    fn send_note_directives(&self, time_range: Range<Metric>) {
        let music = self.music.as_ref();
        for (_, instant) in music.as_mapped().note_instants_during(time_range.clone()) {
            let directive = if instant.is_end {
                Note::new_off(instant.pair.1.pitch)
            } else {
                *instant.pair.1
            };
            self.audio_state
                .synth
                .lock()
                .unwrap()
                .directive(directive.into());
        }
        for (_, _, &control) in music.as_mapped().controls_during(time_range.clone()) {
            self.audio_state
                .synth
                .lock()
                .unwrap()
                .directive(MusicDirective::Control(control));
        }
    }
}

impl Render for VisualizerApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // DockArea::new("dock_area", None, window, cx)
        // TabPanel::

        let preview_div = v_flex()
            .size_full()
            .child(
                div()
                    .flex_1()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(PreviewArea {
                        music: self.music.clone(),
                        video_config: self.video_config.clone(),
                        time: self.playback_state.read(cx).cur_time(),
                    }),
            )
            .child(
                PlaybackControl::new(self.playback_state.clone())
                    .border_t_1()
                    .border_color(cx.theme().border),
            );

        v_flex()
        .size_full()
        .child(
            div()
                    .h_auto()
                    .child(self.menu_bar.clone())
                    .border_b_1()
                    .border_color(cx.theme().border)
        ).child(
            div().flex_grow_1()
            .w_full()
            .child(
                h_resizable("visualizer_app_main")
                .child(preview_div.into_any_element())
                .child(
                    resizable_panel()
                    .size(px(400.))
                    .min_size(px(100.))
                    .max_size(px(600.))
                )
            )
        )
        .on_action(cx.listener(Self::action_show_open_dialog))
        .on_action(cx.listener(Self::action_open_file))
        .on_action(cx.listener(Self::action_close_file))
        .on_action(cx.listener(Self::action_revert_to_default_style))
        .on_action(cx.listener(Self::action_clear_recent_files))
        .on_action(cx.listener(Self::action_play_pause))
        .on_action(cx.listener(Self::action_jump_to_start))
        .on_action(cx.listener(Self::action_jump_to_end))
        .on_action(cx.listener(Self::action_toggle_looping))
        .on_action(cx.listener(Self::action_step_frame))
        .on_action(cx.listener(Self::action_start_export))
    }
}

#[derive(Debug, Clone, Default)]
pub enum ExportProgress {
    #[default]
    Pending,
    InProgress {
        total_frames: u64,
        completed_frames: u64,
    },
    Done,
    Error(String),
}

pub enum ExportEvent {
    ExportStarted,
    ExportDone,
}

#[derive(Default)]
struct ExportWindowView {
    progress: ExportProgress,
    receiver: Option<async_channel::Receiver<ExportProgress>>,
}

impl ExportWindowView {
    fn is_exporting(&self) -> bool {
        self.receiver.is_some()
    }
}

impl EventEmitter<ExportEvent> for ExportWindowView {}

impl Render for ExportWindowView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use ExportProgress::*;
        let export_percentage = match &self.progress {
            Pending => 0.,
            &InProgress {
                total_frames,
                completed_frames,
            } => (completed_frames as f32 / total_frames as f32) * 100.,
            _ => 100.,
        };

        div().p_4().size_full().flex().flex_col().child(
            div()
                .flex_grow_1()
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .child(
                    div()
                        .h_auto()
                        .w_full()
                        .flex()
                        .items_center()
                        .child(
                            Progress::new("export_progress")
                                .flex_grow_1()
                                .with_size(gpui_component::Size::Large)
                                .loading(matches!(&self.progress, Pending | Error(_)))
                                .value(export_percentage)
                                .when(matches!(&self.progress, Error(_)), |v| {
                                    v.color(Theme::global(cx).red)
                                })
                                .when(matches!(&self.progress, Done), |v| {
                                    v.color(Theme::global(cx).green)
                                }),
                        )
                        .child(
                            Button::new("export_pause")
                                .icon(IconName::Pause)
                                .aspect_square()
                                .on_click(|_event, _window, _cx| {
                                    info!("Export pause button clicked.");
                                    // [TODO] pause export progress
                                    // This requires relevant functionality to be implemented in Ranim
                                }),
                        ),
                )
                .child({
                    let mut elem = div().flex();
                    elem = match &self.progress {
                        Pending => elem.child(Label::new(t!("export.preparing"))),
                        InProgress {
                            total_frames,
                            completed_frames,
                        } => elem
                            .child(Label::new(t!("export.exporting")).font_bold())
                            .child(Label::new(t!(
                                "export.progress",
                                completed_frames = completed_frames,
                                total_frames = total_frames,
                                percentage = export_percentage : {:.2},
                            ))),
                        Done => elem.child(Label::new(t!("export.complete"))),
                        Error(err) => elem
                            .child(Label::new(t!("export.error")).font_bold())
                            .child(Label::new(format!(": {}", err))),
                    };
                    elem
                }),
        )
    }
}

/// Run the export loop in a separate thread.
struct ExportRunner {
    music: Arc<Music>,
    visualizer_config: MidiVisualizerConfig,
    export_config: Output,
    clear_color: Hsla,
    sender: async_channel::Sender<ExportProgress>,
}

impl FnOnce<()> for ExportRunner {
    type Output = ();

    extern "rust-call" fn call_once(self, args: ()) -> Self::Output {
        self.call(args)
    }
}

impl FnMut<()> for ExportRunner {
    extern "rust-call" fn call_mut(&mut self, args: ()) -> Self::Output {
        self.call(args)
    }
}

impl Fn<()> for ExportRunner {
    extern "rust-call" fn call(&self, _args: ()) -> Self::Output {
        let resolution = Resolution::new(self.export_config.width, self.export_config.height);
        let scene_config = SceneConfig {
            // [TODO] redundant conversion from color to string
            // This requires Ranim to doirectly use color as SceneConfig's value
            clear_color: rgba_to_string(self.clear_color),
        };
        let sender = self.sender.clone();
        let constructor = |r: &mut RanimScene| {
            midi_visualizer_scene(r, self.music.as_ref(), &self.visualizer_config, resolution);
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            render_scene_output_with_progress(
                constructor,
                "ranim-midi-visualizer".to_string(),
                &scene_config,
                &self.export_config,
                2,
                Some(Box::new(move |completed_frames, total_frames| {
                    sender
                        .send_blocking(ExportProgress::InProgress {
                            total_frames,
                            completed_frames,
                        })
                        .log_err();
                })),
            );
            self.sender.send_blocking(ExportProgress::Done).log_err();
        }));
        if let Err(err) = result {
            let err = if let Some(err) = err.downcast_ref::<&str>() {
                err.to_string()
            } else if let Some(err) = err.downcast_ref::<String>() {
                err.clone()
            } else {
                "Unknown error".to_string()
            };
            self.sender
                .send_blocking(ExportProgress::Error(err))
                .log_err();
        } else {
            self.sender.send_blocking(ExportProgress::Done).log_err();
        }
    }
}

impl VisualizerApp {
    fn export_loop(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.export_state.update(cx, |v, _cx| {
            if let Some(receiver) = &v.receiver
                && let Ok(progress) = receiver.try_recv()
            {
                v.progress = progress;
            }
        });
        let should_continue = !matches!(
            &self.export_state.read(cx).progress,
            ExportProgress::Done | ExportProgress::Error(_)
        );
        if should_continue {
            let this = cx.entity();
            window.on_next_frame(move |window, cx| {
                this.update(cx, |v, cx| v.export_loop(window, cx));
            });
        }
        window.refresh();
    }

    pub fn start_export(&mut self, cx: &mut Context<Self>) {
        let music = self.music.clone();
        let visualizer_config = self.video_config.visualizer_config.read(cx).clone();
        let export_config = self.video_config.export_config.read(cx).clone();
        let clear_color = self.video_config.clear_color;
        let (sender, receiver) = async_channel::unbounded();

        self.export_state.update(cx, |v, cx| {
            v.receiver = Some(receiver);
            cx.emit(ExportEvent::ExportStarted);
            cx.notify();
        });

        thread::spawn(ExportRunner {
            music,
            visualizer_config,
            export_config,
            clear_color,
            sender,
        });

        const EXPORT_DIALOG_SIZE: Size<Pixels> = size(px(400.), px(100.));
        // [TODO] current limitation of gpui: cannot close a window programmatically
        let bounds = Bounds::centered(None, EXPORT_DIALOG_SIZE, cx);
        let this = cx.entity();
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                kind: WindowKind::Floating,
                titlebar: Some(TitlebarOptions {
                    title: Some(t!("export.title").into()),
                    ..Default::default()
                }),
                is_resizable: false,
                ..Default::default()
            },
            move |window, cx| {
                let this_ = this.clone();
                window.on_next_frame(move |window, cx| {
                    this_.update(cx, |v, cx| {
                        v.export_loop(window, cx);
                    })
                });
                let this_ = this.clone();
                window.on_window_should_close(cx, move |_window, cx| {
                    use ExportProgress::*;
                    let should_close = matches!(
                        &this_.read(cx).export_state.read(cx).progress,
                        Done | Error(_)
                    );
                    if should_close {
                        this_.update(cx, |v, cx| {
                            v.export_state.update(cx, |v, cx| {
                                *v = Default::default();
                                cx.emit(ExportEvent::ExportDone);
                                cx.notify();
                            });
                        });
                        info!("Export window closed. Export complete!");
                        true
                    } else {
                        warn!("Cannot close due to export in progress.");
                        false
                    }
                });
                cx.new(|cx| Root::new(self.export_state.clone(), window, cx))
            },
        )
        .expect("Failed to open export window.");
    }
}

fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("ctrl-o", ShowOpenDialog, None),
        KeyBinding::new("ctrl-s", ExportVideo, None),
    ]
}

pub fn run_app() {
    rust_i18n::extend!(gpui_component);
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);
            component::init(cx);

            ThemeRegistry::watch_dir(PathBuf::from("./themes"), cx, move |cx| {
                if let Some(theme) = ThemeRegistry::global(cx)
                    .themes()
                    .get("Hybrid Dark")
                    .cloned()
                {
                    Theme::global_mut(cx).apply_config(&theme);
                }
            })
            .log_err();

            let theme_mode = match dark_light::detect().inspect_err(|v| warn!("{}", v)) {
                Ok(dark_light::Mode::Light) => ThemeMode::Light,
                _ => ThemeMode::Dark,
            };
            Theme::change(theme_mode, None, cx);

            cx.bind_keys(key_bindings());
            cx.activate(true);

            const MAIN_WINDOW_SIZE: Size<Pixels> = size(px(1200.), px(800.));
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        MAIN_WINDOW_SIZE,
                        cx,
                    ))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(t!("app.title").into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let view = cx.new(VisualizerApp::new);
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Failed to open window");
        });
}
