//! **M34 / W34-3 — audio to device: positional and ambient layer**
//! (`PROBE-AUDIO-RT-001` blocks C and D; spec
//! `docs/sintergic/game/programming/m34-e2e-integration-presentation-spec-CR-5243af63.md`
//! §5 `:239-257`).
//!
//! **Boundary.** This module types the *presentation* side of sound: it does
//! not decode retail containers (that is `daggerfall-parser-rs` plus the M14
//! production mixer in `daggerfall-app::audio`) and it **never** depends on
//! `daggerfall-app` (forbidden edge `nervous → app`, spine contract §1).
//! The mixer hands over **already decoded PCM**; this layer adds the two things
//! the mixer does not own:
//!
//! 1. **spatial placement** — the DFU laws for ambient water
//!    (`DFU-UP:Assets/Scripts/Game/AmbientEffectsPlayer.cs:165-177@0038a1e6`:
//!    `y = blockWaterLevel * -1 * MeshReader.GlobalScale`, `x,z ± 3 m`,
//!    trigger on `DFRandom.rand() < 50` per `classicUpdateInterval`) and the
//!    attenuation/pan derived from the 3D `AudioSource` Unity played
//!    (`SpatializedPlayOneShot`, `:228-237`: `spatialBlend = 1f`,
//!    `minDistance = 8f`, `maxDistance = minDistance * 8`, linear rolloff);
//! 2. **device output with a measured counter** —
//!    [`AudioDevice::samples_played`] is bumped by the counting source the
//!    platform audio callback *pulls*, so a value `> 0` means the device asked
//!    for samples; it is never a number the probe keeps on its own.
//!
//! ## Why `rodio` (and not bare `cpal` nor `bevy_audio`)
//!
//! The workspace already resolves **one** audio stack: `rodio 0.20.1`
//! (`crates/app/Cargo.toml`, feature `audio`, used by the M14 `RodioSink`).
//! This layer declares *the same* crate, *the same* version and *the same*
//! feature set (`default-features = false, features = ["wav"]`), so the
//! unified graph gains **0 new packages** (`Cargo.lock` already carries
//! `rodio 0.20.1`); `rodio` re-exports `cpal`, which is where the device
//! **name** for the probe's `C device` line is read from. Bare `cpal` would
//! force hand-writing the mixer and the resampling rodio already provides;
//! `bevy/bevy_audio` would pull a **second** rodio into the unified graph,
//! exactly what `crates/game-core/Cargo.toml:44-48` forbids.
//! The dependency is **optional and OFF by default**: workspace `cargo build`/
//! `cargo test` still need no system audio libraries, and without the feature
//! the probe declares `[SKIP-DEVICE]` with its reason.
//!
//! ## What this layer does NOT promise
//!
//! - It is not a 3D engine with HRTF: pan is a constant-power law over **two**
//!   channels and attenuation is the DFU linear one, not `SpatialSink` (which
//!   would need ear positions the game does not model yet).
//! - The DFU `± 3 m` jitter uses `UnityEngine.Random`, which is not
//!   reproducible outside Unity: here it comes from a deterministic LCG
//!   ([`jitter_meters`]) — **declared deviation**, not a DFU formula.

// The device types only exist under the `audio` feature: the imports are gated
// with it so the default build stays at zero warnings.
#[cfg(feature = "audio")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(feature = "audio")]
use std::sync::{mpsc, Arc};
#[cfg(feature = "audio")]
use std::thread::JoinHandle;
#[cfg(feature = "audio")]
use std::time::Duration;

/// `MeshReader.GlobalScale` — meters per world unit
/// (`DFU-UP:Assets/Scripts/MeshReader.cs:58@0038a1e6`).
pub const GLOBAL_SCALE: f32 = 0.025;

/// DFU sentinel for "no water in this block"
/// (`AmbientEffectsPlayer.cs:170@0038a1e6`, `blockWaterLevel != 10000`).
pub const NO_WATER_LEVEL: i32 = 10000;

/// Horizontal water jitter: `Random.Range(-3f, 3f)`
/// (`AmbientEffectsPlayer.cs:174-175@0038a1e6`).
pub const WATER_JITTER_METERS: f32 = 3.0;

/// Water trigger probability per classic step:
/// `DFRandom.rand() < 50` (`AmbientEffectsPlayer.cs:173@0038a1e6`).
pub const WATER_TRIGGER_PERCENT: u32 = 50;

/// DFU classic cadence: `GameManager.classicUpdateInterval = 0.0625`
/// (`DFU-UP:Assets/Scripts/Game/GameManager.cs:42@0038a1e6`).
pub const CLASSIC_UPDATE_INTERVAL: f32 = 0.0625;

/// `minDistance` of the ambient 3D source
/// (`AmbientEffectsPlayer.cs:228@0038a1e6`).
pub const SPATIAL_MIN_DISTANCE: f32 = 8.0;

/// `maxDistance = minDistance * 8` (`:236@0038a1e6`).
pub const SPATIAL_MAX_DISTANCE: f32 = SPATIAL_MIN_DISTANCE * 8.0;

/// Minimum wait of the general ambience, in seconds
/// (`AmbientEffectsPlayer.cs:26@0038a1e6`, `MinWaitTime = 4`).
pub const AMBIENT_MIN_WAIT_SECS: f32 = 4.0;

/// Maximum wait of the general ambience (`:27@0038a1e6`, `MaxWaitTime = 35`).
pub const AMBIENT_MAX_WAIT_SECS: f32 = 35.0;

/// Minimum cemetery wait (`:28@0038a1e6`, `CemeteryMinWaitTime = 1`).
pub const CEMETERY_MIN_WAIT_SECS: f32 = 1.0;

/// Maximum cemetery wait (`:29@0038a1e6`, `CemeteryMaxWaitTime = 80`).
pub const CEMETERY_MAX_WAIT_SECS: f32 = 80.0;

/// Mean wait of the general ambience — the GV-06 literal
/// (`(4 + 35) / 2 = 19.5 s` ⇒ ≈ 30 triggers in 600 s).
pub const AMBIENT_MEAN_WAIT_SECS: f32 =
    (AMBIENT_MIN_WAIT_SECS + AMBIENT_MAX_WAIT_SECS) * 0.5;

/// A typed failure of the device layer. There is never a panic: the runtime
/// degrades and **declares** the reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceError {
/// There is no output device, or the backend refused to open it. The
/// message is **from the backend**, verbatim.
    NoOutput {
/// Text returned by `rodio`/`cpal`.
        backend: String,
    },
/// The audio thread no longer exists (channel closed or startup aborted).
    Stopped {
/// Reason for the shutdown.
        reason: String,
    },
}

impl std::fmt::Display for DeviceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoOutput { backend } => write!(f, "no output device: {backend}"),
            Self::Stopped { reason } => write!(f, "audio worker stopped: {reason}"),
        }
    }
}

impl std::error::Error for DeviceError {}

/// `true` when DFU considers the block to have water
/// (`blockWaterLevel != 10000`, `AmbientEffectsPlayer.cs:170@0038a1e6`).
pub const fn has_water(block_water_level: i32) -> bool {
    block_water_level != NO_WATER_LEVEL
}

/// Height of the water source: `y = blockWaterLevel * -1 * GlobalScale`
/// (`AmbientEffectsPlayer.cs:173@0038a1e6` + `MeshReader.cs:58@0038a1e6`).
///
/// `block_water_level` arrives in world units (negative = below the
/// surface), so `-128` yields `+3.2 m`.
pub fn water_source_y(block_water_level: i32) -> f32 {
    -(block_water_level as f32) * GLOBAL_SCALE
}

/// A uniform `f32` in `[-1, 1)` derived from `seed` and `lane`.
///
/// **Declared deviation:** DFU uses `UnityEngine.Random.Range(-3f, 3f)`, whose
/// generator is not reproducible outside Unity (it owns global state). The
/// probe needs the **same** jitter on both passes, so a documented Numerical
/// Recipes LCG lives here — deterministic, not a DFU formula.
fn signed_unit(seed: u64, lane: u64) -> f32 {
    let mut state = seed
        ^ lane
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add(0x2545_F491_4F6C_DD1D);
    state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
// Top 16 bits: resolution to spare and exact in `f32`.
    let bits = ((state >> 48) & 0xFFFF) as u32;
    let unit = bits as f32 / 65_535.0;
    (unit * 2.0 - 1.0).clamp(-1.0, 1.0)
}

/// Horizontal jitter, in meters: `± WATER_JITTER_METERS`
/// (`AmbientEffectsPlayer.cs:174-175@0038a1e6`).
pub fn jitter_meters(seed: u64, lane: u64) -> f32 {
    signed_unit(seed, lane) * WATER_JITTER_METERS
}

/// Position of the water source: the player's `x,z` plus the DFU jitter, and
/// `y` **always** derived from `blockWaterLevel` (never the player's — DFU
/// only overrides `y`, `:172-175@0038a1e6`).
pub fn water_source_position(
    block_water_level: i32,
    player: [f32; 3],
    seed: u64,
) -> [f32; 3] {
    [
        player[0] + jitter_meters(seed, 0),
        water_source_y(block_water_level),
        player[2] + jitter_meters(seed, 1),
    ]
}

/// Mix of a spatial source with respect to the listener.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialMix {
/// Euclidean distance in meters (`>= 0`).
    pub distance: f32,
/// Stereo pan in `[-1, 1]`; `0.0` only when `x` coincides.
    pub pan: f32,
/// Linear gain in `[0, 1]` (DFU linear rolloff).
    pub gain: f32,
}

/// Positional mix: pan by horizontal offset, gain by distance.
///
/// - **pan** = `clamp((source.x − listener.x) / minDistance, −1, 1)`. DFU leaves
///   pan to the Unity engine (`spatialBlend = 1f`, `:233@0038a1e6`); with two
///   channels, spreading the offset over the source's `minDistance` (`8 m`) is
///   the most conservative reading: from `8 m` of lateral offset on, the source
///   sits in a single channel.
/// - **gain** = **linear** rolloff between `minDistance` (1.0) and
///   `maxDistance = minDistance * 8` (0.0) — the two values DFU fixes
///   (`:235-236@0038a1e6`) with Unity's default rolloff mode.
///
/// Pan is `0.0` **if and only if** `source.x == listener.x`: vertical offset is
/// never invented as panning.
pub fn spatial_mix(source: [f32; 3], listener: [f32; 3]) -> SpatialMix {
    let dx = source[0] - listener[0];
    let dy = source[1] - listener[1];
    let dz = source[2] - listener[2];
    let distance = (dx * dx + dy * dy + dz * dz).sqrt();
    let gain = if distance <= SPATIAL_MIN_DISTANCE {
        1.0
    } else if distance >= SPATIAL_MAX_DISTANCE {
        0.0
    } else {
        (SPATIAL_MAX_DISTANCE - distance) / (SPATIAL_MAX_DISTANCE - SPATIAL_MIN_DISTANCE)
    };
    SpatialMix {
        distance,
        pan: (dx / SPATIAL_MIN_DISTANCE).clamp(-1.0, 1.0),
        gain,
    }
}

/// Per-channel gains of a **constant-power** law for `pan` in `[-1, 1]`
/// (`0.0` = center ⇒ `0.7071` on each channel).
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * std::f32::consts::FRAC_PI_4;
    (angle.cos(), angle.sin())
}

/// Ambience clock: waits `[min, max]` seconds and fires when exhausted.
///
/// DFU draws the wait with `System.Random`
/// (`StartWaiting`, `AmbientEffectsPlayer.cs:45-49@0038a1e6`); here the draw is
/// the deterministic LCG of `signed_unit` mapped to `[min, max)` — the same
/// declared deviation as the jitter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmbientClock {
    min: f32,
    max: f32,
    seed: u64,
    wait: f32,
    counter: f32,
}

impl AmbientClock {
/// General ambience clock: `[4, 35] s` (`:26-27@0038a1e6`).
    pub fn general(seed: u64) -> Self {
        Self::with_range(seed, AMBIENT_MIN_WAIT_SECS, AMBIENT_MAX_WAIT_SECS)
    }

/// Cemetery clock: `[1, 80] s` (`:28-29@0038a1e6`).
    pub fn cemetery(seed: u64) -> Self {
        Self::with_range(seed, CEMETERY_MIN_WAIT_SECS, CEMETERY_MAX_WAIT_SECS)
    }

/// Clock with an explicit range (both DFU ones, or a test one).
    pub fn with_range(seed: u64, min: f32, max: f32) -> Self {
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        let mut clock = Self {
            min,
            max,
            seed,
            wait: 0.0,
            counter: 0.0,
        };
        clock.wait = clock.draw_wait();
        clock
    }

/// Next drawn wait, in seconds.
    fn draw_wait(&mut self) -> f32 {
        self.seed = self.seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let unit = (signed_unit(self.seed, 7) + 1.0) * 0.5;
        self.min + (self.max - self.min) * unit
    }

/// Current wait, in seconds.
    pub fn wait(&self) -> f32 {
        self.wait
    }

/// Advances `dt` seconds; `true` on the step where the ambience fires
/// (and the clock re-arms).
    pub fn tick(&mut self, dt: f32) -> bool {
        self.counter += dt.max(0.0);
        if self.counter >= self.wait {
            self.counter = 0.0;
            self.wait = self.draw_wait();
            true
        } else {
            false
        }
    }
}

/// A clip ready for the device: decoded PCM + its placement.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceClip {
/// Evidence label (`music`, `sfx`, `ambient`).
    pub label: &'static str,
/// **Mono unsigned 8-bit** PCM (128 = silence) — the retail format and the M14
/// synthesizer's.
    pub pcm: Vec<u8>,
/// Sample rate of `pcm`.
    pub sample_rate: u32,
/// Stereo pan in `[-1, 1]`.
    pub pan: f32,
/// Linear gain (clamped to `[0, 4]`).
    pub gain: f32,
}

impl DeviceClip {
/// Centered clip, gain `1.0`.
    pub fn new(label: &'static str, pcm: Vec<u8>, sample_rate: u32) -> Self {
        Self {
            label,
            pcm,
            sample_rate,
            pan: 0.0,
            gain: 1.0,
        }
    }

/// Places the clip in the stereo plane with the mix gain.
    pub fn placed(self, mix: SpatialMix) -> Self {
        Self {
            pan: mix.pan,
            gain: mix.gain,
            ..self
        }
    }

/// Duration in seconds (`pcm` is mono: 1 byte = 1 frame).
    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            return 0.0;
        }
        self.pcm.len() as f64 / f64::from(self.sample_rate)
    }

/// The first `secs` seconds of the clip (for a bounded sample).
    pub fn take_secs(&self, secs: f32) -> Vec<u8> {
        let frames = (f64::from(secs.max(0.0)) * f64::from(self.sample_rate)).round() as usize;
        let frames = frames.min(self.pcm.len());
        self.pcm[..frames].to_vec()
    }
}

/// Audio destination of the device. A failure is **always** a [`DeviceError`]:
/// never a panic, never silence without a reason.
pub trait AudioDevice {
/// Output device name as the backend reports it.
    fn name(&self) -> &str;

/// Samples **handed to the callback** since startup (interleaved: frames ×
/// channels). It only grows if the device asks for samples.
    fn samples_played(&self) -> u64;

/// Queues a clip for immediate playback.
    fn play(&mut self, clip: DeviceClip) -> Result<(), DeviceError>;

/// Cuts whatever is playing and leaves the device ready for the next clip (the
/// counter is **not** reset: it is cumulative).
    fn reset(&mut self);
}

/// Audio thread commands.
#[cfg(feature = "audio")]
enum Command {
/// Plays a clip (mono 8-bit PCM + placement).
    Play(DeviceClip),
/// Stops playback and opens a fresh `Sink`.
    Reset,
}

/// Real device (`rodio`/`cpal`), feature `audio`.
///
/// cpal's `OutputStream` is *thread-affine* (`!Send`), so the stream, the
/// `Sink` and the counting source live on a **dedicated thread**; this type only
/// keeps the command channel, the shared counter and the name.
#[cfg(feature = "audio")]
pub struct RodioDevice {
    name: String,
    samples: Arc<AtomicU64>,
    commands: Option<mpsc::Sender<Command>>,
    worker: Option<JoinHandle<()>>,
}

#[cfg(feature = "audio")]
impl RodioDevice {
/// Opens the default output device.
///
/// `Err(DeviceError::NoOutput)` carries the **backend's** message so the probe
/// can print `[SKIP-DEVICE]` with the real cause.
    pub fn open() -> Result<Self, DeviceError> {
        let (commands, queue) = mpsc::channel::<Command>();
        let (ready_sender, ready) = mpsc::channel::<Result<String, String>>();
        let samples = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&samples);
        let worker = std::thread::Builder::new()
            .name("m34-rodio".to_string())
            .spawn(move || {
                use rodio::cpal::traits::{DeviceTrait, HostTrait};
                let (stream, handle) = match rodio::OutputStream::try_default() {
                    Ok(pair) => pair,
                    Err(err) => {
                        let _ = ready_sender.send(Err(err.to_string()));
                        return;
                    }
                };
                let name = rodio::cpal::default_host()
                    .default_output_device()
                    .and_then(|device| device.name().ok())
                    .unwrap_or_else(|| "default output (unnamed)".to_string());
                let mut sink = match rodio::Sink::try_new(&handle) {
                    Ok(sink) => sink,
                    Err(err) => {
                        let _ = ready_sender.send(Err(err.to_string()));
                        return;
                    }
                };
                let _ = ready_sender.send(Ok(name));
                for command in queue {
                    match command {
                        Command::Play(clip) => {
                            let samples = stereo_samples(&clip);
                            let rate = clip.sample_rate.max(1);
                            let buffer = rodio::buffer::SamplesBuffer::new(2, rate, samples);
                            sink.append(CountingSource::new(buffer, Arc::clone(&counter)));
                        }
                        Command::Reset => {
                            sink.stop();
                            match rodio::Sink::try_new(&handle) {
                                Ok(fresh) => sink = fresh,
                                Err(_) => return,
                            }
                        }
                    }
                }
                drop(stream);
            })
            .map_err(|err| DeviceError::Stopped {
                reason: format!("audio thread: {err}"),
            })?;
        match ready.recv() {
            Ok(Ok(name)) => Ok(Self {
                name,
                samples,
                commands: Some(commands),
                worker: Some(worker),
            }),
            Ok(Err(backend)) => {
                let _ = worker.join();
                Err(DeviceError::NoOutput { backend })
            }
            Err(_) => {
                let _ = worker.join();
                Err(DeviceError::Stopped {
                    reason: "audio thread exited during startup".to_string(),
                })
            }
        }
    }

    fn send(&self, command: Command) -> Result<(), DeviceError> {
        match self.commands.as_ref() {
            Some(commands) => commands.send(command).map_err(|_| DeviceError::Stopped {
                reason: "audio thread stopped".to_string(),
            }),
            None => Err(DeviceError::Stopped {
                reason: "audio thread stopped".to_string(),
            }),
        }
    }
}

#[cfg(feature = "audio")]
impl AudioDevice for RodioDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn samples_played(&self) -> u64 {
        self.samples.load(Ordering::Relaxed)
    }

    fn play(&mut self, clip: DeviceClip) -> Result<(), DeviceError> {
        self.send(Command::Play(clip))
    }

    fn reset(&mut self) {
        let _ = self.send(Command::Reset);
    }
}

#[cfg(feature = "audio")]
impl Drop for RodioDevice {
    fn drop(&mut self) {
// Closing the channel ends the worker loop (and with it the stream).
        self.commands.take();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Unsigned 8-bit mono → interleaved `f32` stereo with constant-power pan and
/// linear gain.
#[cfg(feature = "audio")]
fn stereo_samples(clip: &DeviceClip) -> Vec<f32> {
    let (left, right) = pan_gains(clip.pan);
    let gain = clip.gain.clamp(0.0, 4.0);
    let mut out = Vec::with_capacity(clip.pcm.len() * 2);
    for byte in &clip.pcm {
        let sample = (f32::from(*byte) - 128.0) / 128.0;
        out.push(sample * gain * left);
        out.push(sample * gain * right);
    }
    out
}

/// Source that **counts** every sample the audio callback asks it for.
///
/// The counter is the only proof the device pulled audio: if the stream never
/// starts, `next()` is never called and the counter stays at `0`.
#[cfg(feature = "audio")]
struct CountingSource<S> {
    inner: S,
    counter: Arc<AtomicU64>,
}

#[cfg(feature = "audio")]
impl<S> CountingSource<S> {
    fn new(inner: S, counter: Arc<AtomicU64>) -> Self {
        Self { inner, counter }
    }
}

#[cfg(feature = "audio")]
impl<S> Iterator for CountingSource<S>
where
    S: Iterator,
{
    type Item = S::Item;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.inner.next();
        if item.is_some() {
            self.counter.fetch_add(1, Ordering::Relaxed);
        }
        item
    }
}

#[cfg(feature = "audio")]
impl<S> rodio::Source for CountingSource<S>
where
    S: rodio::Source,
    S::Item: rodio::Sample,
{
    fn current_frame_len(&self) -> Option<usize> {
        self.inner.current_frame_len()
    }

    fn channels(&self) -> u16 {
        self.inner.channels()
    }

    fn sample_rate(&self) -> u32 {
        self.inner.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn water_y_follows_global_scale() {
// DFU: y = blockWaterLevel * -1 * 0.025
        assert_eq!(water_source_y(-128), 3.2);
        assert_eq!(water_source_y(0), 0.0);
        assert_eq!(water_source_y(40), -1.0);
        assert!(!has_water(NO_WATER_LEVEL));
        assert!(has_water(-128));
    }

    #[test]
    fn jitter_stays_within_three_meters_and_is_deterministic() {
        for seed in 0..256u64 {
            for lane in 0..2u64 {
                let jitter = jitter_meters(seed, lane);
                assert!(
                    (-WATER_JITTER_METERS..=WATER_JITTER_METERS).contains(&jitter),
                    "jitter {jitter} out of range for seed {seed} lane {lane}"
                );
                assert_eq!(jitter, jitter_meters(seed, lane));
            }
        }
    }

    #[test]
    fn water_source_overrides_only_y_and_keeps_player_xz() {
        let player = [100.0, 44.0, -7.0];
        let source = water_source_position(-128, player, 0x5EED);
        assert_eq!(source[1], 3.2);
        assert!((source[0] - player[0]).abs() <= WATER_JITTER_METERS);
        assert!((source[2] - player[2]).abs() <= WATER_JITTER_METERS);
    }

    #[test]
    fn pan_is_null_only_when_x_coincides() {
        let listener = [10.0, 0.0, 10.0];
        let same = spatial_mix([10.0, 5.0, 10.0], listener);
        assert_eq!(same.pan, 0.0);
        for offset in [-100.0f32, -3.0, -0.001, 0.001, 3.0, 100.0] {
            let mix = spatial_mix([10.0 + offset, 0.0, 10.0], listener);
            assert!(
                mix.pan != 0.0,
                "pan must be non-null for dx={offset} (got {})",
                mix.pan
            );
            assert!(mix.pan.abs() <= 1.0);
        }
// Sign: to the right of the listener ⇒ positive pan.
        assert!(spatial_mix([20.0, 0.0, 10.0], listener).pan > 0.0);
        assert!(spatial_mix([0.0, 0.0, 10.0], listener).pan < 0.0);
    }

    #[test]
    fn attenuation_is_the_linear_dfu_rolloff() {
        let listener = [0.0, 0.0, 0.0];
        assert_eq!(spatial_mix([0.0, 0.0, 0.0], listener).gain, 1.0);
        assert_eq!(spatial_mix([8.0, 0.0, 0.0], listener).gain, 1.0);
        assert_eq!(spatial_mix([64.0, 0.0, 0.0], listener).gain, 0.0);
        assert_eq!(spatial_mix([200.0, 0.0, 0.0], listener).gain, 0.0);
        let mid = spatial_mix([36.0, 0.0, 0.0], listener).gain;
        assert!((mid - 0.5).abs() < 1e-6, "mid gain {mid}");
    }

    #[test]
    fn pan_gains_are_constant_power() {
        for pan in [-1.0f32, -0.5, 0.0, 0.5, 1.0] {
            let (left, right) = pan_gains(pan);
            let power = left * left + right * right;
            assert!((power - 1.0).abs() < 1e-5, "power {power} at pan {pan}");
        }
        assert!((pan_gains(0.0).0 - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!(pan_gains(1.0).0 < 1e-6);
    }

    #[test]
    fn ambient_clock_waits_inside_the_dfu_ranges() {
        let mut general = AmbientClock::general(0x5EED);
        for _ in 0..512 {
            let wait = general.wait();
            assert!(
                (AMBIENT_MIN_WAIT_SECS..=AMBIENT_MAX_WAIT_SECS).contains(&wait),
                "general wait {wait}"
            );
            assert!(!general.tick(0.0));
        }
        let mut cemetery = AmbientClock::cemetery(0x5EED);
        for _ in 0..512 {
            let wait = cemetery.wait();
            assert!(
                (CEMETERY_MIN_WAIT_SECS..=CEMETERY_MAX_WAIT_SECS).contains(&wait),
                "cemetery wait {wait}"
            );
            assert!(!cemetery.tick(0.0));
        }
    }

    #[test]
    fn ambient_clock_fires_about_thirty_times_in_ten_minutes() {
        let mut clock = AmbientClock::general(0x5EED);
        let dt = 1.0 / 60.0;
        let mut fires = 0usize;
        for _ in 0..36_000 {
            if clock.tick(dt) {
                fires += 1;
            }
        }
// GV-06: 600 s / 19.5 s ≈ 30 (one deterministic pass: fixed value).
        assert!((20..=45).contains(&fires), "fires in 600 s = {fires}");
    }

    #[test]
    fn device_clip_duration_and_trim() {
        let clip = DeviceClip::new("music", vec![128u8; 11_025], 11_025);
        assert!((clip.duration_secs() - 1.0).abs() < 1e-9);
        assert_eq!(clip.take_secs(0.5).len(), 5_513);
        assert_eq!(clip.take_secs(10.0).len(), 11_025);
    }

    #[test]
    fn device_error_is_displayed_with_the_backend_text() {
        let err = DeviceError::NoOutput {
            backend: "ALSA: no such device".to_string(),
        };
        assert!(err.to_string().contains("ALSA: no such device"));
    }
}
