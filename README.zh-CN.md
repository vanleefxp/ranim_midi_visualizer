# Ranim MIDI Visualizer

[English](README.md) | 简体中文

_注_：本文档的主要内容目前为 AI 生成，但项目代码均为手写开发，AI 在其中仅作补全代码的工作，未参与大规模代码生成。

**Ranim MIDI Visualizer** 是一款使用 Rust 编写、基于 [Ranim](https://github.com/AzurIce/ranim) 动画引擎的 MIDI 可视化与 MIDI 视频生成工具。它可以把标准 MIDI 文件渲染成「瀑布流」钢琴动画视频：音符长条从上方下落，落到 88 键钢琴键盘上时对应琴键亮起，踏板图标随延音、持音、弱音控制器的变化而变化，画面底部还会实时显示时间、音符数、NPS、连奏度等统计信息。

本项目受 [`@kosua20` 的 MIDI Visualizer](https://github.com/kosua20/MIDIVisualizer) 启发，适合用于曲谱同步视频、钢琴翻弹、练习视频，以及需要把干净的钢琴卷帘动画叠加到乐谱、背景视频或 DAW 画面上的工作流。

> **项目状态：** `0.1.0-alpha`。命令行接口和配置类型仍在演进，版本之间可能发生变化。

## 目录

- [功能特性](#功能特性)
- [画面构成](#画面构成)
- [环境要求](#环境要求)
- [从源码构建](#从源码构建)
- [快速开始](#快速开始)
- [命令行参考](#命令行参考)
  - [顶层命令](#顶层命令)
  - [`render`](#render)
  - [`preview`](#preview)
  - [通用参数](#通用参数)
  - [分辨率预设](#分辨率预设)
  - [颜色](#颜色)
  - [输出文件](#输出文件)
- [使用示例](#使用示例)
- [工作原理](#工作原理)
- [项目结构](#项目结构)
- [MIDI 支持与限制](#midi-支持与限制)
- [日志](#日志)
- [常见问题](#常见问题)
- [开发](#开发)
- [许可证](#许可证)
- [致谢](#致谢)

## 功能特性

- **MIDI 转视频**：把 `.mid` 文件渲染为 MP4、MOV、WebM 或 GIF。
- **瀑布流 / 钢琴卷帘动画**：每个音符对应一个矩形长条，长度由音符时值决定，不透明度由力度（velocity）决定。
- **88 键钢琴键盘**：显示 A0–C8 的完整键盘，并在音符触发时高亮对应琴键。
- **踏板显示**：支持延音踏板（CC64）、持音踏板（CC66）和弱音踏板（CC67），踏板深度会实时反映在图标上。
- **实时统计信息**：状态栏显示乐曲时间、已演奏音符数、每秒音符数（NPS）和连奏度（Legato）。
- **进度条**：画面顶部的细进度条显示当前播放位置。
- **按时间或按拍滚动**：可选择让音符按真实秒数滚动，或按音乐拍数滚动。
- **可配置外观**：背景色、音符配色、进度条颜色、音符配色依据、滚动速度、分辨率、帧率、缓冲数量均可通过命令行调整。
- **透明背景输出**：WebM（VP9 + Alpha）和 MOV（ProRes 4444 + Alpha）支持透明通道，便于叠加到其他素材上；MP4 和 GIF 不支持透明。
- **交互式预览**：正式渲染前，可在桌面窗口中检查场景、时间轴和播放效果。
- **跨平台**：渲染基于 `wgpu`，编码调用系统 `ffmpeg`，目标平台为 Windows、macOS 和 Linux。

## 画面构成

场景布局如下：

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

状态栏各项的含义：

| 字段         | 含义                                                                                                     |
| ------------ | -------------------------------------------------------------------------------------------------------- |
| `TIME`       | MIDI 乐曲已经播放的时间，格式为 `HH:MM:SS.mmm`。片头留白和片尾留白不计入。                               |
| `NOTE COUNT` | 当前已经开始的音符数 / 乐曲总音符数。                                                                    |
| `NPS (MAX)`  | 最近 1 秒窗口内开始的音符数（换算为每秒），括号内为截至当前的最大值。                                    |
| `LEGATO`     | 最近 1 秒窗口内所有发声音符与窗口交集长度之和，除以窗口长度。和弦或音符重叠时大于 `1.000` 属于正常现象。 |

目前 1 秒的分析窗口长度没有作为命令行参数暴露。

## 环境要求

### 运行已编译的程序

- **FFmpeg**：推荐安装并确保 `ffmpeg` 在 `PATH` 中；也可以把名为 `ffmpeg` 的静态可执行文件放在运行目录下。
- **GPU 与驱动**：需要 `wgpu` 支持的图形后端（Vulkan、Metal 或 Direct3D 12）。
- **桌面环境**：只有使用 `preview` 子命令时才需要。

Ranim 会先查找 `PATH` 中的 `ffmpeg`，再查找当前目录下的 `./ffmpeg`；如果都没有找到，会尝试下载静态 FFmpeg 到当前运行目录。建议自行安装 FFmpeg，特别是在 Windows 上；无网络环境则必须手动安装。

### 从源码构建

- **Rust nightly**：项目使用了不稳定特性和 edition 2024。当前项目使用的版本是：

  ```text
  rustc 1.101.0-nightly (16d1ffac1 2026-10-26)
  ```

- **Git**：Ranim 以及打过补丁的 `egui`/`eframe` 都固定到 Git 提交，首次构建需要从 GitHub 拉取。
- **本机 C/C++ 工具链**：Windows 上的 MSVC Build Tools、macOS 上的 Xcode Command Line Tools，或 Linux 上的 GCC/Clang。部分传递依赖（尤其是 `aws-lc-sys`）通常还需要 **CMake**，Windows x86-64 上还可能需要 **NASM**。
- **Linux 图形/窗口开发库**：由于使用 `winit`/`wgpu`，需要 X11 或 Wayland 以及 Vulkan loader 相关的开发库。
- **FFmpeg**：只在运行时渲染需要，编译本身不依赖 FFmpeg。

## 从源码构建

```bash
# 1. 获取源码（克隆仓库或解压源码包）
git clone <repository-url> ranim-midi-visualizer
cd ranim-midi-visualizer

# 2. 安装并选择 nightly 工具链
rustup toolchain install nightly
rustup override set nightly

# 可选，但推荐安装
rustup component add rustfmt clippy

# 3. 构建优化版本
cargo build --release --locked
```

构建产物位于 `target/release/ranim-midi-visualizer`（Windows 上为 `target\release\ranim-midi-visualizer.exe`）。

直接运行：

```bash
./target/release/ranim-midi-visualizer --help
./target/release/ranim-midi-visualizer preview path/to/song.mid
```

也可以安装到 Cargo 的可执行文件目录：

```bash
cargo install --path . --locked
```

之后即可在任意目录使用：

```bash
ranim-midi-visualizer render song.mid
```

### 为什么必须使用 nightly？

工作区中的多个 crate 启用了 `decl_macro`、`allocator_ext`、`unboxed_closures`、`fn_traits`、`trait_alias` 等不稳定特性，稳定版工具链无法编译。仓库中没有 `rust-toolchain.toml`，因此需要显式用 `rustup override` 选择 nightly，或者在命令中使用 `cargo +nightly`。

如果更新后的 nightly 导致构建失败，可以安装一个接近上述版本的 nightly，并把它设为当前目录的工具链。

### 网络与代理

首次构建需要访问 GitHub 拉取 Ranim 和 `egui` 的固定提交。如果 GitHub 访问失败，可以为 Git/Cargo 配置本地代理。例如代理端口为 `7897` 时：

```bash
# Git 使用代理
git config --global http.proxy http://127.0.0.1:7897

# PowerShell 中为当前终端设置代理，Cargo 会读取这些环境变量
$env:HTTP_PROXY = "http://127.0.0.1:7897"
$env:HTTPS_PROXY = "http://127.0.0.1:7897"

# Bash/Zsh
export HTTP_PROXY=http://127.0.0.1:7897
export HTTPS_PROXY=http://127.0.0.1:7897
```

请把端口替换为实际可用的代理端口。

### 特性开关

| 特性      | 默认 | 作用                                                   |
| --------- | ---- | ------------------------------------------------------ |
| `preview` | 开启 | 编译 `preview` 子命令，用于打开 Ranim 的桌面预览窗口。 |

只构建 `render` 子命令：

```bash
cargo build --release --locked --no-default-features
```

注意：当前依赖配置仍然为 Ranim 开启了它自己的 `preview` 特性，因此这样做主要是去掉本项目的 `preview` 子命令，并不能完全移除 GUI 依赖。

### 首次构建说明

- 强烈建议使用 release 构建，debug 构建的渲染速度会非常慢。
- 首次构建会编译 Ranim、Typst、`wgpu`、`egui` 及其依赖，耗时较长；后续构建是增量的。
- `--locked` 会让 Cargo 严格使用仓库中的 `Cargo.lock`，包括固定的 Ranim 和 `egui` Git 提交。
- 构建目录 `target/` 已被 Git 忽略。

## 快速开始

```bash
# 先交互式预览场景
ranim-midi-visualizer preview song.mid

# 渲染 1920x1080、60 fps 的 MP4 到当前目录
ranim-midi-visualizer render song.mid

# 输出文件为：./song_1920x1080_60.mp4
```

渲染结果是**纯视频**，不包含音轨。可以在剪辑软件中把原音频合进去，或使用 FFmpeg：

```bash
ffmpeg -i song_1920x1080_60.mp4 -i song.wav -c:v copy -c:a aac -shortest song_with_audio.mp4
```

## 命令行参考

### 顶层命令

```text
ranim-midi-visualizer [COMMAND]
```

不带子命令运行程序时，会打印完整帮助信息并正常退出。

| 子命令    | 作用                                                         |
| --------- | ------------------------------------------------------------ |
| `render`  | 把 MIDI 文件渲染为滚动视频文件。                             |
| `preview` | 打开桌面窗口预览场景（需要默认的 `preview` 特性）。          |
| `help`    | 查看程序或子命令的帮助（如 `help render`、`help preview`）。 |

### `render`

```text
ranim-midi-visualizer render [OPTIONS] <INFILE>
```

渲染输入的 MIDI 文件，并把视频写入**当前工作目录**。

| 参数                     | 取值                        | 默认值 | 说明                                             |
| ------------------------ | --------------------------- | ------ | ------------------------------------------------ |
| `<INFILE>`               | 文件路径                    | _必填_ | 输入的标准 MIDI 文件（`.mid`）。                 |
| `-f`, `--format <VALUE>` | `mp4`、`mov`、`webm`、`gif` | `mp4`  | 输出容器/编码格式，大小写不敏感。                |
| `--fps <INT>`            | 正整数                      | `60`   | 输出帧率。GIF 输出会被渲染器限制为最高 50 fps。  |
| `--buf <INT>`            | 大于等于 `1` 的整数         | `2`    | 多重缓冲使用的帧缓冲数量。数值越大占用显存越多。 |

`render` 同时接受所有[通用参数](#通用参数)。

### `preview`

```text
ranim-midi-visualizer preview [OPTIONS] <INFILE>
```

使用与 `render` 相同的通用外观与滚动参数，在 Ranim 的交互式预览窗口中打开同一个场景。`preview` 命令本身不会渲染视频文件，但原生构建的预览窗口中带有 **Export**（导出）对话框。

适合在正式渲染前检查音符位置、配色、滚动速度和首尾留白。预览窗口支持：

- <kbd>Space</kbd>：播放/暂停
- <kbd>←</kbd> / <kbd>→</kbd>：向前/向后单帧步进
- 时间轴滑块和时间输入框：跳转到任意位置
- 播放速度、循环、跳到开头/结尾、Output/Depth 视图切换
- **Export**：直接从预览窗口渲染当前场景

`preview` 接受所有[通用参数](#通用参数)。

> **注意：** 当前版本的预览窗口初始清屏色为透明，因此 `--bg` 会应用于 `render` 的输出视频，但不会反映在预览窗口中。

> `preview` 只有在默认的 `preview` 特性开启时才会被编译。使用 `--no-default-features` 构建后，该子命令不存在。

### 通用参数

以下参数由 `render` 和 `preview` 共用。

| 参数                          | 取值                                            | 默认值                            | 说明                                                                                                                                                                        |
| ----------------------------- | ----------------------------------------------- | --------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `-m`, `--metric_base <VALUE>` | `time`、`beat`                                  | `time`                            | 滚动基准。`time` 按秒匀速移动音符；`beat` 按拍匀速移动，速度变化会改变音符在屏幕上的间距。                                                                                  |
| `-s`, `--size <SIZE>`         | 预设名称，或 `WIDTH HEIGHT`                     | `1080p`                           | 输出分辨率。可以写一个预设（如 `4k`），也可以写两个像素值（如 `-s 2560 1440`）。                                                                                            |
| `--bg <COLOR>`                | CSS 颜色                                        | `#282c34`                         | 背景色。与 WebM 或 MOV 配合时可以使用 `transparent` 输出透明背景。                                                                                                          |
| `--fg <COLOR>...`             | 一个或多个 CSS 颜色                             | `#89b9eb #9be347 #f7931e #f7c71e` | 音符配色。一旦提供该参数，会整体替换默认配色。                                                                                                                              |
| `--progress_bar_fg <COLOR>`   | CSS 颜色                                        | `#a8a3cc`                         | 进度条前景色。                                                                                                                                                              |
| `--progress_bar_bg <COLOR>`   | CSS 颜色                                        | `transparent`                     | 进度条背景色。                                                                                                                                                              |
| `--color_by <VALUE>`          | `channel`/`voice`、`track`/`staff`、`key_color` | `channel`                         | 配色分配依据：按 MIDI 通道、按轨道，或按黑键/白键。分组数超过颜色数时配色会循环。                                                                                           |
| `--scroll_speed <FLOAT>`      | 大于 `0` 的数                                   | `2`                               | 滚动速度，单位为坐标单位。可见画面高度为 8 个坐标单位；由于底部键盘占用一部分空间，音符滚动区域会略矮一些。`2` 表示音符大约用 4 秒（`time`）或 4 拍（`beat`）穿过滚动区域。 |
| `--buf_time <BEFORE> <AFTER>` | 非负秒数，一个或两个值                          | `2 2`                             | 乐曲开始前和结束后的额外留白。只给一个值时两侧使用相同数值。渲染器还会自动加入首个音符从画面顶部落到键盘所需的时间。                                                        |

**重要：** 长参数名中的下划线必须与源码一致，例如 `--metric_base`、`--scroll_speed`、`--buf_time`、`--progress_bar_fg`、`--progress_bar_bg`、`--color_by`。写成 `--metric-base` 这样的连字符形式会被拒绝。

**参数顺序：** 请把 `<INFILE>` 放在接受多个值的选项（`--size`、`--buf_time`、`--fg`）之前。这些选项会连续吞掉后续的非选项值，因此后面的文件路径可能被误当作另一个值。下文的示例都使用了安全顺序。

#### `metric_base` 详解

- `time`（默认）：音符位置与真实秒数成正比。无论速度如何变化，音符的垂直速度都是 `scroll_speed` 坐标单位/秒。
- `beat`：音符位置与音乐拍数成正比。音符速度为 `scroll_speed` 坐标单位/拍；速度较快的段落看起来会移动得更快，但拍与拍之间的间距保持均匀。

#### `size` 的写法

```bash
-s 1080p          # 预设
-s 4k             # 预设
-s 1920 1080      # 显式指定宽高
-s 2560 1440      # 显式指定宽高
```

宽度和高度必须作为两个独立参数传入，`-s 1920x1080` 这种写法不被接受。

### 分辨率预设

预设名称大小写不敏感。

| 预设    | 别名           | 分辨率      |
| ------- | -------------- | ----------- |
| `8k`    | `4320p`        | 7680 × 4320 |
| `4k`    | `2160p`、`uhd` | 3840 × 2160 |
| `2k`    | —              | 2048 × 1080 |
| `1080p` | `fhd`          | 1920 × 1080 |
| `720p`  | `hd`           | 1280 × 720  |
| `480p`  | —              | 854 × 480   |
| `360p`  | —              | 640 × 360   |
| `240p`  | —              | 426 × 240   |

也可以用 `-s <WIDTH> <HEIGHT>` 指定自定义分辨率。

### 颜色

颜色由 Ranim 的颜色工具解析，支持 CSS 风格写法，例如：

- 十六进制：`#282c34`、`#89b9eb`、`#00000080`
- 函数式：`rgb(137, 185, 235)`、`rgba(0, 0, 0, 0.5)`
- 颜色名：`white`、`black`、`red` 等
- `transparent`

音符矩形以所选配色为基色，但不透明度由 MIDI 力度决定：力度小的音符更透明，力度大的音符更实。

### 输出文件

- 视频写入**当前工作目录**，而不是 MIDI 文件所在目录。
- 文件名由 MIDI 文件名（不含扩展名）和渲染参数组成：

  ```text
  <midi-file-stem>_<width>x<height>_<fps>.<extension>
  ```

  例如，对 `path/to/Nocturne.mid` 以 1080p60 渲染，会得到 `./Nocturne_1920x1080_60.mp4`。

- 同名文件会被覆盖。
- 编码由 FFmpeg 完成：Ranim 通过管道把原始 RGBA 帧发给 FFmpeg。各格式使用的编码如下：

  | 格式   | 编码器                     | 像素格式       | Alpha |
  | ------ | -------------------------- | -------------- | ----- |
  | `mp4`  | `libx264`                  | `yuv420p`      | 否    |
  | `mov`  | `prores_ks`（ProRes 4444） | `yuva444p10le` | 是    |
  | `webm` | `libvpx-vp9`               | `yuva420p`     | 是    |
  | `gif`  | `gif`                      | `rgb8`         | 否    |

  用于合成时，带 Alpha 的 WebM 通常是体积和兼容性折中的最佳选择。ProRes 4444 能保留 Alpha，但文件会非常大。

## 使用示例

示例使用 Bash 风格的反斜杠 `\` 续行。在 PowerShell 中，请把每条命令写成一行，或在行尾使用反引号（`` ` ``）续行。

```bash
# 先预览
ranim-midi-visualizer preview song.mid

# 基本的 1080p60 MP4
ranim-midi-visualizer render song.mid

# 4K 渲染，自定义背景和配色
ranim-midi-visualizer render song.mid \
  -s 4k --fps 60 \
  --bg '#101014' \
  --fg '#89b9eb' '#9be347' '#f7931e' '#f7c71e'

# 透明 WebM，便于叠加到乐谱或背景视频上
ranim-midi-visualizer render song.mid -f webm --bg transparent -s 1080p

# 按拍滚动，前后各留 1 秒
ranim-midi-visualizer render song.mid -m beat --scroll_speed 2 --buf_time 1 1

# 按轨道分配音符颜色，并自定义进度条
ranim-midi-visualizer render song.mid \
  --color_by track \
  --progress_bar_fg '#ff8800' \
  --progress_bar_bg '#00000080'

# 生成小尺寸 GIF 预览（帧率最高 50）
ranim-midi-visualizer render song.mid -f gif --fps 30 -s 720p

# 自定义分辨率
ranim-midi-visualizer render song.mid -s 2560 1440 --fps 30

# 增加渲染缓冲（占用更多显存）
ranim-midi-visualizer render song.mid --buf 4

# 不安装，直接从源码构建并运行
cargo run --release --locked -- render song.mid -s 720p
```

## 工作原理

1. **解析 MIDI 文件。** `waveform-utils` 使用 [`midly`](https://crates.io/crates/midly) 读取所有轨道。音符的 NoteOn/NoteOff 按通道和音高配对，并以 tick 区间的形式存入区间树。力度为 0 的 NoteOn 会被当作 NoteOff 处理。
2. **构建速度映射。** Set Tempo 元事件会生成一个从 MIDI tick 到纳秒级时间单位的分段线性映射。该映射用于 `time` 滚动模式和状态栏。
3. **提取踏板事件。** 控制器 64、66、67 分别转换为延音、持音、弱音踏板事件，深度归一化到 `0..=1`。
4. **构建 Ranim 场景。** `ranim-midi-visualizer_lib` 创建静态布局（键盘、状态栏、进度条、踏板），并为每个音符添加动画序列。音符矩形向下平移，长度由音符时值决定，颜色和不透明度由当前配色和力度决定。
5. **使用 `wgpu` 渲染帧。** Ranim 按指定帧率求值场景，在 GPU 上离屏渲染每一帧，再把 RGBA 像素读回 CPU。
6. **调用 FFmpeg 编码。** 原始帧通过管道传给 FFmpeg 子进程，由它编码为所选容器和编码格式。终端会显示当前帧和预计剩余时间。
7. **用预览代替渲染。** `preview` 子命令在 `RanimPreviewApp` 中构建同一个场景，并在桌面窗口中进行交互式查看。

场景构建是确定性的：相同的 MIDI、配置和 Ranim 提交会得到相同的动画。

## 项目结构

本仓库是一个 Cargo workspace：

| 路径                                | 说明                                                                                                                                          |
| ----------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/main.rs`                       | 命令行入口：参数解析、日志初始化、配置转换，以及 `render`/`preview` 的分发。                                                                  |
| `crates/ranim-midi-visualizer_lib`  | 可视化核心：场景构建、音符动画，以及公开的配置类型（`MidiVisualizerConfig`、`NoteConfig`、`ColorBy`、`MetricBase`、`ProgressBarConfig` 等）。 |
| `crates/waveform-utils`             | MIDI 解析与音乐模型：音符、声部、轨道、控制/踏板事件、速度映射、音符统计，以及波形/合成相关工具。                                             |
| `crates/ranim_music`                | 可复用的 Ranim 音乐可视化元素：88 键 `PianoKeyboard` 和 `PianoPedals` 显示。                                                                  |
| `crates/ranim-midi-visualizer_math` | 动画代码使用的小型数学工具：循环索引、分段线性函数、阶梯函数、插值、步进网格。                                                                |
| `crates/simple-interval-tree`       | 基于两个 `BTreeMap` 和 `SlotMap` 实现的区间树，支持重叠区间查询和相同区间多值存储。                                                           |
| `crates/music-utils`                | 音高与键盘辅助函数，如黑键判断、八度范围、键位索引转换。                                                                                      |
| `justfile`                          | 可选的开发命令（`build`、`run`、`test`、`lint` 等），配置为 Windows PowerShell。                                                              |
| `Cargo.toml`                        | workspace 定义、共享依赖版本，以及固定的 Ranim Git 提交。                                                                                     |

## MIDI 支持与限制

- **仅支持标准 MIDI 文件。** 不支持 MIDI 2.0 文件和非 MIDI 输入。
- **主要面向格式 0 和格式 1。** 格式 2（独立序列）没有实际支持，因为解析器会把所有轨道合并成一首曲子。
- **支持速度变化。** `Set Tempo` 元事件会被读取并用于时间映射。速度事件是全局收集的；如果各轨道声明了互相冲突的速度映射，时间可能不准确。
- **音符配对** 以 `(通道, 音高)` 为单位。结束音符既可以是 `NoteOff` 事件，也可以是力度为 0 的 `NoteOn` 事件。
- **踏板**：可视化 CC64（延音）、CC66（持音）、CC67（弱音）。其他控制器会被忽略。
- **力度** 只影响音符不透明度，不影响时间或音高。
- **音色、音源库和音频** 不会被处理。渲染结果没有声音，也不进行任何乐器合成。
- **键盘范围** 固定为标准 88 键 A0–C8（MIDI 音符 21–108）。超出该范围的音符不会在键盘上显示。
- **布局固定**：进度条在顶部，状态栏在底部，键盘位于状态栏上方，踏板位于音符区域右下角。
- **输出位置和命名固定** 为当前目录和 `<stem>_<width>x<height>_<fps>` 模板，目前无法通过命令行修改。
- **透明输出** 仅 WebM 和 MOV 支持。

## 日志

程序使用 `tracing` 输出日志，可通过 `RUST_LOG` 环境变量控制详细程度：

```bash
# 全局 debug 日志
RUST_LOG=debug ranim-midi-visualizer render song.mid

# 按模块设置日志级别
RUST_LOG=ranim=debug,ranim_midi_visualizer_lib=trace,waveform_utils=info \
  ranim-midi-visualizer render song.mid
```

如果 `RUST_LOG` 中没有提到某个模块，则使用以下默认级别：

| 模块                        | 默认级别 |
| --------------------------- | -------- |
| `ranim_midi_visualizer_lib` | `DEBUG`  |
| `ranim_midi_visualizer`     | `INFO`   |
| `ranim`                     | `INFO`   |
| `ranim_cli`                 | `INFO`   |
| `waveform_utils`            | `ERROR`  |

PowerShell 中：

```powershell
$env:RUST_LOG = "ranim=debug,ranim_midi_visualizer_lib=trace"
ranim-midi-visualizer render song.mid
```

## 常见问题

### 找不到 `ffmpeg`，或自动下载失败

请安装 FFmpeg，并确认在同一个终端中执行 `ffmpeg -version` 能正常工作：

- Windows：从 [ffmpeg.org](https://ffmpeg.org/download.html) 下载，或使用 `winget`/`choco` 等包管理器安装，然后把 `bin` 目录加入 `PATH`。
- macOS：`brew install ffmpeg`
- Debian/Ubuntu：`sudo apt install ffmpeg`

也可以把名为 `ffmpeg` 的静态可执行文件放到运行渲染命令的目录下，Ranim 会把它作为后备。

### 提示找不到 GPU 适配器（`No suitable GPU adapter`）或 `wgpu` 初始化失败

请更新显卡驱动，并确认对应图形后端可用：

- Windows：Direct3D 12 或 Vulkan
- macOS：Metal
- Linux：Vulkan（可用 `vulkan-tools` 中的 `vulkaninfo` 检查）

虚拟机或远程会话可能无法提供可用的 GPU。

### 透明背景渲染出来是黑色

MP4（`yuv420p`）和 GIF 不能存储 Alpha 通道。请改用：

```bash
ranim-midi-visualizer render song.mid -f webm --bg transparent
# 或
ranim-midi-visualizer render song.mid -f mov --bg transparent
```

然后在剪辑软件中确认透明通道被正确保留。

### 报错 `unexpected argument '--metric-base'`

长参数名使用下划线，与源码中的定义一致：

```bash
--metric_base --scroll_speed --buf_time --progress_bar_fg --progress_bar_bg --color_by
```

### 状态栏字体不对

状态栏使用以下字体回退列表：

```text
Maple Mono NF, Cascadia Code NF, LXGW WenKai Mono, Consolas, Monaco, Courier New
```

安装前三种字体之一可以获得预期效果；否则系统可能回退到其他等宽字体。

### 渲染很慢

- 降低分辨率或帧率。
- 使用 `--release` 构建。
- 如果瓶颈是显存，减小 `--buf`。
- 关闭其他占用 GPU 的程序。
- 带 Alpha 的 WebM/VP9 和 ProRes 4444 MOV 明显比 MP4 更慢、更大。

### 在无图形界面的 Linux 上预览失败

`preview` 通过 `winit` 打开真实桌面窗口，需要 X11/Wayland 会话和 GPU。无头服务器上请使用 `render`，或在具备 GPU 访问的虚拟显示环境中运行预览。

## 开发

安装 [`just`](https://github.com/casey/just) 后可以使用：

```bash
just build       # cargo build --release
just run render path/to/song.mid
just test        # cargo test --workspace
just lint        # 先 cargo fmt --all，再以 -D warnings 运行 clippy
just fmt         # cargo fmt --all
```

不使用 `just` 时，直接运行底层 Cargo 命令：

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

仓库在 `crates/waveform-utils/src/music/tests/` 下提供了 MIDI 测试文件，可用于手动测试：

```bash
cargo run --release -- render crates/waveform-utils/src/music/tests/song_2.mid
```

## 许可证

本项目使用 MIT 许可证，详见 [LICENSE.txt](LICENSE.txt)。

Copyright © 2026 F.X.P.

## 致谢

- [Ranim](https://github.com/AzurIce/ranim) 及其作者 **@AzurIce**：为本项目提供动画引擎和渲染管线。
- [`@kosua20` 的 MIDI Visualizer](https://github.com/kosua20/MIDIVisualizer)：这类瀑布流可视化的灵感来源。
- [Manim](https://github.com/3b1b/manim) 与 [JAnim](https://github.com/jkjkil4/JAnim)：启发了 Ranim 的动画引擎。
- **Wiwi Kuan（好和弦）**：`NPS` 和 `LEGATO` 指标参考了 [Pianometer](https://nicechord.com/pianometer/) 键盘显示程序的设计。
- Rust 生态：`midly`、`clap`、`wgpu`、`egui`/`eframe`、`typst`，以及本项目依赖的众多其他 crate。
