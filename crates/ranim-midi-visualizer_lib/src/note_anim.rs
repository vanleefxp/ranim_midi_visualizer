#![allow(clippy::too_many_arguments)]

use std::{iter, ops::Range};

use derive_more::{Deref, DerefMut, From};
use itertools::Itertools as _;
use music_utils::is_black_key;
use ranim::{
    core::{
        animation::{Eval, StaticAnim as _},
        timeline::{Timeline, TimelineFunc as _},
        traits::{Interpolatable as _, Locate as _, With as _},
    },
    glam::{DVec3, dvec2},
    items::vitem::geometry::Rectangle,
    utils::rate_functions::linear,
};
use ranim_music::items::{PianoKeyboard, Tone};
use tracing::debug;
use waveform_utils::music::{FrameRate, Metric, TimeMap};

#[derive(Debug, Clone, Copy, Deref, DerefMut, From)]
pub struct MyAnim<T, F: Fn(f64) -> T>(pub F);

impl<T, F: Fn(f64) -> T> Eval<T> for MyAnim<T, F> {
    fn eval_alpha(&self, alpha: f64) -> T {
        (self.0)(alpha)
    }
}

pub fn anim_note_by_time(
    tl: &mut Timeline,
    keyboard: &PianoKeyboard,
    note_setup: impl 'static + Fn(&mut Rectangle) + Clone,
    pitch: i8,
    time_unit_range: Range<Metric>,
    time_resolution: FrameRate,
    scroll_speed: f64,
    scroll_height: f64,
) {
    let time_unit_to_time = |time_unit: Metric| time_unit as f64 / time_resolution.get() as f64;
    let time_to_time_unit = |time: f64| (time * time_resolution.get() as f64) as Metric;

    let origin = Tone(pitch).locate(keyboard);
    let top = origin + DVec3::Y * scroll_height;
    let key_width = keyboard.size_unit()
        * if is_black_key(pitch) {
            keyboard.size().black_size.x
        } else {
            1.
        };

    let Range {
        start: start_time_unit,
        end: end_time_unit,
    } = time_unit_range;
    let duration_in_time_units = end_time_unit - start_time_unit;
    let start_time = time_unit_to_time(start_time_unit);
    let end_time = time_unit_to_time(end_time_unit);
    let duration = end_time - start_time;

    let scroll_time = scroll_height / scroll_speed;
    let scroll_time_units = time_to_time_unit(scroll_time);
    let t0 = tl.cur_sec() + scroll_time;

    let note_height = duration * scroll_speed;

    let time1 = start_time - scroll_time;
    let mut time2 = time1 + duration;
    let mut time3 = start_time;
    let time4 = time3 + duration;

    if duration_in_time_units > scroll_time_units {
        (time2, time3) = (time3, time2);
    }

    // Stage 1: note entering scroll area
    {
        tl.forward_to(t0 + time1);
        let anim_duration = t0 + time2 - tl.cur_sec();
        let note_setup = note_setup.clone();
        let anim = MyAnim(move |alpha| {
            let rect_height = scroll_speed * anim_duration * alpha;
            let rect_bottom_left = top - DVec3::Y * rect_height;
            Rectangle::from_min_size(rect_bottom_left, dvec2(key_width, rect_height))
                .with(&note_setup)
        });
        tl.play(
            anim.into_animation_cell()
                .with_duration(anim_duration)
                .with_rate_func(linear),
        );
    }

    if duration_in_time_units <= scroll_time_units
    // Case 1
    // Stage 2: note falling to the bottom of scroll area
    {
        let anim_duration = t0 + time3 - tl.cur_sec();
        let note_setup = note_setup.clone();
        let anim = MyAnim(move |alpha| {
            let rect_y_pos = (scroll_height - note_height) * (1. - alpha);
            let rect_bottom_left = origin + DVec3::Y * rect_y_pos;
            Rectangle::from_min_size(rect_bottom_left, dvec2(key_width, note_height))
                .with(&note_setup)
        });
        tl.play(
            anim.into_animation_cell()
                .with_duration(anim_duration)
                .with_rate_func(linear),
        );
    } else
    // Case 2
    // Stage 2: note occupying the full height of scroll area
    {
        let rect =
            Rectangle::from_min_size(origin, dvec2(key_width, scroll_height)).with(&note_setup);
        tl.play(rect.show())
            .forward_to(t0 + time3)
            .play(rect.hide());
    }

    // Stage 3: note leaving scroll area
    {
        let anim_duration = t0 + time4 - tl.cur_sec();
        let note_setup = note_setup.clone();
        let anim = MyAnim(move |alpha| {
            let rect_height = scroll_speed * anim_duration * (1. - alpha);
            Rectangle::from_min_size(origin, dvec2(key_width, rect_height)).with(&note_setup)
        });
        tl.play(
            anim.into_animation_cell()
                .with_duration(anim_duration)
                .with_rate_func(linear),
        );
    }

    tl.hide();
}

pub fn anim_note_by_beat(
    tl: &mut Timeline,
    keyboard: &PianoKeyboard,
    note_setup: impl 'static + Fn(&mut Rectangle) + Clone,
    pitch: i8,
    tick_range: Range<Metric>,
    beat_resolution: FrameRate,
    time_resolution: FrameRate,
    time_map: &TimeMap,
    scroll_speed: f64,
    scroll_height: f64,
) {
    let tick_to_beat = move |tick: Metric| tick as f64 / beat_resolution.get() as f64;
    let beat_to_tick = |beat: f64| (beat * beat_resolution.get() as f64) as Metric;
    let time_unit_to_time = |time_unit: Metric| time_unit as f64 / time_resolution.get() as f64;
    // let time_to_time_unit = |time: f64| (time * time_resolution.get() as f64) as Metric;

    let Range {
        start: start_tick,
        end: end_tick,
    } = tick_range;
    let start_beat = tick_to_beat(start_tick);
    let end_beat = tick_to_beat(end_tick);
    let duration_in_ticks = end_tick - start_tick;
    let duration_in_beats = end_beat - start_beat;

    let note_height = scroll_speed * duration_in_beats;
    debug!(
        "Height per beat: {}, Note height: {}, ",
        scroll_speed, note_height
    );

    let scroll_height_in_beats = scroll_height / scroll_speed;
    let scroll_height_in_ticks = beat_to_tick(scroll_height_in_beats);

    // Ensure that all `tl.forward_to()` calls will bring the timeline to a time point after the current moment
    // (i.e. the result of `tl.cur_sec()` here).
    // Notes need time to scroll from the screen top.
    let init_scroll_time_units = -time_map.eval(&-scroll_height_in_ticks, true);
    let init_scroll_time = time_unit_to_time(init_scroll_time_units);
    let t0 = tl.cur_sec() + init_scroll_time;

    let origin = Tone(pitch).locate(keyboard);
    let top = origin + DVec3::Y * scroll_height;
    let key_width = keyboard.size_unit()
        * if is_black_key(pitch) {
            keyboard.size().black_size.x
        } else {
            1.
        };

    // Case 1: Note bar shorter than scroll area
    //      [             ]
    //                     -----              tick1
    //                -----                   tick2
    //      -----                             tick3
    // -----                                  tick4

    // Case 2: note bar longer than scroll area
    //                [   ]
    //                     ---------------    tick1
    //                ---------------         tick2
    //      ---------------                   tick3
    // ---------------                        tick4

    let tick1 = start_tick - scroll_height_in_ticks;
    let mut tick2 = tick1 + duration_in_ticks;
    let mut tick3 = start_tick;
    let tick4 = tick3 + duration_in_ticks;

    if note_height > scroll_height {
        (tick2, tick3) = (tick3, tick2);
    }

    debug!(
        "Tick1: {}, Tick2: {}, Tick3: {}, Tick4: {}, ",
        tick1, tick2, tick3, tick4
    );

    let beat1 = tick_to_beat(tick1);
    // let beat2 = tick_to_beat(tick2);
    // let beat3 = tick_to_beat(tick3);
    let beat4 = tick_to_beat(tick4);

    let time_unit_1 = time_map.eval(&tick1, true);
    let time_unit_2 = time_map.eval(&tick2, true);
    let time_unit_3 = time_map.eval(&tick3, true);
    let time_unit_4 = time_map.eval(&tick4, true);

    let start_time = t0 + time_unit_to_time(time_unit_1);
    tl.forward_to(start_time);

    // Stage 1: note entering scroll area
    {
        let points = iter::once((tick1, time_unit_1))
            .chain(time_map.range(tick1..tick2).map(|(&v1, &v2)| (v1, v2)))
            .chain(iter::once((tick2, time_unit_2)));
        for ((begin_tick, _), (end_tick, end_time_unit)) in points.tuple_windows() {
            // calculate animation duration by subtraction due to possible floating point error
            let end_time = t0 + time_unit_to_time(end_time_unit);
            let anim_duration = end_time - tl.cur_sec();
            let note_setup = note_setup.clone();

            let anim = MyAnim(move |alpha| {
                let cur_beat = tick_to_beat(begin_tick).lerp(&tick_to_beat(end_tick), alpha);

                let dy = (cur_beat - beat1) * scroll_speed;
                let rect_bottom_left = top - dy * DVec3::Y;

                Rectangle::from_min_size(rect_bottom_left, dvec2(key_width, dy)).with(&note_setup)
            });

            tl.play(
                anim.into_animation_cell()
                    .with_duration(anim_duration)
                    .with_rate_func(linear),
            );
        }
    }

    if note_height <= scroll_height
    // Case 1
    // Stage 2: note falling to the bottom of scroll area
    {
        let points = iter::once((tick2, time_unit_2))
            .chain(time_map.range(tick2..tick3).map(|(&v1, &v2)| (v1, v2)))
            .chain(iter::once((tick3, time_unit_3)));

        for ((begin_tick, _), (end_tick, end_time_unit)) in points.tuple_windows() {
            let end_time = t0 + time_unit_to_time(end_time_unit);
            let anim_duration = end_time - tl.cur_sec();
            let note_setup = note_setup.clone();

            let anim = MyAnim(move |alpha| {
                let cur_beat = tick_to_beat(begin_tick).lerp(&tick_to_beat(end_tick), alpha);

                let dy = (cur_beat - beat1) * scroll_speed;
                let rect_bottom_left = top - dy * DVec3::Y;

                Rectangle::from_min_size(rect_bottom_left, dvec2(key_width, note_height))
                    .with(&note_setup)
            });

            tl.play(
                anim.into_animation_cell()
                    .with_duration(anim_duration)
                    .with_rate_func(linear),
            );
        }
    } else
    // Case 2
    // Stage 2: note occupying the full height of scroll area
    {
        let rect =
            Rectangle::from_min_size(origin, dvec2(key_width, scroll_height)).with(&note_setup);
        let end_time = t0 + time_unit_to_time(time_unit_3);
        tl.play(rect.show()).forward_to(end_time).play(rect.hide());
    }

    // Stage 3: note leaving scroll area
    {
        let points = iter::once((tick3, time_unit_3))
            .chain(time_map.range(tick3..tick4).map(|(&v1, &v2)| (v1, v2)))
            .chain(iter::once((tick4, time_unit_4)));

        for ((begin_tick, _), (end_tick, end_time_unit)) in points.tuple_windows() {
            let end_time = time_unit_to_time(end_time_unit);
            let anim_duration = t0 + end_time - tl.cur_sec();
            let note_setup = note_setup.clone();

            let anim = MyAnim(move |alpha| {
                let cur_beat = tick_to_beat(begin_tick).lerp(&tick_to_beat(end_tick), alpha);
                let rect_height = (beat4 - cur_beat) * scroll_speed;
                Rectangle::from_min_size(origin, dvec2(key_width, rect_height)).with(&note_setup)
            });

            tl.play(
                anim.into_animation_cell()
                    .with_duration(anim_duration)
                    .with_rate_func(linear),
            );
        }
    }

    tl.hide();
}
