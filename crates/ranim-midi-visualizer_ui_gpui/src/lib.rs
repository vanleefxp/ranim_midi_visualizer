#![feature(decl_macro)]

mod action;
mod component;
mod menu;
mod state;

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    IconName, Root, Selectable, Theme, ThemeMode,
    button::{Button, ButtonRounded, ButtonVariants},
    menu::AppMenuBar,
};
use tracing::{info, warn};

use crate::{
    action::*,
    component::PreviewArea,
    menu::update_menus,
    state::{MusicDataState, PlaybackState, *},
};

#[derive(Debug, Clone)]
pub struct VisualizerApp {
    focus_handle: FocusHandle,
    menu_bar: Entity<AppMenuBar>,
}

impl Render for VisualizerApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Reload menu bar if needed
        if cx.read_global::<ShouldReloadMenuBar, _>(|g, _cx| **g) {
            cx.update_entity(&self.menu_bar, |v, cx| v.reload(cx));
            cx.update_global::<ShouldReloadMenuBar, _>(|g, _cx| **g = false);
        }

        div()
            .track_focus(&self.focus_handle)
            .flex()
            .flex_col()
            .size_full()
            .child(
                div()
                    .h_auto()
                    .child(self.menu_bar.clone())
                    .border_b_1()
                    .border_color(Theme::global(cx).border),
            )
            .child(
                div()
                    .flex_grow(1.)
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(PreviewArea {
                        music: music_data::music(cx),
                        visualizer_config: video_config::visualizer_config(cx),
                        export_config: video_config::export_config(cx),
                        clear_color: video_config::clear_color(cx),
                        time: playback::time(cx),
                    }),
            )
            .child(
                div()
                    .h_auto()
                    .border_t_1()
                    .border_color(Theme::global(cx).border)
                    .flex()
                    .child(
                        Button::new("jump_to_start")
                            .icon(IconName::ChevronLeft)
                            .tooltip_with_action("Jump to start", &JumpToStart, None)
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .on_click(|_, window, cx| {
                                info!("\"Jump to start\" button clicked!");
                                window.dispatch_action(Box::new(JumpToStart), cx);
                            }),
                    )
                    .child(
                        Button::new("step_back")
                            .icon(IconName::ChevronLeft)
                            .tooltip_with_action("Step back 1 frame", &StepBack, None)
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .on_click(|_, window, cx| {
                                info!("\"Step back\" button clicked!");
                                window.dispatch_action(Box::new(StepBack), cx);
                            }),
                    )
                    .child(
                        Button::new("play_pause")
                            .when_else(
                                playback::is_playing(cx),
                                |v| {
                                    v.icon(IconName::Pause)
                                        .tooltip_with_action("Pause", &PlayPause, None)
                                        .selected(true)
                                },
                                |v| {
                                    v.icon(IconName::Play)
                                        .tooltip_with_action("Play", &PlayPause, None)
                                },
                            )
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .on_click(|_, window, cx| {
                                info!("\"Play / Pause\" button clicked!");
                                window.dispatch_action(Box::new(PlayPause), cx);
                            }),
                    )
                    .child(
                        Button::new("step_forward")
                            .icon(IconName::ChevronRight)
                            .tooltip_with_action("Step forward 1 frame", &StepForward, None)
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .on_click(|_, window, cx| {
                                info!("\"Step forward\" button clicked!");
                                window.dispatch_action(Box::new(StepForward), cx);
                            }),
                    )
                    .child(
                        Button::new("jump_to_end")
                            .icon(IconName::ChevronRight)
                            .tooltip_with_action("Jump to end", &JumpToEnd, None)
                            .ghost()
                            .compact()
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .on_click(|_, window, cx| {
                                info!("\"Jump to end\" button clicked!");
                                window.dispatch_action(Box::new(JumpToEnd), cx);
                            }),
                    )
                    .child(
                        Button::new("toggle_looping")
                            .icon(IconName::LoaderCircle)
                            .tooltip_with_action("Toggle looping", &ToggleLooping, None)
                            .ghost()
                            .compact()
                            .selected(playback::is_looping(cx))
                            .rounded(ButtonRounded::None)
                            .on_click(|_, window, cx| {
                                info!("\"Toogle looping\" button clicked!");
                                window.dispatch_action(Box::new(ToggleLooping), cx);
                            }),
                    ),
            )
            .on_action(show_open_dialog)
            .on_action(open_file)
            .on_action(load_style)
            .on_action(save_style)
            .on_action(revert_to_default_style)
            .on_action(clear_recent_files)
            .on_action(play_pause)
            .on_action(jump_to_start)
            .on_action(jump_to_end)
            .on_action(toggle_looping)
            .on_action(step_forward)
            .on_action(step_back)
    }
}

fn key_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("ctrl-o", ShowOpenDialog, None),
        KeyBinding::new("ctrl-e", ExportVideo, None),
        KeyBinding::new("space", PlayPause, None),
        KeyBinding::new("left", StepBack, None),
        KeyBinding::new("right", StepForward, None),
    ]
}

pub fn run_app() {
    gpui_platform::application()
        .with_assets(gpui_component_assets::Assets)
        .run(move |cx| {
            gpui_component::init(cx);

            let theme_mode = match dark_light::detect().inspect_err(|v| warn!("{}", v)) {
                Ok(dark_light::Mode::Light) => ThemeMode::Light,
                _ => ThemeMode::Dark,
            };
            Theme::change(theme_mode, None, cx);

            cx.bind_keys(key_bindings());
            FileState::init(cx);
            MusicDataState::init(cx);
            VideoConfigState::init(cx);
            PlaybackState::init(cx);
            playback::refresh(cx);
            cx.default_global::<ShouldReloadMenuBar>();
            update_menus(cx);
            cx.activate(true);

            cx.spawn(async |cx| {
                cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds(
                            point(px(0.), px(0.)),
                            size(px(1200.), px(800.)),
                        ))),
                        titlebar: Some(TitlebarOptions {
                            title: Some("Ranim MIDI Visualizer".into()),
                            ..Default::default()
                        }),
                        ..Default::default()
                    },
                    |window, cx| {
                        let view = cx.new(|cx| VisualizerApp {
                            focus_handle: cx.focus_handle(),
                            menu_bar: AppMenuBar::new(cx),
                        });
                        cx.new(|cx| Root::new(view, window, cx))
                    },
                )
                .expect("Failed to open window");
            })
            .detach();
        });
}
