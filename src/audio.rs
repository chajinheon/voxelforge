//! Audio output, voice admission, mixing, and the offline WAV self-test.

use std::io::{self, Write};
use std::num::NonZero;
use std::path::Path;

use rodio::buffer::SamplesBuffer;
use rodio::{DeviceSinkBuilder, MixerDeviceSink};

pub mod synth;

pub use synth::{Effect, Pcm, SAMPLE_RATE, event_seed, generate};

pub const MAX_AUDIO_VOICES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceKind {
    Water,
    Other,
}

/// A deterministic cue emitted by the movement scheduler.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MovementCue {
    pub effect: Effect,
    pub kind: VoiceKind,
    pub gain: f32,
    pub block_id: u16,
}

/// Tracks horizontal walking distance without depending on a device or game
/// window.  The caller supplies one frame's horizontal displacement and the
/// resulting movement state after physics has run.
#[derive(Clone, Debug, Default)]
pub struct MovementAudio {
    distance: f32,
    was_in_water: bool,
    initialized: bool,
}

impl MovementAudio {
    pub fn new() -> Self {
        Self::default()
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        horizontal_distance: f32,
        on_ground: bool,
        flying: bool,
        in_water: bool,
        ground_block: u16,
        sprinting: bool,
        paused: bool,
    ) -> Vec<MovementCue> {
        let mut cues = Vec::new();
        if !self.initialized {
            self.was_in_water = in_water;
            self.initialized = true;
        } else if self.was_in_water != in_water {
            if !paused && !flying {
                cues.push(MovementCue {
                    effect: Effect::Water,
                    kind: VoiceKind::Water,
                    gain: 1.0,
                    block_id: crate::world::block::WATER,
                });
            }
            self.distance = 0.0;
            self.was_in_water = in_water;
        }

        let distance = horizontal_distance.max(0.0);
        if paused || flying || (!in_water && !on_ground) {
            return cues;
        }
        self.distance += distance;
        let stride = if in_water {
            0.9
        } else if sprinting {
            1.3
        } else {
            1.8
        };
        while self.distance >= stride {
            self.distance -= stride;
            cues.push(MovementCue {
                effect: if in_water {
                    Effect::Water
                } else {
                    Effect::Footstep
                },
                kind: if in_water {
                    VoiceKind::Water
                } else {
                    VoiceKind::Other
                },
                gain: if in_water { 0.6 } else { 1.0 },
                block_id: if in_water {
                    crate::world::block::WATER
                } else {
                    ground_block
                },
            });
        }
        cues
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoiceId(u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ActiveVoice {
    id: VoiceId,
    kind: VoiceKind,
    started_at: u64,
    finishes_at: Option<u64>,
}

/// Pure voice admission policy.  It is independent of rodio and can be used
/// by simulation/replay code without touching an OS audio device.
#[derive(Clone, Debug)]
pub struct VoiceCap {
    voices: Vec<ActiveVoice>,
    next_id: u64,
}

impl Default for VoiceCap {
    fn default() -> Self {
        Self::new()
    }
}

impl VoiceCap {
    pub fn new() -> Self {
        Self {
            voices: Vec::with_capacity(MAX_AUDIO_VOICES),
            next_id: 1,
        }
    }

    pub fn len(&self) -> usize {
        self.voices.len()
    }

    pub fn is_empty(&self) -> bool {
        self.voices.is_empty()
    }

    pub fn contains(&self, id: VoiceId) -> bool {
        self.voices.iter().any(|voice| voice.id == id)
    }

    pub fn admit(&mut self, kind: VoiceKind, started_at: u64) -> Option<VoiceId> {
        if self.voices.len() == MAX_AUDIO_VOICES {
            let index = self
                .voices
                .iter()
                .enumerate()
                .filter(|(_, voice)| voice.kind != VoiceKind::Water)
                .min_by_key(|(_, voice)| voice.started_at)
                .map(|(index, _)| index)?;
            self.voices.remove(index);
        }
        let id = VoiceId(self.next_id);
        self.next_id = self.next_id.wrapping_add(1).max(1);
        self.voices.push(ActiveVoice {
            id,
            kind,
            started_at,
            finishes_at: None,
        });
        Some(id)
    }

    /// Admit a finite one-shot and retire any voices whose playback deadline
    /// has already passed. Runtime callers use frame ticks for both values.
    pub fn admit_until(
        &mut self,
        kind: VoiceKind,
        started_at: u64,
        finishes_at: u64,
    ) -> Option<VoiceId> {
        self.voices
            .retain(|voice| voice.finishes_at.is_none_or(|end| end > started_at));
        let id = self.admit(kind, started_at)?;
        if let Some(voice) = self.voices.iter_mut().find(|voice| voice.id == id) {
            voice.finishes_at = Some(finishes_at.max(started_at.saturating_add(1)));
        }
        Some(id)
    }

    pub fn finish(&mut self, id: VoiceId) -> bool {
        let Some(index) = self.voices.iter().position(|voice| voice.id == id) else {
            return false;
        };
        self.voices.remove(index);
        true
    }
}

/// Rodio output wrapper.  Lack of a default device is intentionally a normal
/// state: the caller can keep running and receive an unavailable silent sink.
pub struct AudioOutput {
    sink: Option<MixerDeviceSink>,
    pub voices: VoiceCap,
    master_volume: f32,
}

impl AudioOutput {
    pub fn new() -> Self {
        match DeviceSinkBuilder::open_default_sink() {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                Self {
                    sink: Some(sink),
                    voices: VoiceCap::new(),
                    master_volume: 1.0,
                }
            }
            Err(error) => {
                log::warn!("audio unavailable: {error}; continuing silently");
                Self::silent()
            }
        }
    }

    pub fn silent() -> Self {
        Self {
            sink: None,
            voices: VoiceCap::new(),
            master_volume: 1.0,
        }
    }

    pub fn is_available(&self) -> bool {
        self.sink.is_some()
    }

    pub fn set_master_volume(&mut self, volume: f32) {
        self.master_volume = volume.clamp(0.0, 1.0);
    }

    /// Play a mono buffer with equal-power stereo panning in `[-1, 1]`.
    pub fn play(
        &mut self,
        pcm: &Pcm,
        pan: f32,
        kind: VoiceKind,
        started_at: u64,
    ) -> Option<VoiceId> {
        self.play_with_gain(pcm, pan, kind, started_at, 1.0)
    }

    pub fn play_with_gain(
        &mut self,
        pcm: &Pcm,
        pan: f32,
        kind: VoiceKind,
        started_at: u64,
        gain: f32,
    ) -> Option<VoiceId> {
        let duration_frames = ((pcm.samples.len() as f64 / f64::from(pcm.sample_rate)) * 60.0)
            .ceil()
            .max(1.0) as u64;
        let id = self.voices.admit_until(
            kind,
            started_at,
            started_at.saturating_add(duration_frames),
        )?;
        let Some(sink) = self.sink.as_ref() else {
            return Some(id);
        };
        let pan = pan.clamp(-1.0, 1.0);
        let angle = (pan + 1.0) * std::f32::consts::FRAC_PI_4;
        let left_gain = angle.cos() * self.master_volume;
        let right_gain = angle.sin() * self.master_volume;
        let gain = if gain.is_finite() {
            gain.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut stereo = Vec::with_capacity(pcm.samples.len() * 2);
        for &sample in &pcm.samples {
            stereo.push(sample * left_gain * gain);
            stereo.push(sample * right_gain * gain);
        }
        let Some(channels) = NonZero::new(2_u16) else {
            return Some(id);
        };
        let Some(sample_rate) = NonZero::new(pcm.sample_rate) else {
            return Some(id);
        };
        sink.mixer()
            .add(SamplesBuffer::new(channels, sample_rate, stereo));
        Some(id)
    }

    pub fn finish(&mut self, id: VoiceId) -> bool {
        self.voices.finish(id)
    }
}

impl Default for AudioOutput {
    fn default() -> Self {
        Self::new()
    }
}

/// Mix mono voices into a clamped mono output buffer.  Missing tail samples
/// are treated as silence, matching rodio's source completion semantics.
pub fn mix_mono(voices: &[&Pcm]) -> Pcm {
    let length = voices
        .iter()
        .map(|pcm| pcm.samples.len())
        .max()
        .unwrap_or(0);
    let mut samples = vec![0.0; length];
    for pcm in voices {
        for (out, &sample) in samples.iter_mut().zip(&pcm.samples) {
            *out += sample;
        }
    }
    Pcm {
        samples: synth::clamp_buffer(samples),
        sample_rate: SAMPLE_RATE,
    }
}

/// Write signed 16-bit mono RIFF/WAVE PCM for the `--audio-self-test` path.
pub fn write_wav(path: impl AsRef<Path>, pcm: &Pcm) -> io::Result<()> {
    let samples: Vec<i16> = synth::clamp_buffer(pcm.samples.clone())
        .into_iter()
        .map(|sample| (sample * i16::MAX as f32).round() as i16)
        .collect();
    let data_bytes = samples.len().saturating_mul(2);
    let riff_size = 36_u32.saturating_add(data_bytes as u32);
    let mut file = std::fs::File::create(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&riff_size.to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16_u32.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&1_u16.to_le_bytes())?;
    file.write_all(&pcm.sample_rate.to_le_bytes())?;
    file.write_all(&(pcm.sample_rate.saturating_mul(2)).to_le_bytes())?;
    file.write_all(&2_u16.to_le_bytes())?;
    file.write_all(&16_u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&(data_bytes as u32).to_le_bytes())?;
    for sample in samples {
        file.write_all(&sample.to_le_bytes())?;
    }
    file.flush()
}

/// Generate a deterministic multi-effect diagnostic and write it to WAV.
pub fn write_self_test_wav(path: impl AsRef<Path>, world_seed: u64) -> io::Result<()> {
    let seed = event_seed(world_seed, 0, 1);
    let pcm = mix_mono(&[
        &generate(Effect::Footstep, seed),
        &generate(Effect::Break, seed ^ 1),
        &generate(
            Effect::Place {
                pane_or_fence: false,
            },
            seed ^ 2,
        ),
        &generate(Effect::Water, seed ^ 3),
        &generate(Effect::Inventory, seed ^ 4),
        &generate(Effect::Switch, seed ^ 5),
    ]);
    write_wav(path, &pcm)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn voice_cap_removes_oldest_non_water_voice() {
        let mut cap = VoiceCap::new();
        let first = cap.admit(VoiceKind::Other, 1).expect("voice admitted");
        for timestamp in 2..=16 {
            cap.admit(VoiceKind::Other, timestamp)
                .expect("voice admitted");
        }
        let water = cap.admit(VoiceKind::Water, 17).expect("voice admitted");
        assert_eq!(cap.len(), MAX_AUDIO_VOICES);
        assert!(!cap.contains(first));
        assert!(cap.contains(water));
    }

    #[test]
    fn all_water_voice_cap_rejects_without_evicting_water() {
        let mut cap = VoiceCap::new();
        for timestamp in 0..MAX_AUDIO_VOICES as u64 {
            cap.admit(VoiceKind::Water, timestamp)
                .expect("voice admitted");
        }
        assert!(cap.admit(VoiceKind::Other, 100).is_none());
        assert_eq!(cap.len(), MAX_AUDIO_VOICES);
    }

    #[test]
    fn finite_voices_retire_before_next_admission() {
        let mut cap = VoiceCap::new();
        let expired = cap
            .admit_until(VoiceKind::Other, 10, 12)
            .expect("voice admitted");
        let current = cap
            .admit_until(VoiceKind::Other, 12, 14)
            .expect("voice admitted");
        assert!(!cap.contains(expired));
        assert!(cap.contains(current));
        assert_eq!(cap.len(), 1);
    }

    #[test]
    fn movement_audio_uses_walk_and_sprint_strides() {
        let mut movement = MovementAudio::new();
        assert!(
            movement
                .update(0.0, true, false, false, 1, false, false)
                .is_empty()
        );
        assert!(
            movement
                .update(1.79, true, false, false, 1, false, false)
                .is_empty()
        );
        let cues = movement.update(0.02, true, false, false, 1, false, false);
        assert_eq!(
            cues,
            [MovementCue {
                effect: Effect::Footstep,
                kind: VoiceKind::Other,
                gain: 1.0,
                block_id: crate::world::block::STONE,
            }]
        );

        let mut sprint = MovementAudio::new();
        let _ = sprint.update(0.0, true, false, false, 1, true, false);
        assert_eq!(
            sprint.update(1.3, true, false, false, 1, true, false).len(),
            1
        );
    }

    #[test]
    fn movement_audio_emits_water_transition_and_scaled_steps() {
        let mut movement = MovementAudio::new();
        let _ = movement.update(0.0, true, false, false, 1, false, false);
        let enter = movement.update(0.0, false, false, true, 1, false, false);
        assert_eq!(enter.len(), 1);
        assert_eq!(enter[0].kind, VoiceKind::Water);
        assert_eq!(enter[0].gain, 1.0);
        let step = movement.update(0.9, false, false, true, 1, false, false);
        assert_eq!(step.len(), 1);
        assert_eq!(step[0].gain, 0.6);
        assert!(
            movement
                .update(1.0, false, true, true, 1, false, false)
                .is_empty()
        );
    }

    #[test]
    fn equal_power_pan_has_unit_gain_at_extremes() {
        let left = (std::f32::consts::FRAC_PI_4 * 0.0 + std::f32::consts::FRAC_PI_4).cos();
        let right = (std::f32::consts::FRAC_PI_4 * 2.0).sin();
        assert!((left - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.001);
        assert!((right - 1.0).abs() < 0.001);
    }
}
