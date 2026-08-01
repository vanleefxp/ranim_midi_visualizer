use std::{
    num::NonZeroUsize,
    ops::Deref,
    path::PathBuf,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use derivative::Derivative;
use gpui::*;
use gpui_util::ResultExt;
use indexmap::IndexSet;
use ranim::Output;
use ranim_midi_visualizer_lib::config::MidiVisualizerConfig;
use tracing::{error, info};
use waveform_utils::{
    envelope::ExpDecay,
    synth::{SimpleWaveformSynth, Synth},
    waveform::Triangle,
};

#[derive(Derivative)]
#[derivative(Debug, Clone, Default)]
pub struct RecentFiles {
    inner: IndexSet<PathBuf>,
    #[derivative(Default(value = "Some(64.try_into().unwrap())"))]
    max_count: Option<NonZeroUsize>,
}

impl Deref for RecentFiles {
    type Target = IndexSet<PathBuf>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RecentFiles {
    pub fn insert(&mut self, path: PathBuf) {
        let (idx, inserted) = self.inner.insert_full(path);
        let len = self.inner.len();
        if !inserted {
            self.inner.swap_indices(idx, len - 1);
        }
        if let Some(max_count) = self.max_count {
            let max_count = max_count.get();
            if len > max_count {
                self.inner
                    .drain(..(len - max_count))
                    .for_each(|v| info!("\"{}\" removed from recent files.", v.display()));
            }
        }
    }

    pub fn clear(&mut self) {
        self.inner
            .drain(..)
            .for_each(|v| info!("\"{}\" removed from recent files.", v.display()));
    }

    pub fn iter(&self) -> indexmap::set::Iter<'_, PathBuf> {
        self.inner.iter()
    }
}

#[derive(Debug, Clone)]
pub struct FileState {
    opened_file: Entity<Option<PathBuf>>,
    recent_files: Entity<RecentFiles>,
}

impl FileState {
    pub fn new(cx: &mut App) -> Self {
        let opened_file = cx.new(|_| None);
        let recent_files = cx.new(|_| RecentFiles::default());
        Self {
            opened_file,
            recent_files,
        }
    }

    pub fn opened_file(&self) -> Entity<Option<PathBuf>> {
        self.opened_file.clone()
    }

    pub fn recent_files(&self) -> Entity<RecentFiles> {
        self.recent_files.clone()
    }

    pub fn set_opened_file(&mut self, path: Option<PathBuf>, cx: &mut App) {
        if let Some(path) = path {
            info!("\"{}\" opened.", path.display());
            self.add_recent_file(path.clone(), cx);
            self.opened_file.update(cx, |v, cx| {
                *v = Some(path);
                cx.notify();
            });
        } else {
            self.opened_file.update(cx, |v, cx| {
                *v = None;
                cx.notify();
            });
        }
    }

    pub fn add_recent_file(&mut self, path: PathBuf, cx: &mut App) {
        self.recent_files.update(cx, |v, cx| {
            v.insert(path);
            cx.notify();
        });
    }

    pub fn clear_recent_files(&mut self, cx: &mut App) {
        self.recent_files.update(cx, |v, cx| {
            v.clear();
            cx.notify();
        });
    }
}

#[derive(Debug, Clone)]
pub struct VideoConfigState {
    pub visualizer_config: Entity<MidiVisualizerConfig>,
    pub export_config: Entity<Output>,
    pub clear_color: Hsla,
}

impl Global for VideoConfigState {}

impl VideoConfigState {
    pub fn new(cx: &mut App) -> Self {
        let visualizer_config = cx.new(|_cx| MidiVisualizerConfig::default());
        let export_config = cx.new(|_cx| Output::default());
        Self {
            visualizer_config,
            export_config,
            clear_color: rgb(0x282c34).into(),
        }
    }
}

pub struct DeviceSettings {
    audio_device: cpal::Device,
    stream_config: cpal::StreamConfig,
}

pub enum AudioSignal {
    DeviceChanged,
    Paused,
    Resumed,
}

pub struct AudioState {
    device_settings: Option<DeviceSettings>,
    // playback_task: Option<Task<()>>,
    pub synth: Arc<Mutex<dyn Synth>>,
    sender: Option<async_channel::Sender<AudioSignal>>,
}

pub struct AudioPlayer {
    stream: cpal::Stream,
    receiver: async_channel::Receiver<AudioSignal>,
}

impl FnOnce<()> for AudioPlayer {
    type Output = ();

    extern "rust-call" fn call_once(self, args: ()) -> Self::Output {
        self.call(args)
    }
}

impl FnMut<()> for AudioPlayer {
    extern "rust-call" fn call_mut(&mut self, args: ()) -> Self::Output {
        self.call(args)
    }
}

impl Fn<()> for AudioPlayer {
    extern "rust-call" fn call(&self, _args: ()) -> Self::Output {
        const AUDIO_THREAD_POLL_PERIOD: Duration = Duration::from_nanos(1_000_000_000 / 60);
        if self.stream.play().log_err().is_some() {
            {
                let cur_thread = thread::current();
                info!(
                    "Stream started in thread \'{}\' ({})!",
                    cur_thread.name().unwrap_or("<unknown>"),
                    cur_thread.id().as_u64(),
                );
            }
            loop {
                match self.receiver.try_recv() {
                    Err(_) => {
                        // No signal passed
                        thread::sleep(AUDIO_THREAD_POLL_PERIOD);
                    }
                    Ok(AudioSignal::DeviceChanged) => {
                        info!("Playback thread finished due to device change.");
                        break;
                    }
                    Ok(AudioSignal::Paused) => {
                        info!("Playback stream paused.");
                        if self.stream.pause().log_err().is_none() {
                            break;
                        }
                    }
                    Ok(AudioSignal::Resumed) => {
                        info!("Playback stream resumed.");
                        if self.stream.play().log_err().is_none() {
                            break;
                        }
                    }
                }
            }
        }
    }
}

impl AudioState {
    pub fn new() -> Self {
        let synth = Arc::new(Mutex::new(
            SimpleWaveformSynth::default()
                .with_envelope(ExpDecay(1.))
                .with_waveform(Triangle),
        ));
        let mut result = Self {
            device_settings: None,
            synth,
            sender: None,
        };

        let host = cpal::default_host();
        if let Some(device) = host.default_output_device() {
            result.set_audio_device(device);
        }
        result.reload();

        result
    }

    pub fn set_audio_device(&mut self, device: cpal::Device) {
        if let Some(sender) = &self.sender {
            sender.send_blocking(AudioSignal::DeviceChanged).log_err();
        }
        let device_settings = if let Some(config) = device.default_output_config().log_err() {
            info!(
                "Audio device: {:?}.",
                device
                    .description()
                    .log_err()
                    .map(|v| v.name().to_string())
                    .unwrap_or("Unknown device".to_string())
            );
            info!("Stream config: {:?}.", config);
            Some(DeviceSettings {
                audio_device: device,
                stream_config: config.into(),
            })
        } else {
            None
        };
        self.device_settings = device_settings;
    }

    pub fn play(&self) {
        if let Some(sender) = &self.sender {
            sender.send_blocking(AudioSignal::Resumed).log_err();
        }
    }

    pub fn pause(&self) {
        if let Some(sender) = &self.sender {
            sender.send_blocking(AudioSignal::Paused).log_err();
        }
    }

    pub fn reload(&mut self) {
        if let Some(device_settings) = &self.device_settings {
            let synth = self.synth.clone();
            let config = device_settings.stream_config.clone();
            let data_callback = move |buffer: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut synth = synth.lock().unwrap();
                synth.write_to_buffer(&config, buffer);
            };
            let error_callback = |err: cpal::StreamError| {
                error!("{}", err);
            };
            if let Some(stream) = device_settings
                .audio_device
                .build_output_stream(
                    &device_settings.stream_config,
                    data_callback,
                    error_callback,
                    None,
                )
                .log_err()
            {
                let (sender, receiver) = async_channel::unbounded();
                self.sender = Some(sender);
                thread::spawn(AudioPlayer { stream, receiver });
            }
        }
    }
}
