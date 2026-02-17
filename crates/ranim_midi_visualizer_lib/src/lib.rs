#![feature(range_into_bounds)]

pub mod cyc_index;
pub mod items;
pub mod midi;
pub mod stroke_and_fill;

use std::{ops::Range, sync::Arc};

use crate::{
    cyc_index::IndexCyc as _,
    items::{PianoKeyboard, PianoKeyboardSize, PianoPedals},
    midi::{MidiMusic, MultiTrackLoc, MultiTrackMidiNote, MultiTrackPedalInstant},
};
use ranim::{
    Output, SceneConfig,
    anims::morph::MorphAnim,
    cmd::render::render_scene_output,
    color::{AlphaColor, Srgb},
    core::animation::{Eval, StaticAnim as _},
    glam::{DVec2, DVec3, dvec2, dvec3},
    items::vitem::{
        geometry::{Rectangle, anchor::Origin},
        text::{TextFont, TextItem},
    },
    prelude::*,
    utils::rate_functions::linear,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ColorBy {
    #[default]
    Channel,
    Track,
    KeyColor,
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct StatusBarConfig {
    pub em_size: f64,
    pub padding: [DVec2; 2],
}

impl Default for StatusBarConfig {
    fn default() -> Self {
        Self {
            em_size: 0.2,
            padding: [dvec2(0.1, 0.1), dvec2(0.1, 0.05)],
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ProgressBarConfig {
    pub height: f64,
    pub color: AlphaColor<Srgb>,
}

impl Default for ProgressBarConfig {
    fn default() -> Self {
        Self {
            height: 0.06,
            color: AlphaColor::from_rgb8(168, 163, 204), // rgb(168, 163, 204)
        }
    }
}

#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
// #[non_exhaustive]
pub struct MidiVisualizerConfig {
    pub colors: Vec<AlphaColor<Srgb>>,
    pub scroll_speed: f64,
    pub color_by: ColorBy,
    pub buf_time: [f64; 2],
    pub keyboard_size: PianoKeyboardSize,
    pub key_range: Range<u8>,
    pub status_bar_config: StatusBarConfig,
    pub progress_bar_config: ProgressBarConfig,
    pub time_window: f64,
    #[serde(skip)]
    pub text_font: Arc<TextFont>,
}

impl Default for MidiVisualizerConfig {
    fn default() -> Self {
        Self {
            colors: vec![
                rgb8(0x89, 0xb9, 0xeb),
                rgb8(0x9b, 0xe3, 0x47),
                rgb8(0xf7, 0x93, 0x1e),
                rgb8(0xf7, 0xc7, 0x1e),
            ],
            color_by: ColorBy::Channel,
            scroll_speed: 2.,
            buf_time: [2., 2.],
            keyboard_size: Default::default(),
            key_range: 21..109,
            status_bar_config: Default::default(),
            progress_bar_config: Default::default(),
            time_window: 1.,
            text_font: Arc::new(TextFont::new([
                "Maple Mono NF",
                "Cascadia Code NF",
                "LXGW WenKai Mono",
                "Consolas",
                "Monaco",
                "Courier New",
            ])),
        }
    }
}

macro_rules! key_is_black {
    ($key:expr) => {
        matches!($key % 12, 1 | 3 | 6 | 8 | 10)
    };
}

struct TimerTextAnim {
    origin: DVec3,
    em_size: f64,
    duration: f64,
    text_font: Arc<TextFont>,
}

impl Eval<TextItem> for TimerTextAnim {
    fn eval_alpha(&self, alpha: f64) -> TextItem {
        let &Self {
            origin,
            em_size,
            duration,
            ..
        } = self;

        let time = alpha * duration;
        let nano = (time * 1e9) as u64;
        let (nano, sec) = (nano % 1_000_000_000, nano / 1_000_000_000);
        let micro = nano / 1_000_000;
        let (sec, min) = (sec % 60, sec / 60);
        let (min, hour) = (min % 60, min / 60);
        let hour = hour % 100;

        let src = format!("TIME {hour:02}:{min:02}:{sec:02}.{micro:03}");

        TextItem::new(src, em_size)
            .with_font(self.text_font.as_ref().clone())
            .with(|item| item.move_anchor_to(Origin, origin).discard())
    }
}

struct NPSTextAnim {
    origin: DVec3,
    em_size: f64,
    window_size: f64,
    music: Arc<MidiMusic>,
    text_font: Arc<TextFont>,
}

impl NPSTextAnim {
    fn calc_value(&self, alpha: f64) -> f64 {
        let &Self { window_size, .. } = self;
        let duration = self.music.duration() as f64 / 1e9 + window_size;
        let window_end = alpha * duration;
        let window_end_nano = (window_end * 1e9) as u64;
        let window_nano = (window_size * 1e9) as u64;
        self.music.nps(window_end_nano, window_nano)
    }

    fn create_item(&self, nps: f64) -> TextItem {
        let &Self {
            origin,
            em_size: font_size,
            ..
        } = self;
        let src = format!("NPS {:.0}", nps);
        TextItem::new(src, font_size)
            .with_font(self.text_font.as_ref().clone())
            .with(|item| item.move_anchor_to(Origin, origin).discard())
    }
}

impl Eval<TextItem> for NPSTextAnim {
    fn eval_alpha(&self, alpha: f64) -> TextItem {
        let nps = self.calc_value(alpha);
        let item = self.create_item(nps);
        item
    }
}

struct LegatoTextAnim {
    origin: DVec3,
    font_size: f64,
    window_size: f64,
    music: Arc<MidiMusic>,
    text_font: Arc<TextFont>,
}

impl LegatoTextAnim {
    fn calc_value(&self, alpha: f64) -> f64 {
        let &Self { window_size, .. } = self;
        let duration = self.music.duration() as f64 / 1e9 + window_size;
        let window_end = alpha * duration;
        let window_end_nano = (window_end * 1e9) as u64;
        let window_nano = (window_size * 1e9) as u64;
        self.music.legato_index(window_end_nano, window_nano)
    }

    fn create_item(&self, legato_index: f64) -> TextItem {
        let &Self {
            origin,
            font_size: em_size,
            ..
        } = self;
        let src = format!("LEGATO {:.3}", legato_index);
        TextItem::new(src, em_size)
            .with_font(self.text_font.as_ref().clone())
            .with(|item| item.move_anchor_to(Origin, origin).discard())
    }
}

impl Eval<TextItem> for LegatoTextAnim {
    fn eval_alpha(&self, alpha: f64) -> TextItem {
        let legato_index = self.calc_value(alpha);
        let item = self.create_item(legato_index);
        item
    }
}

pub fn midi_visualizer_scene(
    r: &mut RanimScene,
    song: Arc<MidiMusic>,
    config: &MidiVisualizerConfig,
    video_size: (u32, u32),
) {
    let cam = CameraFrame::default();
    r.insert(cam.clone());

    let &MidiVisualizerConfig {
        scroll_speed,
        color_by,
        buf_time,
        status_bar_config:
            StatusBarConfig {
                em_size: font_size,
                padding,
            },
        progress_bar_config:
            ProgressBarConfig {
                height: progress_bar_height,
                color: progress_bar_color,
            },
        time_window: window_size,
        ..
    } = config;
    let colors = &config.colors;
    let font = &config.text_font;

    let frame_height = cam.frame_height;
    let frame_width = frame_height * video_size.0 as f64 / video_size.1 as f64;
    let frame_bottom_left = dvec3(-frame_width / 2., -frame_height / 2., 0.);
    let frame_top_left = dvec3(-frame_width / 2., frame_height / 2., 0.);
    let progress_bar_min = frame_top_left - DVec3::Y * progress_bar_height;
    let status_bar_height = font_size + padding[0].y + padding[1].y;

    // static items
    r.insert_with(|tl| {
        let rect_setup = |item: &mut Rectangle| {
            item.set_color(AlphaColor::BLACK.with_alpha(0.5))
                .set_stroke_opacity(0.)
                .shift(DVec3::NEG_Z * 1e-4)
                .discard()
        };
        let i_status_bar_rect =
            Rectangle::from_min_size(frame_bottom_left, dvec2(frame_width, status_bar_height))
                .with(rect_setup);
        tl.play(i_status_bar_rect.show());
    });

    let i_keyboard_tem = PianoKeyboard::default().with(|item| {
        item.set_size(|size| *size = config.keyboard_size)
            .set_key_range(config.key_range.clone());

        let width = item.aabb_size().x;
        let scale_factor = frame_width / width;
        item.scale(DVec3::splat(scale_factor));
        item.move_anchor_to(
            AabbPoint(dvec3(-1., -1., -1.)),
            frame_bottom_left + status_bar_height * DVec3::Y,
        );
    });
    let i_pedals_tem = PianoPedals::default().with(|item| {
        item.move_anchor_to(
            AabbPoint(dvec3(1., -1., 0.)),
            i_keyboard_tem.aabb()[1] + dvec3(-0.2, 0.2, 1e-4),
        )
        .discard()
    });

    let scroll_height = frame_height - i_keyboard_tem.aabb_size().y;
    let scroll_time = scroll_height / scroll_speed;
    let duration = song.duration() as f64 / 1e9;

    let instants = song.instants().collect::<Vec<_>>();
    let text_origin = |n_columns: usize, column: usize| {
        let available_width = frame_width - padding[0].x - padding[1].x;
        let dx = available_width / n_columns as f64 * column as f64 + padding[0].x;
        let dy = padding[0].y;
        frame_bottom_left + dvec3(dx, dy, 1e-4)
    };

    // progress bar
    r.insert_with(|tl| {
        let progress_bar_setup = |item: &mut Rectangle| {
            item.set_fill_color(progress_bar_color)
                .set_stroke_opacity(0.)
                .shift(DVec3::Z * 2e-4)
                .discard()
        };
        let mut i_progress_bar =
            Rectangle::from_min_size(progress_bar_min, dvec2(0., progress_bar_height))
                .with(progress_bar_setup);
        let i_progress_bar_final =
            Rectangle::from_min_size(progress_bar_min, dvec2(frame_width, progress_bar_height))
                .with(progress_bar_setup);
        tl.forward_to(buf_time[0] + scroll_time).play(
            i_progress_bar
                .morph_to(i_progress_bar_final)
                .with_duration(duration)
                .with_rate_func(linear),
        );
    });

    // timer
    r.insert_with(|tl| {
        let origin = text_origin(4, 0);
        let timer_anim = TimerTextAnim {
            origin,
            em_size: font_size,
            duration,
            text_font: font.clone(),
        };
        let i_timer_zero = timer_anim.eval_alpha(0.);
        let i_timer_final = timer_anim.eval_alpha(1.);
        tl.play(i_timer_zero.show())
            .forward_to(buf_time[0] + scroll_time)
            .play(i_timer_zero.hide())
            .play(
                timer_anim
                    .into_animation_cell()
                    .with_duration(duration)
                    .with_rate_func(linear),
            )
            .play(i_timer_final.show())
            .forward(buf_time[1]);
    });

    // note count
    r.insert_with(|tl| {
        let origin = text_origin(4, 1);
        let create_note_count_text = |n: usize| {
            let src = format!("NOTE COUNT {n}");
            TextItem::new(src, font_size)
                .with_font(font.as_ref().clone())
                .with(|item| item.move_anchor_to(Origin, origin).discard())
        };

        let mut note_count = 0usize;
        let mut i_note_count = create_note_count_text(note_count);
        tl.play(i_note_count.show())
            .forward(buf_time[0] + scroll_time);

        for instant in instants.iter().filter(|instant| instant.is_start()) {
            tl.forward_to(instant.time as f64 / 1e9 + buf_time[0] + scroll_time);
            note_count += 1;
            tl.play(i_note_count.hide());
            i_note_count = create_note_count_text(note_count);
            tl.play(i_note_count.show());
        }
    });

    // note per second
    r.insert_with(|tl| {
        let origin = text_origin(4, 2);
        let anim = NPSTextAnim {
            origin,
            em_size: font_size,
            window_size,
            music: song.clone(),
            text_font: font.clone(),
        };
        let i_nps_zero = anim.create_item(0.);
        tl.play(i_nps_zero.show())
            .forward_to(buf_time[0] + scroll_time)
            .play(i_nps_zero.hide())
            .play(
                anim.into_animation_cell()
                    .with_duration(duration + window_size),
            )
            .play(i_nps_zero.show());
    });

    // legato index
    r.insert_with(|tl| {
        let origin = text_origin(4, 3);
        let anim = LegatoTextAnim {
            origin,
            font_size,
            window_size,
            music: song.clone(),
            text_font: font.clone(),
        };
        let i_legato_zero = anim.create_item(0.);
        tl.play(i_legato_zero.show())
            .forward_to(buf_time[0] + scroll_time)
            .play(i_legato_zero.hide())
            .play(
                anim.into_animation_cell()
                    .with_duration(duration + window_size),
            )
            .play(i_legato_zero.show());
    });

    // keyboard animation
    r.insert_with(|tl| {
        let mut i_keyboard = i_keyboard_tem.clone();
        tl.play(i_keyboard.show()).forward(buf_time[0] + scroll_time);

        for instant in instants.iter() {
            tl.forward_to(instant.time as f64 / 1e9 + buf_time[0] + scroll_time);
            tl.play(i_keyboard.hide());
            i_keyboard = i_keyboard.with(|item| {
                let key = instant.key();

                if instant.is_start() {
                    item.highlight_keys(|m| {
                        use ColorBy::*;
                        let color = *colors.index_cyc(match color_by {
                            Channel => instant.loc.channel as usize,
                            Track => instant.loc.track,
                            KeyColor => key_is_black!(key) as usize,
                        });
                        m.insert(key, color);
                    });
                } else {
                    item.highlight_keys(|m| {
                        m.remove(&key);
                    });
                }
            });
            tl.play(i_keyboard.show());
        }
    });

    // note animations
    for (range, note) in song.notes() {
        let Range { start, end } = range;
        let MultiTrackMidiNote {
            loc: MultiTrackLoc { track, channel },
            key,
            vel,
        } = note;

        let t_start = start as f64 / 1e9 + buf_time[0];
        let duration = (end - start) as f64 / 1e9;

        let color = {
            use ColorBy::*;
            *colors.index_cyc(match color_by {
                Channel => channel as usize,
                Track => track,
                KeyColor => key_is_black!(key) as usize,
            })
        };

        r.insert_with(|tl| {
            tl.forward_to(t_start);
            i_keyboard_tem.anim_note(
                tl,
                |item| {
                    item.set_fill_color(color.with_alpha(vel as f32 / 127.))
                        .set_stroke_color(AlphaColor::TRANSPARENT);
                    item.stroke_width = 0.;
                },
                key,
                duration,
                scroll_speed,
                scroll_height,
            );
            tl.hide();
        });
    }

    // pedals animation
    r.insert_with(|tl| {
        let mut i_pedals = i_pedals_tem.clone();
        tl.play(i_pedals.show()).forward(buf_time[0] + scroll_time);

        for instant in song.pedals() {
            let MultiTrackPedalInstant {
                // loc: MultiTrackLoc { track, channel },
                pedal_type,
                value,
                time,
                ..
            } = instant;
            tl.forward_to(time as f64 / 1e9 + buf_time[0] + scroll_time)
                .play(i_pedals.hide());
            i_pedals = i_pedals.with(|item| {
                item.set_pedal_status(pedal_type, value);
            });
            tl.play(i_pedals.show());
        }
    });
}

pub fn render_midi_visualizer(
    song: Arc<MidiMusic>,
    name: String,
    visualizer_config: &MidiVisualizerConfig,
    scene_config: &SceneConfig,
    output: &Output,
    buffer_count: usize,
) {
    let video_size = (output.width, output.height);
    let constructor = |r: &mut RanimScene| {
        midi_visualizer_scene(r, song.clone(), visualizer_config, video_size);
    };
    render_scene_output(constructor, name, scene_config, output, buffer_count);
}

//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
//////////////////////////////////////////////////
///////////////////////////////////////////
