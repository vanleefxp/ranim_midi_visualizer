#![feature(range_into_bounds)]

pub mod anim;
pub mod cyc_index;
pub mod items;
pub mod midi;
pub mod stroke_and_fill;

use std::{ops::Range, sync::Arc};

use crate::{
    cyc_index::IndexCyc as _,
    items::{PianoKeyboard, PianoKeyboardSize, PianoPedals},
    midi::{MidiMusic, MultiTrackLoc, MultiTrackMidiNote, MultiTrackPedalInstant},
    stroke_and_fill::StrokeAndFill,
};
use ranim::{
    Output, SceneConfig,
    cmd::render::render_scene_output,
    color::{
        AlphaColor, Srgb,
        palette::css::{BLACK, TRANSPARENT, WHITE},
    },
    core::{
        animation::{Eval, StaticAnim as _},
        components::width::Width,
    },
    glam::{DVec2, DVec3, dvec2, dvec3},
    items::vitem::{geometry::Rectangle, svg::SvgItem, typst::typst_svg},
    prelude::*,
    utils::rate_functions::linear,
};

const TYPST_TEMPLATE: &str = include_str!("assets/template.typ");

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ColorBy {
    Channel,
    Track,
    KeyColor,
}

impl Default for ColorBy {
    fn default() -> Self {
        Self::Channel
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StatusBarSize {
    pub font_size: f64,
    pub padding: DVec2,
}

impl Default for StatusBarSize {
    fn default() -> Self {
        Self {
            font_size: 0.15,
            padding: dvec2(0.1, 0.1),
        }
    }
}

#[derive(Clone, Debug)]
// #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
// #[non_exhaustive]
pub struct MidiVisualizerConfig {
    pub colors: Vec<AlphaColor<Srgb>>,
    pub scroll_speed: f64,
    pub color_by: ColorBy,
    pub buf_time: f64,
    pub keyboard_size: PianoKeyboardSize,
    pub key_range: Range<u8>,
    pub status_bar_size: StatusBarSize,
    pub window_size: f64,
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
            buf_time: 2.,
            keyboard_size: Default::default(),
            key_range: 21..109,
            status_bar_size: Default::default(),
            window_size: 1.,
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
    font_size: f64,
    duration: f64,
}

impl Eval<SvgItem> for TimerTextAnim {
    fn eval_alpha(&self, alpha: f64) -> SvgItem {
        let &Self {
            origin,
            font_size,
            duration,
        } = self;

        let time = alpha * duration;
        let nano = (time * 1e9) as u64;
        let (nano, sec) = (nano % 1000000000, nano / 1000000000);
        let micro = nano % 1000;
        let (sec, min) = (sec % 60, sec / 60);
        let (min, hour) = (min % 60, min / 60);
        let hour = hour % 100;

        let time_src = format!("{hour:02}:{min:02}:{sec:02}.{micro:03}");
        let src = TYPST_TEMPLATE.to_string() + "TIME " + &time_src;

        SvgItem::new(typst_svg(&src)).with(|item| {
            item.scale_to(ScaleHint::PorportionalY(font_size))
                .set_color(WHITE);
            let text_bottom_left = item.aabb()[0];
            let disp = origin - text_bottom_left;
            item.shift(disp);
        })
    }
}

struct NPSTextAnim {
    origin: DVec3,
    font_size: f64,
    window_size: f64,
    music: Arc<MidiMusic>,
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

    fn create_item(&self, nps: f64) -> SvgItem {
        let &Self {
            origin, font_size, ..
        } = self;
        let nps_src = format!("{:.0}", nps);
        let src = TYPST_TEMPLATE.to_string() + "NPS " + &nps_src;
        SvgItem::new(typst_svg(&src)).with(|item| {
            item.scale_to(ScaleHint::PorportionalY(font_size))
                .set_color(WHITE);
            let text_bottom_left = item.aabb()[0];
            let disp = origin - text_bottom_left;
            item.shift(disp);
        })
    }
}

impl Eval<SvgItem> for NPSTextAnim {
    fn eval_alpha(&self, alpha: f64) -> SvgItem {
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

    fn create_item(&self, legato_index: f64) -> SvgItem {
        let &Self {
            origin, font_size, ..
        } = self;
        let nps_src = format!("{:.3}", legato_index);
        let src = TYPST_TEMPLATE.to_string() + "LEGATO " + &nps_src;
        SvgItem::new(typst_svg(&src)).with(|item| {
            item.scale_to(ScaleHint::PorportionalY(font_size))
                .set_color(WHITE);
            let text_bottom_left = item.aabb()[0];
            let disp = origin - text_bottom_left;
            item.shift(disp);
        })
    }
}

impl Eval<SvgItem> for LegatoTextAnim {
    fn eval_alpha(&self, alpha: f64) -> SvgItem {
        let legato_index = self.calc_value(alpha);
        let item = self.create_item(legato_index);
        item
    }
}

fn midi_visualizer_scene(
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
        status_bar_size: StatusBarSize { font_size, padding },
        window_size,
        ..
    } = config;
    let colors = &config.colors;

    let frame_height = cam.frame_height;
    let frame_width = frame_height * video_size.0 as f64 / video_size.1 as f64;
    let frame_bottom_left = dvec3(-frame_width / 2., -frame_height / 2., 0.);
    let status_bar_height = font_size + padding.y * 2.;

    // status bar rect
    {
        let r_status_bar = r.insert_empty();
        let tl = r.timeline_mut(r_status_bar);
        let i_status_bar_rect =
            Rectangle::from_min_size(frame_bottom_left, dvec2(frame_width, status_bar_height))
                .with(|item| {
                    item.set_color(BLACK.with_alpha(0.5)).set_stroke_opacity(0.);
                });
        tl.play(i_status_bar_rect.show());
    }

    let i_keyboard_tem = PianoKeyboard::default().with(|item| {
        item.set_size(|size| *size = config.keyboard_size)
            .set_key_range(config.key_range.clone());

        let width = item.aabb_size().x;
        let scale_factor = frame_width / width;
        item.scale(DVec3::splat(scale_factor));
        let [min, _] = item.aabb();

        item.shift(frame_bottom_left - min + dvec3(0., status_bar_height, 0.));
    });
    let i_pedals_tem = PianoPedals::default().with(|item| {
        item.move_anchor_to(
            AabbPoint(dvec3(1., -1., 0.)),
            i_keyboard_tem.aabb()[1] + dvec3(-0.2, 0.2, 1e-4),
        );
    });

    let scroll_height = frame_height - i_keyboard_tem.aabb_size().y;
    let scroll_time = scroll_height / scroll_speed;
    let duration = song.duration() as f64 / 1e9;

    let instants = song.instants().collect::<Vec<_>>();
    let text_origin = |n_columns: usize, column: usize| {
        let available_width = frame_width - (n_columns + 1) as f64 * padding.x;
        let dx = available_width / n_columns as f64 * column as f64 + padding.x;
        let dy = padding.y;
        frame_bottom_left + dvec3(dx, dy, 1e-4)
    };

    // timer
    {
        let origin = text_origin(4, 0);
        let r_timer = r.insert_empty();
        let tl = r.timeline_mut(r_timer);
        let timer_anim = TimerTextAnim {
            origin,
            font_size,
            duration,
        };
        let i_timer_zero = timer_anim.eval_alpha(0.);
        let i_timer_final = timer_anim.eval_alpha(1.);
        tl.play(i_timer_zero.show())
            .forward_to(buf_time + scroll_time)
            .play(i_timer_zero.hide())
            .play(
                timer_anim
                    .into_animation_cell()
                    .with_duration(duration)
                    .with_rate_func(linear),
            )
            .play(i_timer_final.show())
            .forward(buf_time);
    }

    // note count
    {
        let origin = text_origin(4, 1);
        let create_note_count_text = |n: usize| {
            let mut src = TYPST_TEMPLATE.to_string();
            src.push_str("NOTE COUNT ");
            src.push_str(itoa::Buffer::new().format(n));
            SvgItem::new(typst_svg(&src)).with(|item| {
                item.scale_to(ScaleHint::PorportionalY(font_size))
                    .set_color(WHITE);
                let text_bottom_left = item.aabb()[0];
                let disp = origin - text_bottom_left;
                item.shift(disp);
            })
        };

        let mut note_count = 0usize;
        let mut i_note_count = create_note_count_text(note_count);
        let r_note_count = r.insert_empty();
        let tl = r.timeline_mut(r_note_count);
        tl.play(i_note_count.show()).forward(buf_time + scroll_time);

        for instant in instants.iter().filter(|instant| instant.is_start()) {
            tl.forward_to(instant.time as f64 / 1e9 + buf_time + scroll_time);
            note_count += 1;
            tl.play(i_note_count.hide());
            i_note_count = create_note_count_text(note_count);
            tl.play(i_note_count.show());
        }
    }

    // note per second
    {
        let origin = text_origin(4, 2);
        let r_nps = r.insert_empty();
        let tl = r.timeline_mut(r_nps);
        let anim = NPSTextAnim {
            origin,
            font_size,
            window_size,
            music: song.clone(),
        };
        let i_nps_zero = anim.create_item(0.);
        tl.play(i_nps_zero.show())
            .forward_to(buf_time + scroll_time)
            .play(i_nps_zero.hide())
            .play(
                anim.into_animation_cell()
                    .with_duration(duration + window_size),
            )
            .play(i_nps_zero.show());
    }

    // legato index
    {
        let origin = text_origin(4, 3);
        let r_legato = r.insert_empty();
        let tl = r.timeline_mut(r_legato);
        let anim = LegatoTextAnim {
            origin,
            font_size,
            window_size,
            music: song.clone(),
        };
        let i_legato_zero = anim.create_item(0.);
        tl.play(i_legato_zero.show())
            .forward_to(buf_time + scroll_time)
            .play(i_legato_zero.hide())
            .play(
                anim.into_animation_cell()
                    .with_duration(duration + window_size),
            )
            .play(i_legato_zero.show());
    }

    // keyboard animation
    {
        let mut i_keyboard = i_keyboard_tem.clone();
        let r_keyboard = r.insert_empty();
        let tl = r.timeline_mut(r_keyboard);
        tl.play(i_keyboard.show()).forward(buf_time + scroll_time);

        for instant in instants.iter() {
            tl.forward_to(instant.time as f64 / 1e9 + buf_time + scroll_time);
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
    }

    // note animations
    {
        for (range, note) in song.notes() {
            let r_note = r.insert_empty();
            let tl = r.timeline_mut(r_note);

            let Range { start, end } = range;
            let MultiTrackMidiNote {
                loc: MultiTrackLoc { track, channel },
                key,
                vel,
            } = note;

            let t_start = start as f64 / 1e9 + buf_time;
            let duration = (end - start) as f64 / 1e9;

            let color = {
                use ColorBy::*;
                *colors.index_cyc(match color_by {
                    Channel => channel as usize,
                    Track => track,
                    KeyColor => key_is_black!(key) as usize,
                })
            };
            let stroke_and_fill = StrokeAndFill {
                fill_rgba: color.with_alpha(vel as f32 / 127.),
                stroke_rgba: TRANSPARENT,
                stroke_width: Width(0.),
            };
            tl.forward_to(t_start)
                .play(i_keyboard_tem.anim_note(
                    key,
                    duration,
                    scroll_speed,
                    scroll_height,
                    stroke_and_fill,
                ))
                .hide();
        }
    }

    // pedals animation
    {
        let r_pedals = r.insert_empty();
        let tl = r.timeline_mut(r_pedals);
        let mut i_pedals = i_pedals_tem.clone();
        tl.play(i_pedals.show()).forward(buf_time + scroll_time);

        for instant in song.pedals() {
            let MultiTrackPedalInstant {
                // loc: MultiTrackLoc { track, channel },
                pedal_type,
                value,
                time,
                ..
            } = instant;
            tl.forward_to(time as f64 / 1e9 + buf_time + scroll_time)
                .play(i_pedals.hide());
            i_pedals = i_pedals.with(|item| {
                item.set_pedal_status(pedal_type, value);
            });
            tl.play(i_pedals.show());
        }
    }
}

pub fn render_midi_visualizer(
    song: Arc<MidiMusic>,
    name: String,
    visualizer_config: &MidiVisualizerConfig,
    scene_config: &SceneConfig,
    output: &Output,
) {
    let video_size = (output.width, output.height);
    let constructor = |r: &mut RanimScene| {
        midi_visualizer_scene(r, song.clone(), visualizer_config, video_size);
    };
    render_scene_output(constructor, name, scene_config, output);
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
