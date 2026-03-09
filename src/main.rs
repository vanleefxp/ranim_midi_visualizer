use std::{path::PathBuf, sync::Arc};

use anyhow::{Ok, Result, anyhow, bail};
use clap::{ArgMatches, Command, arg, command};
use phf::phf_map;
use ranim::{
    cmd::preview::{RanimPreviewApp, run_app},
    {Output, OutputFormat, RanimScene, SceneConfig, color::try_color},
};
use uncased::{AsUncased, UncasedStr};

use ranim_midi_visualizer_lib::{
    ColorBy, MidiVisualizerConfig, midi_visualizer_scene, render_midi_visualizer,
};
use structured_midi::MidiMusic;

static VIDEO_SIZES: phf::Map<&UncasedStr, (u32, u32)> = phf_map! {
    UncasedStr::new("8k") | UncasedStr::new("4320p") => (7680, 4320),
    UncasedStr::new("4k") | UncasedStr::new("2160p") => (3840, 2160),
    UncasedStr::new("2k") => (2048, 1080),
    UncasedStr::new("1080p") => (1920, 1080),
    UncasedStr::new("720p") => (1280, 720),
    UncasedStr::new("480p") => (854, 480),
    UncasedStr::new("360p") => (640, 360),
    UncasedStr::new("240p") => (426, 240),
};
static VIDEO_FORMATS: phf::Map<&UncasedStr, OutputFormat> = phf_map! {
    UncasedStr::new("mp4") => OutputFormat::Mp4,
    UncasedStr::new("mov") => OutputFormat::Mov,
    UncasedStr::new("webm") => OutputFormat::Webm,
    UncasedStr::new("gif") => OutputFormat::Gif,
};

fn main() -> Result<()> {
    {
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
    }

    let add_common_args = |cmd: Command| {
        cmd
        .arg(arg!(midi_file:  <INFILE> "Input midi file to render.").required(true))
        .arg(arg!(-s --size <WIDTH> <HEIGHT> "Output video size.").value_names(["WIDTH", "HEIGHT"]).default_values(["1080p"]).num_args(1..=2))
        .arg(arg!(clear_color: --bg <COLOR> "Background color. In any supported CSS color format.").default_value("#282c34"))
        .arg(arg!(note_colors: --fg <COLOR>).default_values(["#89b9eb", "#9be347", "#f7931e", "#f7c71e"]).num_args(1..))
        .arg(arg!(--color_by <VALUE> "How note colors are assigned to different notes. Supported values are: channel, track, key_color.").default_value("channel"))
        .arg(arg!(--scroll_speed <FLOAT> "Note scroll speed in coordinate units per second. By default the screen height is 8 coordinate units.").default_value("2"))
        .arg(arg!(--buf_time <BEFORE> <AFTER> "Additional time before and after playing the song.").value_names(["BEFORE", "AFTER"]).default_values(["2", "2"]).num_args(1..=2))
    };

    let mut cmd_render = Command::new("render");
    cmd_render = add_common_args(cmd_render);
    cmd_render = cmd_render
    .arg(arg!(-f --format <VALUE> "Output video format. Supported values are: mp4, mov, webm, gif.").default_value("mp4"))
    .arg(arg!(--fps <INT> "Frame rate of the output.").default_value("60"))
    .arg(arg!(buffer_count: --buf <INT> "Buffer count used for multiple buffering.").default_value("2"));

    let mut cmd_preview = Command::new("preview");
    cmd_preview = add_common_args(cmd_preview);

    let cmd = command!()
        .subcommand(cmd_render)
        .subcommand(cmd_preview);

    match cmd.get_matches().subcommand() {
        Some(("render", matches)) => render(matches),
        Some(("preview", matches)) => preview(matches),
        _ => Ok(()),
    }
}

fn get_visualizer_config(matches: &ArgMatches) -> Result<MidiVisualizerConfig> {
    let colors = {
        let mut note_colors = Vec::with_capacity(4);
        for color in matches
            .get_many::<String>("note_colors")
            .unwrap()
            .map(|v| try_color(v.as_str()))
        {
            note_colors.push(color?);
        }
        note_colors
    };
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
    let buf_time = get_buf_time(matches)?;
    let visualizer_config = MidiVisualizerConfig {
        scroll_speed: matches.get_one::<String>("scroll_speed").unwrap().parse()?,
        buf_time,
        colors,
        color_by,
        ..Default::default()
    };
    Ok(visualizer_config)
}

fn get_song_and_name(matches: &ArgMatches) -> Result<(MidiMusic, String)> {
    let midi_path = PathBuf::from(
        matches
            .get_one::<String>("midi_file")
            .map(|v| v.as_str())
            .ok_or_else(|| anyhow!("invalid input"))?,
    );
    let src = std::fs::read(&midi_path)?;
    let music = MidiMusic::try_from(&src[..])?;
    let name = midi_path
        .file_stem()
        .map(|v| v.to_string_lossy().to_string())
        .unwrap_or_default();
    Ok((music, name))
}

fn get_video_size(matches: &ArgMatches) -> Result<(u32, u32)> {
    let mut args = matches
        .get_many::<String>("size")
        .expect("should have at least one value");
    if args.len() == 1 {
        let size = args.next().expect("should have one value").as_uncased();
        VIDEO_SIZES
            .get(size)
            .cloned()
            .ok_or_else(|| anyhow!("invalid size: {size}"))
    } else {
        let width = args.next().expect("should have two values").parse()?;
        let height = args.next().expect("should have two values").parse()?;
        Ok((width, height))
    }
}

fn get_video_format(matches: &ArgMatches) -> Result<OutputFormat> {
    let format = matches.get_one::<String>("format").unwrap().as_uncased();
    VIDEO_FORMATS
        .get(format)
        .cloned()
        .ok_or_else(|| anyhow!("invalid format: {format}"))
}

fn get_buf_time(matches: &ArgMatches) -> Result<[f64; 2]> {
    let mut buf_time = [0., 0.];
    let mut args = matches.get_many::<String>("buf_time").unwrap();
    if args.len() == 1 {
        let v = args.next().unwrap().parse::<f64>()?;
        buf_time[0] = v;
        buf_time[1] = v;
    } else {
        buf_time[0] = args.next().unwrap().parse::<f64>()?;
        buf_time[1] = args.next().unwrap().parse::<f64>()?;
    }
    if buf_time.iter().any(|&v| !(v.is_finite() && v >= 0.)) {
        bail!("`buf_time` must be non-negative finite numbers");
    }
    Ok(buf_time)
}

fn render(matches: &ArgMatches) -> Result<()> {
    let (music, name) = get_song_and_name(matches)?;
    let (width, height) = get_video_size(matches)?;
    let format = get_video_format(matches)?;

    let output = Output {
        name: None,
        fps: matches.get_one::<String>("fps").unwrap().parse()?,
        dir: ".".to_string(),
        width,
        height,
        format,
        save_frames: false,
    };

    let clear_color = matches.get_one::<String>("clear_color").unwrap().clone();
    let scene_config = SceneConfig { clear_color };
    let visualizer_config = get_visualizer_config(matches)?;
    let buf_size = matches.get_one::<String>("buffer_count").unwrap().parse()?;

    render_midi_visualizer(
        Arc::new(music),
        name.as_str(),
        &visualizer_config,
        &scene_config,
        &output,
        buf_size,
    );
    Ok(())
}

fn preview(matches: &ArgMatches) -> Result<()> {
    let (music, name) = get_song_and_name(matches)?;
    let visualizer_config = get_visualizer_config(matches)?;
    let music = Arc::new(music);
    let video_size = get_video_size(matches)?;
    let constructor = |r: &mut RanimScene| {
        midi_visualizer_scene(r, music.clone(), &visualizer_config, video_size);
    };
    let mut app = RanimPreviewApp::new(constructor, name);
    let clear_color = matches.get_one::<String>("clear_color").unwrap().clone();
    app.set_clear_color_str(&clear_color);
    run_app(app);
    Ok(())
}
