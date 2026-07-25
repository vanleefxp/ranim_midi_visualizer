pub mod config;
mod note_anim;
use config::*;

use std::{num::NonZero, ops::Range};

use itertools::Itertools as _;
use music_utils::is_black_key;
use ranim::{
    Output, SceneConfig,
    anims::{func::Func, morph::MorphAnim},
    cmd::{preview::Resolution, render::render_scene_output},
    core::animation::{Eval, StaticAnim as _},
    glam::{DVec3, dvec2, dvec3},
    items::vitem::{
        geometry::{Rectangle, anchor::Origin},
        text::TextItem,
    },
    prelude::*,
    utils::rate_functions::linear,
};
use ranim_midi_visualizer_math::cyc_index::IndexCyc as _;

use ranim_music::items::{Pedal, PianoKeyboard, PianoPedals};
use typed_floats::tf64;
use waveform_utils::music::{
    ControlContainer as _, Metric, Music, Note, NoteContainer as _, NoteInstant,
};

use crate::note_anim::{anim_note_by_beat, anim_note_by_time};

pub fn midi_visualizer_scene(
    r: &mut RanimScene,
    song: &Music,
    config: &MidiVisualizerConfig,
    resolution: Resolution,
) {
    let cam = CameraFrame::default();
    r.insert(cam.clone());

    let beat_resolution = song.as_unmapped().resolution;
    let time_resolution = song.time_resolution;
    let &MidiVisualizerConfig {
        metric_base,
        scroll_speed,
        buf_time,
        status_bar_config:
            StatusBarConfig {
                em_size: font_size,
                padding,
                ..
            },
        time_window,
        note_config:
            NoteConfig {
                ref colors,
                color_by,
                h_scale,
            },
        ..
    } = config;
    let time_window =
        NonZero::try_from((f64::from(time_window) * time_resolution.get() as f64) as u64).unwrap();
    let font = config.text_font.clone();

    let frame_height = cam.frame_height;
    let frame_width = frame_height * resolution.width as f64 / resolution.height as f64;
    let frame_rx = frame_width / 2.;
    let frame_ry = frame_height / 2.;
    let frame_bottom_left = dvec3(-frame_rx, -frame_ry, 0.);
    let frame_bottom_right = dvec3(frame_rx, -frame_ry, 0.);
    let frame_top_left = dvec3(-frame_width / 2., frame_height / 2., 0.);
    let progress_bar_height = config.progress_bar_config.height;
    let progress_bar_min = frame_top_left - DVec3::Y * progress_bar_height;
    let status_bar_height = config.status_bar_config.height();

    // Static Items
    //
    r.insert_with(|tl| {
        // Bottom rect for status bar
        let i_status_bar_rect =
            Rectangle::from_min_size(frame_bottom_left, dvec2(frame_width, status_bar_height))
                .with(|item| {
                    item.set_color(config.status_bar_config.bg_color)
                        .set_stroke_opacity(0.)
                        .shift(DVec3::NEG_Z * 1e-4)
                        .discard()
                });
        // top rect for progress bar
        let i_progress_bar_rect =
            Rectangle::from_min_size(progress_bar_min, dvec2(frame_width, progress_bar_height))
                .with(|item| {
                    item.set_fill_color(config.progress_bar_config.bg_color)
                        .set_stroke_opacity(0.)
                        .shift(DVec3::Z * 1e-4)
                        .discard()
                });
        tl.play([i_status_bar_rect, i_progress_bar_rect].show());
    });

    // a template of the piano keyboard item
    // in the animation this item will be cloned with highlighted keys altered
    let (i_keyboard_tem, keyboard_height) = {
        // the keyboard width should fill the screen width
        let Range {
            start: rel_left,
            end: rel_right,
        } = config.keyboard_config.width_range(false);
        let size_unit = frame_width / (rel_right - rel_left);
        let keyboard_height = config.keyboard_config.size.white_height * size_unit;

        // The keyboard's origin is where the middle C key's top left corner is located
        let keyboard_origin = frame_bottom_left
            + dvec3(
                -rel_left * size_unit,
                status_bar_height + keyboard_height,
                0.,
            );

        (
            PianoKeyboard::new(config.keyboard_config.clone(), keyboard_origin, size_unit),
            keyboard_height,
        )
    };

    // pedals on the bottom-right corner of the remaining space
    let i_pedals_tem = PianoPedals::default().with(|item| {
        item.move_anchor_to(
            AabbPoint(dvec3(1., -1., 0.)),
            frame_bottom_right
                + DVec3::Y * (status_bar_height + keyboard_height)
                + dvec3(-0.2, 0.2, 1e-4),
        )
        .discard()
    });

    let scroll_height =
        tf64::PositiveFinite::try_from(frame_height - i_keyboard_tem.aabb_size().y).unwrap();
    let scroll_time = match metric_base {
        MetricBase::Beat => {
            let height_in_beats = scroll_height / scroll_speed;
            let height_in_ticks =
                (beat_resolution.get() as f64 * f64::from(height_in_beats)) as i64;
            let scroll_time_units = -song.time_map().eval(&-height_in_ticks, true);
            scroll_time_units as f64 / time_resolution.get() as f64
        }
        MetricBase::Time => f64::from(scroll_height / scroll_speed),
    };
    let duration = song.duration() as f64 / time_resolution.get() as f64;

    let to_seconds = |time: Metric| time as f64 / time_resolution.get() as f64;
    let to_scene_time =
        |midi_time: Metric| to_seconds(midi_time) + f64::from(buf_time[0]) + scroll_time;

    // let instants = song.instants().collect::<Vec<_>>();
    let text_origin = |n_columns: usize, column: usize| {
        let available_width = frame_width - padding[0].x - padding[1].x;
        let dx = available_width / n_columns as f64 * column as f64 + padding[0].x;
        let dy = padding[0].y;
        frame_bottom_left + dvec3(dx, dy, 1e-4)
    };

    // Progress Bar
    //
    r.insert_with(|tl| {
        let progress_bar_setup = |item: &mut Rectangle| {
            item.set_fill_color(config.progress_bar_config.fg_color)
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
        tl.forward_to(to_scene_time(0)).play(
            i_progress_bar
                .morph_to(i_progress_bar_final)
                .with_duration(duration)
                .with_rate_func(linear),
        );
    });

    // Timer
    //
    r.insert_with(|tl| {
        let origin = text_origin(4, 0);
        let font = font.clone();
        let create_timer_text = move |time: f64| {
            let nano = (time * 1e9) as u64;
            let (nano, sec) = (nano % 1_000_000_000, nano / 1_000_000_000);
            let micro = nano / 1_000_000;
            let (sec, min) = (sec % 60, sec / 60);
            let (min, hour) = (min % 60, min / 60);
            let hour = hour % 100;

            let src = format!("TIME {hour:02}:{min:02}:{sec:02}.{micro:03}");

            TextItem::new(src, font_size)
                .with_font(font.clone())
                .with(|item| item.move_anchor_to(Origin, origin).discard())
        };

        let create_timer_text_cloned = create_timer_text.clone();
        let timer_anim = Func::new(create_timer_text(0.), move |_, t| {
            let time = t * duration;
            create_timer_text_cloned(time)
        });

        tl.play(create_timer_text(0.).show())
            .forward_to(to_scene_time(0))
            .play(
                timer_anim
                    .into_animation_cell()
                    .with_duration(duration)
                    .with_rate_func(linear),
            )
            .play(create_timer_text(duration).show())
            .forward(buf_time[1].into());
    });

    // Note Count
    //
    r.insert_with(|tl| {
        let origin = text_origin(4, 1);
        let note_count_total = song.as_unmapped().note_count();
        let create_note_count_text = |n: usize| {
            let src = format!("NOTE COUNT {n} / {note_count_total}");
            TextItem::new(src, font_size)
                .with_font(font.clone())
                .with(|item| item.move_anchor_to(Origin, origin).discard())
        };

        let mut i_note_count = create_note_count_text(0);
        tl.play(i_note_count.show());

        for (time, note_count) in song
            .as_mapped()
            .note_count_iter()
            .map(|(time, note_count)| (to_scene_time(time), note_count))
        {
            tl.forward_to(time).play(i_note_count.hide());
            i_note_count = create_note_count_text(note_count);
            tl.play(i_note_count.show());
        }
    });

    // Note Per Second
    r.insert_with(|tl| {
        let origin = text_origin(4, 2);

        let note_rate_to_nps = |note_rate: usize| {
            note_rate as f64 * time_resolution.get() as f64 / time_window.get() as f64
        };
        let create_nps_text = |note_rate: usize, note_rate_max: usize| {
            let nps = note_rate_to_nps(note_rate);
            let nps_max = note_rate_to_nps(note_rate_max);
            TextItem::new(format!("NPS (MAX) {nps:.0} ({nps_max:.0})"), font_size)
                .with_font(font.clone())
                .with(|item| item.move_anchor_to(Origin, origin).discard())
        };

        let mut note_rate_max = 0;
        let mut i_nps_text = create_nps_text(0, 0);
        tl.play(i_nps_text.show());
        for (time, nps) in song
            .as_mapped()
            .note_rate_iter(time_window)
            .map(|(time, nps)| (to_scene_time(time), nps))
        {
            note_rate_max = nps.max(note_rate_max);
            tl.forward_to(time).play(i_nps_text.hide());
            i_nps_text = create_nps_text(nps, note_rate_max);
            tl.play(i_nps_text.show());
        }
    });

    // Legato Index
    r.insert_with(|tl| {
        let legato_score_fn = song.as_mapped().legato_fn(time_window);
        let origin = text_origin(4, 3);

        // font and font size are config variables
        // so clone them to move them into the closure
        let font = font.clone();
        let create_legato_text = move |legato_index: f64| {
            TextItem::new(format!("LEGATO {:.3}", legato_index), font_size)
                .with_font(font.clone())
                .with(|item| item.move_anchor_to(Origin, origin).discard())
        };

        let i_text = create_legato_text(0.);
        tl.play(i_text.show());
        if let Some((&t0, _)) = legato_score_fn.iter().next() {
            // value before `t0` should be 0.
            // because no note is in the window
            tl.forward_to(to_scene_time(t0)).play(i_text.hide());
            for ((_, &v1), (&t2, &v2)) in legato_score_fn.iter().tuple_windows() {
                // clone values so that they can be moved into the closure
                let create_legato_text = create_legato_text.clone();

                let i_text = create_legato_text(v1);
                let anim = Func::new(i_text, move |_, t| create_legato_text(v1.lerp(&v2, t)));

                tl.play(
                    anim.into_animation_cell()
                        // duration calculated by the desired end time minus the current time
                        // to avoid float accumulation error
                        .with_duration(to_scene_time(t2) - tl.cur_sec())
                        .with_rate_func(linear),
                );
            }
            // value after last note's end passing the window should also be 0.
            tl.play(i_text.show());
        }
    });

    // keyboard animation
    r.insert_with(|tl| {
        let mut i_keyboard = i_keyboard_tem.clone();
        tl.play(i_keyboard.show()).forward(to_scene_time(0));

        for ([staff_idx, voice_idx], ep) in song.as_mapped().note_instants() {
            let NoteInstant {
                is_end,
                at: time,
                pair: (_, note),
            } = ep;

            tl.forward_to(to_scene_time(time));
            tl.play(i_keyboard.hide());
            i_keyboard = i_keyboard.with(|item| {
                let key = note.pitch;

                if is_end {
                    item.highlight_keys(|m| {
                        m.remove(&key);
                    });
                } else {
                    item.highlight_keys(|m| {
                        use ColorBy::*;
                        let color = *colors.index_cyc(match color_by {
                            Voice => voice_idx,
                            Staff => staff_idx,
                            KeyColor => is_black_key(key) as usize,
                        });
                        m.insert(key, color);
                    });
                }
            });
            tl.play(i_keyboard.show());
        }
    });

    // note animations

    for ([staff_idx, voice_idx], tick_range, &Note { pitch, velocity }) in
        song.as_unmapped().notes_by_start()
    {
        use ColorBy::*;
        use MetricBase::*;

        let is_black = is_black_key(pitch);
        let color = {
            *colors.index_cyc(match color_by {
                Voice => voice_idx,
                Staff => staff_idx,
                KeyColor => is_black as usize,
            })
        };

        let note_setup = move |item: &mut Rectangle| {
            item.set_fill_color(color.with_alpha(f64::from(velocity) as f32))
                .set_stroke_opacity(0.);
            let pos = AabbPoint::CENTER.locate(item);
            let scale_factor = h_scale[is_black as usize];
            let scale = dvec3(scale_factor.into(), 1., 1.);
            item.move_to(DVec3::ZERO).scale(scale).move_to(pos);
        };

        match metric_base {
            Time => {
                let Range {
                    start: start_tick,
                    end: end_tick,
                } = tick_range;
                let start_time_unit = song.time_map().eval(&start_tick, true);
                let end_time_unit = song.time_map().eval(&end_tick, true);
                r.insert_with(|tl| {
                    tl.forward_to(buf_time[0].into());
                    anim_note_by_time(
                        tl,
                        &i_keyboard_tem,
                        note_setup,
                        pitch,
                        start_time_unit..end_time_unit,
                        time_resolution,
                        scroll_speed.into(),
                        scroll_height.into(),
                    );
                });
            }
            Beat => {
                r.insert_with(|tl| {
                    tl.forward_to(buf_time[0].into());
                    anim_note_by_beat(
                        tl,
                        &i_keyboard_tem,
                        note_setup,
                        pitch,
                        tick_range.clone(),
                        beat_resolution,
                        time_resolution,
                        &song.time_map(),
                        scroll_speed.into(),
                        scroll_height.into(),
                    );
                });
            }
        }
    }

    // Pedals
    r.insert_with(|tl| {
        let mut i_pedals = i_pedals_tem.clone();
        tl.play(i_pedals.show()).forward(to_scene_time(0));

        for (_, time, control) in song.as_mapped().controls() {
            let pedal_type = Pedal::try_from(control.pedal as u8).expect("should be successful");
            tl.forward_to(to_scene_time(time)).play(i_pedals.hide());
            i_pedals = i_pedals.with(|item| {
                item.set_pedal_status(pedal_type, control.depth.into());
            });
            tl.play(i_pedals.show());
        }
    });
}

pub fn render_midi_visualizer(
    song: &Music,
    name: &str,
    visualizer_config: &MidiVisualizerConfig,
    scene_config: &SceneConfig,
    output: &Output,
    buffer_count: usize,
) {
    let resolution = Resolution::new(output.width, output.height);
    let constructor = |r: &mut RanimScene| {
        midi_visualizer_scene(r, song, visualizer_config, resolution);
    };
    render_scene_output(
        constructor,
        name.to_string(),
        scene_config,
        output,
        buffer_count,
    );
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
