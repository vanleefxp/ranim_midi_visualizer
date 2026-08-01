use std::{
    mem,
    ops::{Deref, Range},
};

use derivative::Derivative;
use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme as _, Disableable, IconName, Selectable as _, StyledExt,
    button::{Button, ButtonRounded, ButtonVariants as _},
    label::Label,
    popover::Popover,
    slider::{Slider, SliderEvent, SliderScale, SliderState, SliderValue},
};
use jiff::{SignedDuration, Timestamp};
use rust_i18n::t;
use tracing::info;
use typed_floats::{self as tf, tf64};
use waveform_utils::music::{FrameRate, Metric};

use crate::utils::duration_to_string;
use actions::*;

pub mod actions {
    use super::*;

    actions!(
        playback,
        [PlayPause, JumpToStart, JumpToEnd, ToggleLooping,]
    );

    #[derive(
        Debug,
        Clone,
        Copy,
        PartialEq,
        Eq,
        Hash,
        Default,
        serde::Serialize,
        serde::Deserialize,
        schemars::JsonSchema,
        Action,
    )]
    #[action(namespace = playback)]
    pub struct StepFrame(pub isize);

    impl StepFrame {
        pub const FORWARD: Self = Self(1);
        pub const BACK: Self = Self(-1);
    }
}

pub type PlaybackSpeed = tf64::StrictlyPositiveFinite;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PlaybackEvent {
    /// Start playing.
    Play,
    /// Pause playing. All currently sounding notes will continue to sound when playback is resumed.
    Pause,
    /// Stop playing. Starts from blank when playback is resumed.
    Stop,
    /// Jump to a specific time.
    Jump { from: Metric, to: Metric },
    /// Indicating that a time range has been played.
    Update(Range<Metric>),
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum PlaybackParamEvent {
    TimeResolution(FrameRate),
    PlaybackSpeed(PlaybackSpeed),
    Looping(bool),
    MaxTime(Metric),
    SteppingFramerate(u32),
}

#[derive(Derivative)]
#[derivative(Debug, Clone, Default)]
pub struct PlaybackStateInner {
    #[derivative(Default(value = "1_000_000_000.try_into().unwrap()"))]
    time_resolution: FrameRate,
    cur_time: Metric,
    max_time: Metric,
    #[derivative(Default(value = "tf::as_const!(StrictlyPositiveFinite, 1f64)"))]
    playback_speed: PlaybackSpeed,
    play_start_time: Option<Timestamp>,
    looping: bool,
    #[derivative(Default(value = "60"))]
    stepping_framerate: u32,
}

#[allow(dead_code)]
impl PlaybackStateInner {
    pub fn time_resolution(&self) -> FrameRate {
        self.time_resolution
    }

    pub fn cur_time(&self) -> Metric {
        self.cur_time
    }

    pub fn max_time(&self) -> Metric {
        self.max_time
    }

    pub fn playback_speed(&self) -> PlaybackSpeed {
        self.playback_speed
    }

    pub fn looping(&self) -> bool {
        self.looping
    }

    pub fn stepping_framerate(&self) -> u32 {
        self.stepping_framerate
    }

    pub fn is_playing(&self) -> bool {
        self.play_start_time.is_some()
    }

    pub fn set_time_resolution(&mut self, time_resolution: FrameRate, cx: &mut Context<Self>) {
        self.time_resolution = time_resolution;
        cx.emit(PlaybackParamEvent::TimeResolution(time_resolution));
        cx.notify();
    }

    pub fn set_max_time(&mut self, max_time: Metric, cx: &mut Context<Self>) {
        self.max_time = max_time;
        cx.emit(PlaybackParamEvent::MaxTime(max_time));
        cx.notify();
    }

    pub fn set_playback_speed(&mut self, playback_speed: PlaybackSpeed, cx: &mut Context<Self>) {
        let playing = self.is_playing();
        if playing {
            self.pause(cx);
        }
        self.playback_speed = playback_speed;
        cx.emit(PlaybackParamEvent::PlaybackSpeed(playback_speed));
        cx.notify();
        if playing {
            self.play(cx);
        }
    }

    pub fn set_looping(&mut self, looping: bool, cx: &mut Context<Self>) {
        self.looping = looping;
        cx.emit(PlaybackParamEvent::Looping(looping));
        cx.notify();
    }

    pub fn set_stepping_framerate(&mut self, stepping_framerate: u32, cx: &mut Context<Self>) {
        self.stepping_framerate = stepping_framerate;
        cx.emit(PlaybackParamEvent::SteppingFramerate(stepping_framerate));
        cx.notify();
    }

    fn time_unit_to_duration(&self, time_unit: Metric) -> SignedDuration {
        SignedDuration::from_secs_f64(time_unit as f64 / self.time_resolution.get() as f64)
    }

    fn duration_to_time_unit(&self, duration: SignedDuration) -> Metric {
        (duration.as_secs_f64() * self.time_resolution.get() as f64) as Metric
    }

    fn get_play_start_time(&self, time_unit: Metric) -> Timestamp {
        Timestamp::now()
            - self
                .time_unit_to_duration(time_unit)
                .div_f64(self.playback_speed.into())
    }

    pub fn cur_time_duration(&self) -> SignedDuration {
        self.time_unit_to_duration(self.cur_time)
    }

    pub fn max_time_duration(&self) -> SignedDuration {
        self.time_unit_to_duration(self.max_time)
    }

    pub fn play(&mut self, cx: &mut Context<Self>) {
        info!("Start playing...");
        self.play_start_time = Some(self.get_play_start_time(self.cur_time));
        cx.emit(PlaybackEvent::Play);
        cx.notify();
    }

    pub fn pause(&mut self, cx: &mut Context<Self>) {
        info!("Paused.");
        self.play_start_time = None;
        cx.emit(PlaybackEvent::Pause);
        cx.notify();
    }

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        info!("Stopped.");
        self.play_start_time = None;
        cx.emit(PlaybackEvent::Stop);
        cx.notify();
    }

    pub fn jump_to_time(&mut self, time: Metric, cx: &mut Context<Self>) {
        info!("Jump to time {}.", time);
        let playing = self.is_playing();
        if playing {
            self.stop(cx);
        }
        let from_time = self.cur_time;
        self.cur_time = time;
        cx.emit(PlaybackEvent::Jump {
            from: from_time,
            to: time,
        });
        cx.notify();
        if playing {
            self.play(cx);
        }
    }

    pub fn update_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(play_start_time) = self.play_start_time {
            let now = Timestamp::now();
            let cur_time = now
                .duration_since(play_start_time)
                .mul_f64(self.playback_speed.into());
            let cur_time = self.duration_to_time_unit(cur_time);
            if cur_time > self.max_time {
                if self.looping {
                    self.jump_to_time(0, cx);
                } else {
                    self.cur_time = self.max_time;
                    self.pause(cx);
                }
            } else {
                let time_range = self.cur_time..cur_time;
                self.cur_time = cur_time;
                cx.emit(PlaybackEvent::Update(time_range));
                cx.notify();
            }
        }
    }

    pub fn step_frame(&mut self, n_frames: isize, cx: &mut Context<Self>) {
        let time_diff = (self.time_resolution.get() as f64 / self.stepping_framerate as f64
            * n_frames as f64) as Metric;
        self.jump_to_time((self.cur_time + time_diff).clamp(0, self.max_time), cx);
    }
}

impl EventEmitter<PlaybackEvent> for PlaybackStateInner {}
impl EventEmitter<PlaybackParamEvent> for PlaybackStateInner {}

trait SliderEventExt {
    fn value(&self) -> f32;
}

impl SliderEventExt for SliderEvent {
    fn value(&self) -> f32 {
        match self {
            &Self::Change(value) | &Self::Release(value) => match value {
                SliderValue::Single(value) | SliderValue::Range(_, value) => value,
            },
        }
    }
}

impl PlaybackStateInner {
    fn on_playback_slider_change(
        &mut self,
        _slider_state: Entity<SliderState>,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        let time = (event.value() as f64 * self.time_resolution.get() as f64) as Metric;
        self.jump_to_time(time, cx);
    }
    fn on_playback_speed_slider_change(
        &mut self,
        _slider_state: Entity<SliderState>,
        event: &SliderEvent,
        cx: &mut Context<Self>,
    ) {
        let value = PlaybackSpeed::try_from(event.value() as f64).unwrap();
        self.set_playback_speed(value, cx);
    }
}

const CONTEXT: &str = "PlaybackControl";
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("left", StepFrame(-1), Some(CONTEXT)),
        KeyBinding::new("right", StepFrame(1), Some(CONTEXT)),
        KeyBinding::new("space", PlayPause, Some(CONTEXT)),
        KeyBinding::new("[", JumpToStart, Some(CONTEXT)),
        KeyBinding::new("]", JumpToEnd, Some(CONTEXT)),
        KeyBinding::new("l", ToggleLooping, Some(CONTEXT)),
    ]);
}

const MAX_PLAYBACK_SPEED: f64 = 10.;
#[inline(always)]
const fn clamp_playback_speed(value: impl [const] Into<f64>) -> PlaybackSpeed {
    // SAFETY: strictly positive finite f64 value due to clamping bounds
    // `Result::unwrap` is not `const fn` so `unsafe` is needed here
    unsafe {
        PlaybackSpeed::new_unchecked(
            value
                .into()
                .clamp(MAX_PLAYBACK_SPEED.recip(), MAX_PLAYBACK_SPEED),
        )
    }
}

/// Wrapper for [`PlaybackState`] and its related [`SliderState`] entities.
#[derive(Debug, Clone)]
pub struct PlaybackState {
    playback_state: Entity<PlaybackStateInner>,
    playback_slider_state: Entity<SliderState>,
    playback_speed_slider_state: Entity<SliderState>,
}

impl Deref for PlaybackState {
    type Target = Entity<PlaybackStateInner>;
    fn deref(&self) -> &Self::Target {
        &self.playback_state
    }
}

impl PlaybackState {
    pub fn new(cx: &mut App) -> Self {
        let playback_state = cx.new(|_cx| PlaybackStateInner::default());
        let playback_slider_state = {
            let max_time = playback_state.read(cx).max_time_duration().as_secs_f32();
            let cur_time = playback_state.read(cx).cur_time_duration().as_secs_f32();
            let frame_step = (playback_state.read(cx).stepping_framerate as f64).recip() as f32;
            cx.new(|_cx| {
                SliderState::new()
                    .min(0.)
                    .max(max_time)
                    .default_value(cur_time)
                    .step(frame_step)
            })
        };
        let playback_speed_slider_state = cx.new(|_cx| {
            SliderState::new()
                .min(MAX_PLAYBACK_SPEED.recip() as f32)
                .max(MAX_PLAYBACK_SPEED as f32)
                .default_value(1.)
                .scale(SliderScale::Logarithmic)
                .step(0.01)
        });

        {
            let playback_slider_state = playback_slider_state.clone();
            playback_state.update(cx, |_value, cx| {
                cx.subscribe(
                    &playback_slider_state,
                    PlaybackStateInner::on_playback_slider_change,
                )
                .detach();
            });
        }
        {
            let playback_speed_slider_state = playback_speed_slider_state.clone();
            playback_state.update(cx, |_value, cx| {
                cx.subscribe(
                    &playback_speed_slider_state,
                    PlaybackStateInner::on_playback_speed_slider_change,
                )
                .detach();
            });
        }
        {
            let playback_state = playback_state.clone();
            playback_slider_state.update(cx, |_value, cx| {
                cx.subscribe(
                    &playback_state,
                    |slider_state, playback_state, event: &PlaybackEvent, cx| match *event {
                        PlaybackEvent::Update(Range { start: time, .. })
                        | PlaybackEvent::Jump { to: time, .. } => {
                            let value = (time as f64
                                / playback_state.read(cx).time_resolution.get() as f64)
                                as f32;
                            *slider_state =
                                mem::replace(slider_state, SliderState::new()).default_value(value);
                        }
                        _ => (),
                    },
                )
                .detach();
                cx.subscribe(
                    &playback_state,
                    |slider_state, playback_state, event: &PlaybackParamEvent, cx| match *event {
                        PlaybackParamEvent::MaxTime(max_time) => {
                            let max_value = (max_time as f64
                                / playback_state.read(cx).time_resolution.get() as f64)
                                as f32;
                            *slider_state =
                                mem::replace(slider_state, SliderState::new()).max(max_value);
                        }
                        PlaybackParamEvent::SteppingFramerate(stepping_framerate) => {
                            let step = (stepping_framerate as f64).recip() as f32;
                            *slider_state =
                                mem::replace(slider_state, SliderState::new()).step(step);
                        }
                        _ => (),
                    },
                )
                .detach();
            });
            playback_speed_slider_state.update(cx, |_value, cx| {
                cx.subscribe(
                    &playback_state,
                    |slider_state, _playback_state, event: &PlaybackParamEvent, _cx| {
                        if let &PlaybackParamEvent::PlaybackSpeed(playback_speed) = event {
                            // [TODO] `set_value` requires `Window` so use `mem::replace` as a temporary solution
                            // This should be reported as an issue to `gpui_component`
                            *slider_state = mem::replace(slider_state, SliderState::new())
                                .default_value(f64::from(playback_speed) as f32);
                        }
                    },
                )
                .detach();
            });
        }

        Self {
            playback_state,
            playback_slider_state,
            playback_speed_slider_state,
        }
    }
}

#[derive(Debug, Clone, IntoElement)]
pub struct PlaybackControl {
    state: PlaybackState,
    style: StyleRefinement,
    disabled: bool,
}

impl PlaybackControl {
    pub fn new(state: PlaybackState) -> Self {
        Self {
            state,
            style: StyleRefinement::default(),
            disabled: false,
        }
    }
}

impl Styled for PlaybackControl {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl Disableable for PlaybackControl {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for PlaybackControl {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            state:
                PlaybackState {
                    playback_state,
                    playback_slider_state,
                    playback_speed_slider_state,
                },
            style,
            disabled,
        } = self;
        let cur_time_duration = playback_state.read(cx).cur_time_duration();
        let max_time_duration = playback_state.read(cx).max_time_duration();
        let playback_disabled = playback_state.read(cx).max_time <= 0;
        let is_playing = playback_state.read(cx).is_playing();
        let is_looping = playback_state.read(cx).looping;
        let playback_speed = playback_state.read(cx).playback_speed();

        let state_id = playback_state.entity_id();
        let element_id = ElementId::from(("playback_control", state_id));
        let focus_handle = window
            .use_keyed_state(element_id.clone(), cx, |_, cx| cx.focus_handle())
            .read(cx)
            .clone();

        div()
            .id(element_id)
            .key_context(CONTEXT)
            .when(!disabled, |v| v.track_focus(&focus_handle))
            .h_auto()
            .w_full()
            .flex()
            .items_center()
            .refine_style(&style)
            .child(
                div()
                    .flex()
                    .items_stretch()
                    .child(
                        Button::new(("jump_to_start", state_id))
                            .aspect_square()
                            .icon(IconName::ChevronLeft)
                            .tooltip_with_action(
                                t!("playback.jump-to-start"),
                                &JumpToStart,
                                Some(CONTEXT),
                            )
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .disabled(playback_disabled || disabled)
                            .on_click(|_, window, cx| {
                                info!("\"Jump to start\" button clicked!");
                                window.dispatch_action(Box::new(JumpToStart), cx);
                            }),
                    )
                    .child(
                        Button::new(("step_back", state_id))
                            .aspect_square()
                            .icon(IconName::ChevronLeft)
                            .tooltip_with_action(
                                t!("playback.step-back"),
                                &StepFrame::BACK,
                                Some(CONTEXT),
                            )
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .disabled(playback_disabled || disabled)
                            .on_click(|_, window, cx| {
                                info!("\"Step back\" button clicked!");
                                window.dispatch_action(Box::new(StepFrame::BACK), cx);
                            }),
                    )
                    .child(
                        Button::new(("play_pause", state_id))
                            .aspect_square()
                            .when_else(
                                is_playing,
                                |v| {
                                    v.icon(IconName::Pause)
                                        .tooltip_with_action(
                                            t!("playback.pause"),
                                            &PlayPause,
                                            Some(CONTEXT),
                                        )
                                        .selected(true)
                                },
                                |v| {
                                    v.icon(IconName::Play).tooltip_with_action(
                                        t!("playback.play"),
                                        &PlayPause,
                                        Some(CONTEXT),
                                    )
                                },
                            )
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .disabled(playback_disabled || disabled)
                            .on_click(|_, window, cx| {
                                info!("\"Play / Pause\" button clicked!");
                                window.dispatch_action(Box::new(PlayPause), cx);
                            }),
                    )
                    .child(
                        Button::new(("step_forward", state_id))
                            .aspect_square()
                            .icon(IconName::ChevronRight)
                            .tooltip_with_action(
                                t!("playback.step-forward"),
                                &StepFrame::FORWARD,
                                Some(CONTEXT),
                            )
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .disabled(playback_disabled || disabled)
                            .on_click(|_, window, cx| {
                                info!("\"Step forward\" button clicked!");
                                window.dispatch_action(Box::new(StepFrame::FORWARD), cx);
                            }),
                    )
                    .child(
                        Button::new(("jump_to_end", state_id))
                            .aspect_square()
                            .icon(IconName::ChevronRight)
                            .tooltip_with_action(
                                t!("playback.jump-to-end"),
                                &JumpToEnd,
                                Some(CONTEXT),
                            )
                            .ghost()
                            .compact()
                            .ghost()
                            .compact()
                            .rounded(ButtonRounded::None)
                            .disabled(playback_disabled || disabled)
                            .on_click(|_, window, cx| {
                                info!("\"Jump to end\" button clicked!");
                                window.dispatch_action(Box::new(JumpToEnd), cx);
                            }),
                    )
                    .child(
                        Button::new(("toggle_looping", state_id))
                            .aspect_square()
                            .icon(IconName::LoaderCircle)
                            .tooltip_with_action(t!("playback.loop"), &ToggleLooping, Some(CONTEXT))
                            .ghost()
                            .compact()
                            .selected(is_looping)
                            .rounded(ButtonRounded::None)
                            .disabled(disabled)
                            .on_click(|_, window, cx| {
                                info!("\"Looping on / off\" button clicked!");
                                window.dispatch_action(Box::new(ToggleLooping), cx);
                            }),
                    )
                    .child(
                        Popover::new(("playback_speed", state_id))
                            .anchor(Anchor::BottomCenter)
                            .p_2()
                            .trigger(
                                Button::new(("playback_speed_button", state_id))
                                    .ghost()
                                    .compact()
                                    .rounded(ButtonRounded::None)
                                    .disabled(disabled)
                                    .label(format!("{:.2}×", f64::from(playback_speed)))
                                    .font_family(&cx.theme().mono_font_family)
                                    .on_mouse_up(MouseButton::Right, {
                                        let playback_state = playback_state.clone();
                                        move |_event, _window, cx| {
                                            cx.update_entity(&playback_state, |v, cx| {
                                                v.set_playback_speed(
                                                    tf::as_const!(StrictlyPositiveFinite, 1.),
                                                    cx,
                                                )
                                            });
                                        }
                                    })
                                    .on_scroll_wheel({
                                        let playback_state = playback_state.clone();
                                        move |event, _window, cx| {
                                            if let ScrollDelta::Pixels(p) = event.delta {
                                                let playback_speed_delta = -p.y.to_f64() * 0.005;
                                                playback_state.update(cx, |v, cx| {
                                                    let playback_speed = clamp_playback_speed(
                                                        f64::from(v.playback_speed())
                                                            + playback_speed_delta,
                                                    );
                                                    v.set_playback_speed(playback_speed, cx);
                                                });
                                            }
                                        }
                                    })
                                    .tooltip(t!("playback.speed")),
                            )
                            .child(Slider::new(&playback_speed_slider_state).w_48()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .gap_2()
                    .px_2()
                    .child(
                        Label::new(duration_to_string(cur_time_duration))
                            .font_family(&cx.theme().mono_font_family),
                    )
                    .child(
                        Slider::new(&playback_slider_state)
                            .flex_1()
                            .disabled(playback_disabled),
                    )
                    .child(
                        Label::new(duration_to_string(max_time_duration))
                            .font_family(&cx.theme().mono_font_family),
                    ),
            )
            .on_action::<PlayPause>({
                let playback_state = playback_state.clone();
                move |_action, window, cx| {
                    info!("Action \"Play / Pause\" triggered!");
                    playback_state.update(cx, |v, cx| {
                        if v.is_playing() {
                            v.pause(cx);
                        } else {
                            v.play(cx);
                        }
                    });
                    window.refresh();
                }
            })
            .on_action::<JumpToStart>({
                let playback_state = playback_state.clone();
                move |_action, window, cx| {
                    playback_state.update(cx, |v, cx| {
                        v.jump_to_time(0, cx);
                    });
                    window.refresh();
                }
            })
            .on_action::<JumpToEnd>({
                let playback_state = playback_state.clone();
                move |_action, window, cx| {
                    playback_state.update(cx, |v, cx| {
                        v.jump_to_time(v.max_time(), cx);
                    });
                    window.refresh();
                }
            })
            .on_action::<StepFrame>({
                let playback_state = playback_state.clone();
                move |&StepFrame(n), window, cx| {
                    playback_state.update(cx, |v, cx| {
                        v.step_frame(n, cx);
                    });
                    window.refresh();
                }
            })
            .on_action::<ToggleLooping>({
                let playback_state = playback_state.clone();
                move |_action, window, cx| {
                    playback_state.update(cx, |v, cx| {
                        v.set_looping(!v.looping(), cx);
                    });
                    window.refresh();
                }
            })
    }
}
