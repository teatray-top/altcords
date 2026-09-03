use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

// Audio buffered before playback is armed, as a jitter cushion.
const PREBUFFER_SECONDS: f64 = 0.4;
// Rate the TTS engine emits at; the resampler maps this to the device mix rate.
const SRC_RATE: u32 = 24000;

// Output conditioning (applied in the callback, after the gain multiply).
// Soft limiter: pass |x|<=KNEE untouched, then tanh-saturate toward 1.0 so a high
// output-volume boosts loudness without the harsh hard-clip distortion.
const LIMIT_KNEE: f32 = 0.6;
// Noise gate (downward expander): the TTS floor (~-60dB) becomes audible hiss when
// the volume is raised, so attenuate the between-words quiet to GATE_FLOOR. Speech
// (pre-gain |x| above GATE_OPEN) opens the gate fast; it closes slowly to avoid pumping.
const GATE_OPEN: f32 = 0.02;
const GATE_FLOOR: f32 = 0.25;
const GATE_ATTACK: f32 = 0.4;
const GATE_RELEASE: f32 = 0.0006;

fn soft_clip(x: f32) -> f32 {
    let a = x.abs();
    if a <= LIMIT_KNEE {
        x
    } else {
        (LIMIT_KNEE + (1.0 - LIMIT_KNEE) * ((a - LIMIT_KNEE) / (1.0 - LIMIT_KNEE)).tanh()).copysign(x)
    }
}

/// Playback-device name fragments tried when the config names none. Any
/// loopback driver works; these are the common ones.
const VIRTUAL_OUTPUTS: [&str; 5] = [
    "CABLE Input",
    "VoiceMeeter Input",
    "VoiceMeeter Aux Input",
    "VoiceMeeter VAIO3 Input",
    "Virtual Audio Cable",
];

static PREFERRED: Mutex<String> = Mutex::new(String::new());

/// Set the device-name fragment to look for. Empty restores the search over
/// [`VIRTUAL_OUTPUTS`].
pub fn set_output_device(name: &str) {
    *PREFERRED.lock().unwrap() = name.to_string();
}

/// Find the device to play into: the configured name if one is set, otherwise
/// the first virtual output present.
fn find_output_device() -> anyhow::Result<cpal::Device> {
    let host = cpal::default_host();
    let preferred = PREFERRED.lock().unwrap().clone();
    let devices: Vec<cpal::Device> = host.output_devices()?.collect();
    if !preferred.is_empty() {
        for device in &devices {
            if device.to_string().contains(&preferred) {
                return Ok(device.clone());
            }
        }
        anyhow::bail!("output device matching {preferred:?} not found");
    }
    for want in VIRTUAL_OUTPUTS {
        for device in &devices {
            if device.to_string().contains(want) {
                return Ok(device.clone());
            }
        }
    }
    anyhow::bail!(
        "no virtual audio device found. Install one (VB-Cable, VoiceMeeter, …) \
         or set output_device in the config to the playback device to use"
    )
}

/// Phase-continuous linear resampler. Resampling each streamed chunk in isolation
/// resets the interpolation phase to zero at every seam, injecting a small
/// discontinuity per chunk (audible as seam glitches at non-integer rate ratios).
/// This carries the fractional read position and the previous chunk's last sample
/// across calls, so joined chunks reconstruct exactly what a one-shot resample of
/// the whole stream would produce.
struct Resampler {
    ratio: f64,
    pos: f64,
    prev: f32,
    passthrough: bool,
}

impl Resampler {
    fn new(from_hz: u32, to_hz: u32) -> Self {
        Self {
            ratio: from_hz as f64 / to_hz as f64,
            pos: 0.0,
            prev: 0.0,
            passthrough: from_hz == to_hz,
        }
    }

    /// Resample `samples` into `out`, emitting only outputs whose interpolation
    /// neighbour is already available; positions that need the next chunk's first
    /// sample are produced once that chunk arrives.
    fn process(&mut self, samples: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(samples);
            return;
        }
        if samples.is_empty() {
            return;
        }
        let l = samples.len();
        while self.pos < (l - 1) as f64 {
            let floor = self.pos.floor();
            let idx = floor as isize;
            let frac = (self.pos - floor) as f32;
            let a = if idx < 0 { self.prev } else { samples[idx as usize] };
            let b = samples[(idx + 1) as usize];
            out.push(a + (b - a) * frac);
            self.pos += self.ratio;
        }
        // Rebase into the next chunk's coordinate frame; pos in [-1, 0) then
        // interpolates prev↔next across the join.
        self.pos -= l as f64;
        self.prev = samples[l - 1];
    }
}

/// Plays queued audio to the virtual output; started warm (silent) at construction and
/// armed to drain the queue once the prebuffer fills.
pub struct StreamPlayer {
    #[allow(dead_code)] // held to keep the cpal stream alive
    stream: cpal::Stream,
    queue: Arc<Mutex<VecDeque<f32>>>,
    armed: Arc<AtomicBool>,
    underruns: Arc<AtomicUsize>,
    resampler: Resampler,
    out_rate: u32,
    prebuffer_samples: usize,
    pushed_chunks: usize,
    pushed_samples: usize,
}

impl StreamPlayer {
    /// Open the output device and start the stream warm (silent until armed). The
    /// callback goes silent the instant `epoch` moves off `my_epoch` (barge-in),
    /// so a stop hotkey cuts audio mid-chunk, not at the next chunk boundary.
    pub fn new(epoch: Arc<AtomicU64>, my_epoch: u64, volume: Arc<AtomicU32>) -> anyhow::Result<Self> {
        let device = find_output_device()?;

        // WASAPI shared mode only accepts the audio engine's actual mix format.
        let config = device.default_output_config()?;
        let out_rate = config.sample_rate();
        let channels = config.channels() as usize;
        let sample_format = config.sample_format();
        let stream_config = config.config();

        let queue: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let armed = Arc::new(AtomicBool::new(false));
        let underruns = Arc::new(AtomicUsize::new(0));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &stream_config, channels, queue.clone(), armed.clone(), epoch.clone(), my_epoch, volume.clone(), underruns.clone())?,
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &stream_config, channels, queue.clone(), armed.clone(), epoch.clone(), my_epoch, volume.clone(), underruns.clone())?,
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &stream_config, channels, queue.clone(), armed.clone(), epoch.clone(), my_epoch, volume.clone(), underruns.clone())?,
            other => anyhow::bail!("unsupported output sample format: {other:?}"),
        };
        // Warm the device now: run the stream (silent until armed) during generation.
        stream.play()?;

        let prebuffer_samples = (out_rate as f64 * PREBUFFER_SECONDS) as usize;
        Ok(Self {
            stream,
            queue,
            armed,
            underruns,
            resampler: Resampler::new(SRC_RATE, out_rate),
            out_rate,
            prebuffer_samples,
            pushed_chunks: 0,
            pushed_samples: 0,
        })
    }

    /// Queue an audio segment (non-blocking); arms playback once the prebuffer fills.
    pub fn push(&mut self, samples: &[f32], src_rate: u32) -> anyhow::Result<()> {
        debug_assert_eq!(src_rate, SRC_RATE, "resampler is fixed to the engine rate");
        let mut resampled = Vec::new();
        self.resampler.process(samples, &mut resampled);
        self.pushed_chunks += 1;
        self.pushed_samples += resampled.len();
        let queued = {
            let mut q = self.queue.lock().unwrap();
            q.extend(resampled);
            q.len()
        };
        if queued >= self.prebuffer_samples {
            self.armed.store(true, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Blocks until every queued sample has been consumed, then logs a one-line
    /// stream health summary (chunk count, duration, mid-stream underrun) so an
    /// intermittent stutter is pinpointable from the log after the fact.
    pub fn wait_until_drained(&mut self) -> anyhow::Result<()> {
        // A short utterance may never reach the prebuffer; arm so it flushes.
        self.armed.store(true, Ordering::Relaxed);
        while !self.queue.lock().unwrap().is_empty() {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // Snapshot before the deliberate tail sleep so it counts only underruns
        // that happened while audio was still playing (generation couldn't keep up).
        let ur = self.underruns.load(Ordering::Relaxed);
        std::thread::sleep(std::time::Duration::from_millis(150));
        crate::applog::log(&format!(
            "stream: {} chunks, {:.1}s out, underrun {}ms",
            self.pushed_chunks,
            self.pushed_samples as f64 / self.out_rate as f64,
            ur as u64 * 1000 / self.out_rate.max(1) as u64,
        ));
        Ok(())
    }
}

/// Build the output stream; its callback drains the queue only once armed.
#[allow(clippy::too_many_arguments)]
fn build_stream<T: cpal::SizedSample + cpal::FromSample<f32>>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    channels: usize,
    queue: Arc<Mutex<VecDeque<f32>>>,
    armed: Arc<AtomicBool>,
    epoch: Arc<AtomicU64>,
    my_epoch: u64,
    volume: Arc<AtomicU32>,
    underruns: Arc<AtomicUsize>,
) -> anyhow::Result<cpal::Stream> {
    let mut gate = 1.0f32; // noise-gate envelope, persists across callbacks
    let stream = device.build_output_stream(
        *config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            // Emit silence (without draining) until armed, and the instant a stop
            // bumps the epoch past this utterance's — that's the mid-chunk barge-in.
            let live = armed.load(Ordering::Relaxed) && epoch.load(Ordering::Relaxed) == my_epoch;
            let vol = f32::from_bits(volume.load(Ordering::Relaxed));
            let mut q = queue.lock().unwrap();
            let mut ur = 0usize;
            for frame in data.chunks_mut(channels) {
                let raw = if live {
                    match q.pop_front() {
                        Some(v) => v,
                        None => {
                            ur += 1;
                            0.0
                        }
                    }
                } else {
                    0.0
                };
                // Gate the quiet floor (reduces amplified hiss), then gain + soft-limit.
                let target = if raw.abs() > GATE_OPEN { 1.0 } else { GATE_FLOOR };
                let coeff = if target > gate { GATE_ATTACK } else { GATE_RELEASE };
                gate += (target - gate) * coeff;
                let s = T::from_sample(soft_clip(raw * vol * gate));
                for out in frame.iter_mut() {
                    *out = s;
                }
            }
            if ur > 0 {
                underruns.fetch_add(ur, Ordering::Relaxed);
            }
        },
        |err| eprintln!("playback stream error: {err}"),
        None,
    )?;
    Ok(stream)
}
