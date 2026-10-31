# Ranim MIDI Visualizer

_Note_: This document is mainly AI-generated, but the code of the project involves no massive AI-generated code. All code are hand-written aided by AI completion.

English | [简体中文](README.zh-CN.md)

**Ranim MIDI Visualizer** is a MIDI visualizer and MIDI video generator written in Rust, powered by the [Ranim](https://github.com/AzurIce/ranim) animation engine. It turns a Standard MIDI File into a "falling notes" piano video: note bars scroll down onto an 88-key piano keyboard, keys light up as they are played, pedals react to sustain/sostenuto/soft controller events, and a status bar reports timing and performance statistics.

The project is inspired by [`@kosua20`'s MIDI Visualizer](https://github.com/kosua20/MIDIVisualizer). It is intended for score-sync videos, piano covers, practice material, and any workflow where a clean, configurable piano-roll animation needs to be composited with sheet music, background footage, or a DAW session.

> **Project status:** `0.1.0-alpha`. The CLI and configuration types are still evolving and may change between releases.

## Table of contents

- [Features](#features)
- [What the video looks like](#what-the-video-looks-like)
- [Requirements](#requirements)
- [Build from source](#build-from-source)
- [Quick start](#quick-start)
- [Command-line reference](#command-line-reference)
  - [Top-level command](#top-level-command)
  - [`render`](#render)
  - [`preview`](#preview)
  - [Common options](#common-options)
  - [Resolution presets](#resolution-presets)
  - [Colors](#colors)
  - [Output files](#output-files)
- [Examples](#examples)
- [How it works](#how-it-works)
- [Project layout](#project-layout)
- [MIDI support and limitations](#midi-support-and-limitations)
- [Logging](#logging)
- [Troubleshooting](#troubleshooting)
- [Development](#development)
- [License](#license)
- [Acknowledgements](#acknowledgements)

## Features

- **MIDI to video** — renders `.mid` files to MP4, MOV, WebM, or GIF.
- **Waterfall / piano-roll animation** — every note becomes a rectangle whose length follows the note duration and whose opacity follows the note velocity.
- **88-key piano keyboard** — A0–C8 keyboard with per-note key highlighting.
- **Pedal display** — sustain (CC64), sostenuto (CC66), and soft (CC67) pedals are drawn and their depth is reflected in real time.
- **Live statistics** — a status bar shows elapsed musical time, note count, notes per second (NPS), and the legato index.
- **Progress bar** — a thin progress bar at the top of the frame tracks the position in the song.
- **Time- or beat-based scrolling** — choose whether the note grid scrolls by wall-clock seconds or by musical beats.
- **Configurable appearance** — background color, note palette, progress-bar colors, note color grouping, scroll speed, resolution, frame rate, and buffering.
- **Transparent output** — WebM (VP9 + alpha) and MOV (ProRes 4444 + alpha) can be composited over other footage. MP4 and GIF are opaque.
- **Interactive preview** — inspect the scene, timeline, and playback in a desktop window before committing to a long render.
- **Cross-platform** — the rendering stack uses `wgpu` and the system `ffmpeg`; the project is intended to run on Windows, macOS, and Linux.

## What the video looks like

The scene is laid out as follows:

```text
┌──────────────────────────────────────────────────────────────┐
│ Progress bar                                                 │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│  Note scrolling area                                         │
│                                                              │
│  ██████                                                      │
│  ██████████                          ┌─────────────┐         │
│  ████████████████                    │   Pedals    │         │
│                                      │             │         │
├──────────────────────────────────────┴─────────────┴─────────┤
│ Piano keyboard (88 keys, A0–C8)                              │
├──────────────────────────────────────────────────────────────┤
│ Status bar: TIME · NOTE COUNT · NPS (MAX) · LEGATO           │
└──────────────────────────────────────────────────────────────┘
```

The status bar reports:

| Field        | Meaning                                                                                                                                                                           |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `TIME`       | Elapsed musical time of the MIDI file, formatted as `HH:MM:SS.mmm`. The lead-in and trailing padding are not counted.                                                             |
| `NOTE COUNT` | Number of notes that have started so far, out of the total number of notes.                                                                                                       |
| `NPS (MAX)`  | Notes started per second within the last 1-second window, followed by the maximum value seen so far.                                                                              |
| `LEGATO`     | Sum of the intersections between all sounding notes and the last 1-second window, divided by the window length. Values above `1.000` are normal for chords and overlapping notes. |

The 1-second analysis window is currently not exposed as a CLI option.

## Requirements

### To run a released binary

- **FFmpeg** available as `ffmpeg` on `PATH` (recommended), or a static FFmpeg binary named `ffmpeg` in the current working directory.
- A GPU and driver supported by `wgpu` (Vulkan, Metal, or Direct3D 12).
- A desktop session if you want to use the `preview` subcommand.

Ranim checks for `ffmpeg` on `PATH`, then for `./ffmpeg`, and otherwise tries to download a static FFmpeg build into the current working directory. Installing FFmpeg yourself is more reliable, especially on Windows, and is required if the machine has no network access.

### To build from source

- **Rust nightly** — the project uses unstable features and edition 2024.
  The project currently uses:

  ```text
  rustc 1.101.0-nightly (16d1ffac1 2026-10-26)
  ```

- **Git** — Ranim and the patched `egui`/`eframe` crates are pinned to Git revisions, so the first build fetches from GitHub.
- **A native C/C++ toolchain** — MSVC Build Tools on Windows, Xcode Command Line Tools on macOS, or GCC/Clang on Linux. Some transitive dependencies (notably `aws-lc-sys`) also commonly need **CMake** and, on Windows x86-64, **NASM**.
- **Platform graphics/windowing development libraries** on Linux (X11 or Wayland plus the Vulkan loader) because of `winit`/`wgpu`.
- **FFmpeg** — only needed at runtime for rendering, not for compilation.

## Build from source

```bash
# 1. Get the source (clone or unpack the repository)
git clone <repository-url> ranim-midi-visualizer
cd ranim-midi-visualizer

# 2. Install and select a nightly toolchain
rustup toolchain install nightly
rustup override set nightly

# Optional but recommended
rustup component add rustfmt clippy

# 3. Build an optimized binary
cargo build --release --locked
```

The binary is written to `target/release/ranim-midi-visualizer` (`target\release\ranim-midi-visualizer.exe` on Windows).

Run it directly:

```bash
./target/release/ranim-midi-visualizer --help
./target/release/ranim-midi-visualizer preview path/to/song.mid
```

Or install it into Cargo's binary directory:

```bash
cargo install --path . --locked
```

Then invoke it from anywhere:

```bash
ranim-midi-visualizer render song.mid
```

### Why nightly?

The workspace enables unstable features such as `decl_macro`, `allocator_ext`, `unboxed_closures`, `fn_traits`, and `trait_alias` in several crates. A stable toolchain cannot compile the project. There is no `rust-toolchain.toml` in the repository, so the selected toolchain must be set explicitly with `rustup override` (or by using `cargo +nightly`).

If a newer nightly breaks the build, install a nightly close to the version listed above and select it for this directory.

### Network and proxies

The first build fetches Ranim and the patched `egui` crates from GitHub. If GitHub is unreachable, configure a proxy for Git and Cargo. For example, with a local proxy on port `7897`:

```bash
# Git
git config --global http.proxy http://127.0.0.1:7897

# PowerShell: Cargo reads these environment variables
$env:HTTP_PROXY = "http://127.0.0.1:7897"
$env:HTTPS_PROXY = "http://127.0.0.1:7897"

# Bash/Zsh
export HTTP_PROXY=http://127.0.0.1:7897
export HTTPS_PROXY=http://127.0.0.1:7897
```

Replace the port with the port your proxy actually listens on.

### Features

| Feature   | Default | Effect                                                                         |
| --------- | ------- | ------------------------------------------------------------------------------ |
| `preview` | enabled | Compiles the `preview` subcommand, which opens Ranim's desktop preview window. |

To build a `render`-only binary:

```bash
cargo build --release --locked --no-default-features
```

Note that the current dependency configuration still enables Ranim's own `preview` feature, so this mainly removes the CLI subcommand rather than the entire GUI dependency stack.

### First build notes

- A release build is strongly recommended. Debug builds render very slowly.
- The first build compiles Ranim, Typst, `wgpu`, `egui`, and their dependencies and can take a long time. Later builds are incremental.
- `--locked` makes Cargo honor the committed `Cargo.lock`, including the pinned Ranim and `egui` Git revisions.
- The build output (`target/`) is ignored by Git.

## Quick start

```bash
# Preview the scene interactively
ranim-midi-visualizer preview song.mid

# Render a 1920x1080, 60 fps MP4 into the current directory
ranim-midi-visualizer render song.mid

# The output is: ./song_1920x1080_60.mp4
```

The renderer writes a **video-only** file: no audio track is muxed. Add the original audio in a video editor, or with FFmpeg:

```bash
ffmpeg -i song_1920x1080_60.mp4 -i song.wav -c:v copy -c:a aac -shortest song_with_audio.mp4
```

## Command-line reference

### Top-level command

```text
ranim-midi-visualizer [COMMAND]
```

Running the program without a subcommand prints the long help and exits successfully.

| Command   | Purpose                                                                              |
| --------- | ------------------------------------------------------------------------------------ |
| `render`  | Render the MIDI file to a scrolling video file.                                      |
| `preview` | Open a desktop window to preview the scene (requires the default `preview` feature). |
| `help`    | Print help for the program or a subcommand (`help render`, `help preview`).          |

### `render`

```text
ranim-midi-visualizer render [OPTIONS] <INFILE>
```

Renders the input MIDI file and writes the video into the **current working directory**.

| Option                   | Value                       | Default    | Description                                                                                |
| ------------------------ | --------------------------- | ---------- | ------------------------------------------------------------------------------------------ |
| `<INFILE>`               | path                        | _required_ | Input Standard MIDI File (`.mid`).                                                         |
| `-f`, `--format <VALUE>` | `mp4`, `mov`, `webm`, `gif` | `mp4`      | Output container/codec. Case-insensitive.                                                  |
| `--fps <INT>`            | positive integer            | `60`       | Output frame rate. GIF output is capped at 50 fps by the renderer.                         |
| `--buf <INT>`            | integer `>= 1`              | `2`        | Number of frames buffered for multi-buffered rendering. Higher values use more GPU memory. |

`render` accepts all [common options](#common-options) as well.

### `preview`

```text
ranim-midi-visualizer preview [OPTIONS] <INFILE>
```

Opens Ranim's interactive preview window for the same scene, using the same common scroll and appearance options as `render`. The `preview` command itself does not render a video file, but the preview window includes an **Export** dialog in native builds.

Use it to check note placement, colors, scroll speed, and padding before starting a render. Preview controls include:

- <kbd>Space</kbd> — play/pause
- <kbd>←</kbd> / <kbd>→</kbd> — step backward/forward by one frame
- Timeline slider and time field — jump to any point
- Playback speed, looping, jump-to-start/end, and Output/Depth view controls
- **Export** — render the current scene from the preview window

`preview` accepts all [common options](#common-options).

> **Note:** in the current version the preview window starts with a transparent clear color, so `--bg` is applied to `render` output but is not reflected in the preview window.

> `preview` is compiled only when the default `preview` feature is enabled. Building with `--no-default-features` removes this subcommand.

### Common options

These options are shared by `render` and `preview`.

| Option                        | Value                                           | Default                           | Description                                                                                                                                                                                                                                                   |
| ----------------------------- | ----------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-m`, `--metric_base <VALUE>` | `time`, `beat`                                  | `time`                            | Scroll metric. `time` moves notes at a constant rate in seconds; `beat` moves them at a constant rate in beats, so tempo changes alter the on-screen spacing.                                                                                                 |
| `-s`, `--size <SIZE>`         | preset name, or `WIDTH HEIGHT`                  | `1080p`                           | Output resolution. Use one preset (e.g. `4k`) or two explicit pixel values (e.g. `-s 2560 1440`).                                                                                                                                                             |
| `--bg <COLOR>`                | CSS color                                       | `#282c34`                         | Background color. Use `transparent` together with WebM or MOV for an alpha background.                                                                                                                                                                        |
| `--fg <COLOR>...`             | one or more CSS colors                          | `#89b9eb #9be347 #f7931e #f7c71e` | Note color palette. Supplying this option replaces the whole default palette.                                                                                                                                                                                 |
| `--progress_bar_fg <COLOR>`   | CSS color                                       | `#a8a3cc`                         | Progress-bar fill color.                                                                                                                                                                                                                                      |
| `--progress_bar_bg <COLOR>`   | CSS color                                       | `transparent`                     | Progress-bar track color.                                                                                                                                                                                                                                     |
| `--color_by <VALUE>`          | `channel`/`voice`, `track`/`staff`, `key_color` | `channel`                         | How palette entries are assigned: by MIDI channel, by track, or by black/white key. The palette cycles when there are more groups than colors.                                                                                                                |
| `--scroll_speed <FLOAT>`      | number `> 0`                                    | `2`                               | Scroll speed in coordinate units. The visible frame is 8 coordinate units high, and the scrolling area is slightly shorter because of the keyboard, so `2` means a note crosses the scrolling area in about 4 seconds (time metric) or 4 beats (beat metric). |
| `--buf_time <BEFORE> <AFTER>` | non-negative seconds; one or two values         | `2 2`                             | Extra blank time before and after the music. A single value is used for both sides. The renderer additionally inserts the time needed for the first note to travel down the note area.                                                                        |

**Important:** long option names use underscores exactly as shown — `--metric_base`, `--scroll_speed`, `--buf_time`, `--progress_bar_fg`, `--progress_bar_bg`, `--color_by`. Kebab-case spellings such as `--metric-base` are rejected.

**Argument order:** put `<INFILE>` before options that take multiple values (`--size`, `--buf_time`, `--fg`). Those options consume consecutive non-flag values, so a following file path can be mistaken for another value. All examples below use the safe order.

#### Metric base in detail

- `time` (default): note positions are proportional to elapsed seconds. A note's vertical speed is `scroll_speed` coordinate units per second, regardless of tempo.
- `beat`: note positions are proportional to musical beats. A note's speed is `scroll_speed` coordinate units per beat; faster tempo sections appear to move faster in seconds, while beat spacing stays musically even.

#### Size argument forms

```bash
-s 1080p          # named preset
-s 4k             # named preset
-s 1920 1080      # explicit width and height
-s 2560 1440      # explicit width and height
```

The width and height must be supplied as two separate arguments; `-s 1920x1080` is not accepted.

### Resolution presets

Preset names are case-insensitive.

| Preset  | Aliases        | Resolution  |
| ------- | -------------- | ----------- |
| `8k`    | `4320p`        | 7680 × 4320 |
| `4k`    | `2160p`, `uhd` | 3840 × 2160 |
| `2k`    | —              | 2048 × 1080 |
| `1080p` | `fhd`          | 1920 × 1080 |
| `720p`  | `hd`           | 1280 × 720  |
| `480p`  | —              | 854 × 480   |
| `360p`  | —              | 640 × 360   |
| `240p`  | —              | 426 × 240   |

A custom size is also accepted: `-s <WIDTH> <HEIGHT>`.

### Colors

Colors are parsed by Ranim's color utilities and accept CSS-style values such as:

- hexadecimal: `#282c34`, `#89b9eb`, `#00000080`
- functional: `rgb(137, 185, 235)`, `rgba(0, 0, 0, 0.5)`
- named colors: `white`, `black`, `red`, ...
- `transparent`

Note rectangles use the selected palette color as their base color, but their opacity is driven by MIDI velocity. A quiet note is more transparent than a loud note.

### Output files

- Videos are written to the **current working directory**, not next to the MIDI file.
- The file name is derived from the MIDI file stem and the render settings:

  ```text
  <midi-file-stem>_<width>x<height>_<fps>.<extension>
  ```

  For example, `path/to/Nocturne.mid` rendered at 1080p60 produces `./Nocturne_1920x1080_60.mp4`.

- Existing files with the same name are overwritten.
- Encoding is performed by FFmpeg, which receives raw RGBA frames over a pipe. The output codecs are:

  | Format | Codec                     | Pixel format   | Alpha |
  | ------ | ------------------------- | -------------- | ----- |
  | `mp4`  | `libx264`                 | `yuv420p`      | No    |
  | `mov`  | `prores_ks` (ProRes 4444) | `yuva444p10le` | Yes   |
  | `webm` | `libvpx-vp9`              | `yuva420p`     | Yes   |
  | `gif`  | `gif`                     | `rgb8`         | No    |

  WebM with alpha is usually the best compromise for compositing. ProRes 4444 preserves alpha but produces very large files.

## Examples

The examples use Bash-style `\` line continuations. In PowerShell, write each command on one line or use a backtick (`` ` ``) at the end of a line.

```bash
# Preview first
ranim-midi-visualizer preview song.mid

# Basic 1080p60 MP4
ranim-midi-visualizer render song.mid

# 4K render with a custom background and palette
ranim-midi-visualizer render song.mid \
  -s 4k --fps 60 \
  --bg '#101014' \
  --fg '#89b9eb' '#9be347' '#f7931e' '#f7c71e'

# Transparent WebM for overlaying on sheet music or footage
ranim-midi-visualizer render song.mid -f webm --bg transparent -s 1080p

# Beat-based scrolling with one second of padding on both sides
ranim-midi-visualizer render song.mid -m beat --scroll_speed 2 --buf_time 1 1

# Color notes by track and recolor the progress bar
ranim-midi-visualizer render song.mid \
  --color_by track \
  --progress_bar_fg '#ff8800' \
  --progress_bar_bg '#00000080'

# Small GIF preview clip (frame rate is capped at 50)
ranim-midi-visualizer render song.mid -f gif --fps 30 -s 720p

# Custom resolution
ranim-midi-visualizer render song.mid -s 2560 1440 --fps 30

# More render buffering (uses more GPU memory)
ranim-midi-visualizer render song.mid --buf 4

# Build and run from source without installing
cargo run --release --locked -- render song.mid -s 720p
```

## How it works

1. **Parse the MIDI file.** `waveform-utils` uses [`midly`](https://crates.io/crates/midly) to read every track. Note-on/note-off pairs are collected per channel and pitch, and stored as tick ranges in an interval tree. Note-on events with velocity 0 are treated as note-offs.
2. **Build a tempo map.** Set Tempo meta events produce a segmented linear mapping from MIDI ticks to nanosecond-resolution time units. This map is used for the `time` metric and for the status bar.
3. **Extract pedal events.** Continuous controllers 64, 66, and 67 become sustain, sostenuto, and soft pedal controls with a normalized depth in `0..=1`.
4. **Construct a Ranim scene.** `ranim-midi-visualizer_lib` creates the static layout (keyboard, status bar, progress bar, pedals) and adds one animation sequence per note. Each note rectangle is translated down the screen, its length is derived from the note duration, and its color/opacity from the active palette and velocity.
5. **Render frames with `wgpu`.** Ranim evaluates the scene at the requested frame rate, renders each frame off-screen on the GPU, and reads the RGBA pixels back to the CPU.
6. **Encode with FFmpeg.** Raw frames are streamed to an FFmpeg child process, which encodes the selected container and codec. A terminal progress bar reports the current frame and ETA.
7. **Preview instead of rendering.** The `preview` subcommand builds the same scene inside `RanimPreviewApp`, which displays it in a desktop window for interactive inspection.

The scene builder is deterministic: the same MIDI file, configuration, and Ranim revision produce the same animation.

## Project layout

This repository is a Cargo workspace:

| Path                                | Description                                                                                                                                                                          |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/main.rs`                       | CLI entry point: argument parsing, logging setup, configuration conversion, and dispatch to `render`/`preview`.                                                                      |
| `crates/ranim-midi-visualizer_lib`  | The visualizer itself: scene construction, note animation, and public configuration types (`MidiVisualizerConfig`, `NoteConfig`, `ColorBy`, `MetricBase`, `ProgressBarConfig`, ...). |
| `crates/waveform-utils`             | MIDI parsing and the music model: notes, voices, staves, control/pedal events, tempo mapping, note statistics, and waveform/synth utilities.                                         |
| `crates/ranim_music`                | Reusable Ranim items for music visualization: the 88-key `PianoKeyboard` and the `PianoPedals` display.                                                                              |
| `crates/ranim-midi-visualizer_math` | Small math toolkit used by the animation code: cyclic indices, segmented linear functions, ladder functions, interpolation, and step grids.                                          |
| `crates/simple-interval-tree`       | An interval tree built from two `BTreeMap`s and a `SlotMap`, supporting overlapping ranges and multiple values per identical interval.                                               |
| `crates/music-utils`                | Pitch and keyboard helpers such as black-key detection, octave ranges, and key-index conversion.                                                                                     |
| `justfile`                          | Optional development recipes (`build`, `run`, `test`, `lint`, ...). It is configured for Windows PowerShell.                                                                         |
| `Cargo.toml`                        | Workspace definition, shared dependency versions, and the pinned Ranim Git revision.                                                                                                 |

## MIDI support and limitations

- **Standard MIDI Files only.** MIDI 2.0 files and non-MIDI inputs are not supported.
- **Formats 0 and 1** are the intended inputs. Format 2 (independent sequences) is not meaningfully supported because the parser merges all tracks into one song.
- **Tempo changes** (`Set Tempo`) are supported. Tempo events are collected globally; files whose tracks declare conflicting tempo maps may be timed incorrectly.
- **Notes** are matched per `(channel, pitch)` pair. Note-off may be expressed either as a `NoteOff` event or as a `NoteOn` event with velocity 0.
- **Pedals**: CC64 (sustain), CC66 (sostenuto), and CC67 (soft) are visualized. Other controllers are ignored.
- **Velocity** controls note opacity. It does not affect timing or pitch.
- **Program changes, banks, and audio** are ignored. Rendering is silent; no instrument synthesis is performed.
- **Keyboard range** is fixed to the standard 88 keys, A0–C8 (MIDI notes 21–108). Notes outside this range are not visible on the keyboard.
- **Layout is fixed**: progress bar at the top, status bar at the bottom, keyboard above the status bar, pedals at the bottom-right of the note area.
- **Output location and naming** are fixed to the current directory and the `<stem>_<width>x<height>_<fps>` template. They are not configurable from the CLI yet.
- **Alpha output** is available only for WebM and MOV.

## Logging

The program initializes a `tracing` subscriber. Log verbosity can be controlled with the `RUST_LOG` environment variable:

```bash
# Global debug logging
RUST_LOG=debug ranim-midi-visualizer render song.mid

# Per-target logging
RUST_LOG=ranim=debug,ranim_midi_visualizer_lib=trace,waveform_utils=info \
  ranim-midi-visualizer render song.mid
```

If a target is not mentioned in `RUST_LOG`, these defaults apply:

| Target                      | Default level |
| --------------------------- | ------------- |
| `ranim_midi_visualizer_lib` | `DEBUG`       |
| `ranim_midi_visualizer`     | `INFO`        |
| `ranim`                     | `INFO`        |
| `ranim_cli`                 | `INFO`        |
| `waveform_utils`            | `ERROR`       |

On PowerShell:

```powershell
$env:RUST_LOG = "ranim=debug,ranim_midi_visualizer_lib=trace"
ranim-midi-visualizer render song.mid
```

## Troubleshooting

### `ffmpeg` cannot be found or the automatic download fails

Install FFmpeg and make sure `ffmpeg -version` works in the same terminal:

- Windows: install a build from [ffmpeg.org](https://ffmpeg.org/download.html) or a package manager such as `winget`/`choco`, then add its `bin` directory to `PATH`.
- macOS: `brew install ffmpeg`
- Debian/Ubuntu: `sudo apt install ffmpeg`

Alternatively, place a static FFmpeg executable named `ffmpeg` in the directory you run the renderer from. Ranim uses `./ffmpeg` as a fallback.

### `No suitable GPU adapter` / `wgpu` initialization failure

Update the graphics driver and make sure the platform backend is available:

- Windows: Direct3D 12 or Vulkan
- macOS: Metal
- Linux: Vulkan (`vulkaninfo` from `vulkan-tools` is a useful check)

Virtual machines and remote sessions may not expose a usable GPU.

### Transparent background renders as black

MP4 (`yuv420p`) and GIF cannot store an alpha channel. Use:

```bash
ranim-midi-visualizer render song.mid -f webm --bg transparent
# or
ranim-midi-visualizer render song.mid -f mov --bg transparent
```

Then verify in your editor that the alpha channel was preserved.

### `unexpected argument '--metric-base'`

Long option names use underscores, matching the source definitions:

```bash
--metric_base --scroll_speed --buf_time --progress_bar_fg --progress_bar_bg --color_by
```

### The status-bar font looks wrong

The status bar uses this font fallback list:

```text
Maple Mono NF, Cascadia Code NF, LXGW WenKai Mono, Consolas, Monaco, Courier New
```

Install one of the first three for the intended look; otherwise the system fallback may substitute a different monospace face.

### Rendering is slow

- Render at a lower resolution or frame rate.
- Prefer `--release` builds.
- Reduce `--buf` if GPU memory is the bottleneck.
- Close other GPU-heavy applications.
- WebM/VP9 with alpha and ProRes 4444 MOV are significantly slower and larger than MP4.

### Preview fails on a headless Linux machine

`preview` opens a real desktop window through `winit`. It needs an X11/Wayland session and a GPU. Use `render` on headless servers, or run the preview under a virtual display with GPU access.

## Development

With [`just`](https://github.com/casey/just) installed:

```bash
just build       # cargo build --release
just run render path/to/song.mid
just test        # cargo test --workspace
just lint        # cargo fmt --all, then clippy with -D warnings
just fmt         # cargo fmt --all
```

Without `just`, use the underlying Cargo commands:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

The repository contains MIDI fixtures under `crates/waveform-utils/src/music/tests/` that can be used for manual testing:

```bash
cargo run --release -- render crates/waveform-utils/src/music/tests/song_2.mid
```

## License

This project is licensed under the MIT License. See [LICENSE.txt](LICENSE.txt).

Copyright © 2026 F.X.P.

## Acknowledgements

- [Ranim](https://github.com/AzurIce/ranim) and its author **@AzurIce** — the animation engine that makes the scene and rendering pipeline possible.
- [`@kosua20`'s MIDI Visualizer](https://github.com/kosua20/MIDIVisualizer) — the original inspiration for this style of waterfall visualization.
- [Manim](https://github.com/3b1b/manim) and [JAnim](https://github.com/jkjkil4/JAnim) — the animation engines that inspired Ranim.
- **Wiwi Kuan** — the `NPS` and `LEGATO` metrics follow the design of the [Pianometer](https://nicechord.com/pianometer/) keyboard display.
- The Rust ecosystem: `midly`, `clap`, `wgpu`, `egui`/`eframe`, `typst`, and the many other crates this project depends on.
