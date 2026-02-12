#![feature(iter_next_chunk, iterator_try_collect)]

use std::{path::PathBuf, sync::Arc};

use anyhow::{Result, anyhow, bail};
use clap::{arg, command};
use ranim::{Output, OutputFormat, SceneConfig, color::try_color};
use ranim_midi_visualizer_lib::{
    ColorBy, MidiVisualizerConfig, midi::MidiMusic, render_midi_visualizer,
};
use uncased::AsUncased;

fn main() -> Result<()> {
    use tracing::level_filters::LevelFilter;
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    fn build_filter() -> EnvFilter {
        const DEFAULT_DIRECTIVES: &[(&str, LevelFilter)] = &[
            ("ranim_midi_visualizer_lib", LevelFilter::INFO),
            ("ranim_midi_visualizer", LevelFilter::INFO),
            ("ranim_cli", LevelFilter::INFO),
            ("ranim", LevelFilter::INFO),
        ];
        let mut filter = EnvFilter::from_default_env();
        let env = std::env::var("RUST_LOG").unwrap_or_default();
        for (name, level) in DEFAULT_DIRECTIVES
            .iter()
            .filter(|(name, _)| !env.contains(name))
        {
            filter = filter.add_directive(format!("{name}={level}").parse().unwrap());
        }
        filter
    }

    let indicatif_layer = tracing_indicatif::IndicatifLayer::new();

    tracing_subscriber::registry()
        .with(fmt::layer().with_writer(indicatif_layer.get_stderr_writer()))
        .with(indicatif_layer)
        .with(build_filter())
        .init();

    let matches = command!()
    .arg(arg!(midi_file:  <INFILE> "Input midi file to render.").required(true))
    .arg(arg!(-f --format <VALUE> "Output video format. Supported values are: mp4, mov, webm, gif.").default_value("mp4"))
    .arg(arg!(--fps <INT> "Frame rate of the output.").default_value("60"))
    .arg(arg!(-s --size <WIDTH> <HEIGHT> "Output video size.").value_names(["WIDTH", "HEIGHT"]).default_values(["1920", "1080"]))
    .arg(arg!(clear_color: --bg <COLOR> "Background color. In any supported CSS color format.").default_value("#282c34"))
    .arg(arg!(note_colors: --fg <COLOR>).default_values(["#89b9eb", "#9be347", "#f7931e", "#f7c71e"]).num_args(1..))
    .arg(arg!(--color_by <VALUE> "How note colors are assigned to different notes. Supported values are: channel, track, key_color.").default_value("channel"))
    .arg(arg!(--scroll_speed <FLOAT> "Note scroll speed in coordinate units per second. By default the screen height is 8 coordinate units.").default_value("2"))
    .arg(arg!(--buf_time <FLOAT> "Additional time before and after playing the song.").default_value("2"))
    .get_matches();

    let midi_path = PathBuf::from(
        matches
            .get_one::<String>("midi_file")
            .map(|v| v.as_str())
            .ok_or_else(|| anyhow!("invalid input"))?,
    );
    let src = std::fs::read(&midi_path)?;
    let music = MidiMusic::try_from(&src[..])?;

    let [width, height] = matches
        .get_many::<String>("size")
        .unwrap()
        .filter_map(|src| u32::from_str_radix(src, 10).ok())
        .next_chunk()
        .map_err(|_| anyhow!("invalid size"))?;
    let format = {
        use OutputFormat::*;
        let format = matches.get_one::<String>("format").unwrap().as_uncased();
        if format == "mp4" {
            Mp4
        } else if format == "mov" {
            Mov
        } else if format == "webm" {
            Webm
        } else if format == "gif" {
            Gif
        } else {
            bail!("Invalid output format: {format}")
        }
    };

    let output = Output {
        fps: matches.get_one::<String>("fps").unwrap().parse()?,
        dir: ".".to_string(),
        width,
        height,
        format,
        save_frames: false,
    };

    let clear_color = matches.get_one::<String>("clear_color").unwrap().clone();
    let scene_config = SceneConfig { clear_color };

    let note_colors = matches
        .get_many::<String>("note_colors")
        .unwrap()
        .map(|v| try_color(v.as_str()).ok())
        .try_collect::<Vec<_>>()
        .ok_or_else(|| anyhow!("invalid color"))?;
    let color_by = {
        use ColorBy::*;
        let color_by = matches.get_one::<String>("color_by").unwrap();
        if color_by == "channel" {
            Channel
        } else if color_by == "track" {
            Track
        } else if color_by == "key_color" {
            KeyColor
        } else {
            bail!("Invalid color_by value: {color_by}");
        }
    };
    let visualizer_config = MidiVisualizerConfig {
        scroll_speed: matches.get_one::<String>("scroll_speed").unwrap().parse()?,
        buf_time: matches.get_one::<String>("buf_time").unwrap().parse()?,
        colors: note_colors,
        color_by,
        ..Default::default()
    };
    let name = midi_path
        .file_stem()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_default();
    render_midi_visualizer(
        Arc::new(music),
        name,
        &visualizer_config,
        &scene_config,
        &output,
    );
    Ok(())
}
