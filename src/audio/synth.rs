//! Deterministic procedural sound effects.
//!
//! The game deliberately has no external sound assets.  Every effect is a small
//! 48 kHz mono buffer generated from an event seed, which keeps snapshots and
//! replayed interactions bit-for-bit reproducible.

pub const SAMPLE_RATE: u32 = 48_000;
pub const FOOTSTEP_SAMPLES: usize = 4_320;
pub const BREAK_SAMPLES: usize = 8_640;
pub const PLACE_SAMPLES: usize = 3_360;
pub const WATER_SAMPLES: usize = 12_000;
pub const INVENTORY_SAMPLES: usize = 2_640;
pub const SWITCH_SAMPLES: usize = 1_680;
const SEED_MIX: u64 = 0x9e37_79b9;

/// Effect families emitted by world and UI interaction events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Footstep,
    Break,
    Place { pane_or_fence: bool },
    Water,
    Inventory,
    Switch,
}

/// A mono PCM sound at the engine's fixed sample rate.
#[derive(Clone, Debug, PartialEq)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

impl Pcm {
    fn new(samples: Vec<f32>) -> Self {
        Self {
            samples,
            sample_rate: SAMPLE_RATE,
        }
    }

    pub fn duration_seconds(&self) -> f32 {
        self.samples.len() as f32 / self.sample_rate as f32
    }

    pub fn peak(&self) -> f32 {
        self.samples
            .iter()
            .copied()
            .map(f32::abs)
            .fold(0.0, f32::max)
    }
}

/// The exact event seed contract from the renderer/gameplay design.
pub fn event_seed(world_seed: u64, event_counter: u64, block_id: u16) -> u64 {
    world_seed ^ event_counter ^ u64::from(block_id).wrapping_mul(SEED_MIX)
}

/// Generate one deterministic effect from an already-derived event seed.
pub fn generate(effect: Effect, seed: u64) -> Pcm {
    match effect {
        Effect::Footstep => footstep(seed),
        Effect::Break => break_sound(seed),
        Effect::Place { pane_or_fence } => place(seed, pane_or_fence),
        Effect::Water => water(seed),
        Effect::Inventory => inventory(),
        Effect::Switch => switch(),
    }
}

pub fn footstep(seed: u64) -> Pcm {
    let mut rng = XorShift32::from_seed(seed);
    let mut filtered = 0.0;
    let samples = (0..FOOTSTEP_SAMPLES)
        .map(|index| {
            let t = index as f32 / FOOTSTEP_SAMPLES as f32;
            let noise = rng.next_signed();
            filtered += 0.18 * (noise - filtered);
            0.18 * filtered * (1.0 - t).powi(2)
        })
        .collect();
    Pcm::new(clamp_buffer(samples))
}

pub fn break_sound(seed: u64) -> Pcm {
    let mut rng = XorShift32::from_seed(seed);
    let mut filtered = 0.0;
    let samples = (0..BREAK_SAMPLES)
        .map(|index| {
            let t = index as f32 / BREAK_SAMPLES as f32;
            let noise = rng.next_signed();
            filtered += 0.35 * (noise - filtered);
            let envelope = (1.0 - t).powi(2);
            let impulse = match index {
                0 => 1.0,
                1_680 => 0.72,
                3_360 => 0.52,
                _ => 0.0,
            };
            0.28 * (filtered * envelope + impulse * (1.0 - t))
        })
        .collect();
    Pcm::new(clamp_buffer(samples))
}

pub fn place(seed: u64, pane_or_fence: bool) -> Pcm {
    let phase_offset = (seed as f32 / u32::MAX as f32) * std::f32::consts::TAU;
    let mut previous = 0.0;
    let samples = (0..PLACE_SAMPLES)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            let sine = (std::f32::consts::TAU * 180.0 * t + phase_offset).sin();
            let value = 0.20 * sine * (-35.0 * t).exp();
            let output = if pane_or_fence {
                high_pass(value, &mut previous)
            } else {
                value
            };
            previous = value;
            output
        })
        .collect();
    Pcm::new(clamp_buffer(samples))
}

pub fn water(seed: u64) -> Pcm {
    let mut rng = XorShift32::from_seed(seed);
    let mut filtered = 0.0;
    let samples = (0..WATER_SAMPLES)
        .map(|index| {
            let t = index as f32 / WATER_SAMPLES as f32;
            let attack = (t / 0.020).min(1.0);
            let decay = (1.0 - t).max(0.0).powf(0.8);
            filtered += 0.05 * (rng.next_signed() - filtered);
            0.15 * filtered * attack * decay
        })
        .collect();
    Pcm::new(clamp_buffer(samples))
}

pub fn inventory() -> Pcm {
    let samples = (0..INVENTORY_SAMPLES)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            let frequency = 640.0 + (420.0 - 640.0) * (t / 0.055).min(1.0);
            let envelope = (1.0 - t / 0.055).max(0.0).powi(2);
            0.08 * (std::f32::consts::TAU * frequency * t).sin() * envelope
        })
        .collect();
    Pcm::new(clamp_buffer(samples))
}

pub fn switch() -> Pcm {
    let samples = (0..SWITCH_SAMPLES)
        .map(|index| {
            let t = index as f32 / SAMPLE_RATE as f32;
            let envelope = (1.0 - t / 0.035).max(0.0).powi(2);
            0.05 * (std::f32::consts::TAU * 880.0 * t).sin() * envelope
        })
        .collect();
    Pcm::new(clamp_buffer(samples))
}

/// Clamp at the contract's headroom limit, including non-finite input safety.
pub fn clamp_buffer(mut samples: Vec<f32>) -> Vec<f32> {
    for sample in &mut samples {
        *sample = if sample.is_finite() {
            (*sample).clamp(-0.95, 0.95)
        } else {
            0.0
        };
    }
    samples
}

fn high_pass(value: f32, previous: &mut f32) -> f32 {
    // A one-pole high-pass preserves the deterministic source while removing
    // the low-frequency body used for ordinary block placement.
    let output = value - 0.35 * *previous;
    *previous = value;
    output
}

struct XorShift32 {
    state: u32,
}

impl XorShift32 {
    fn from_seed(seed: u64) -> Self {
        let folded = (seed as u32) ^ (seed >> 32) as u32;
        Self {
            state: if folded == 0 { 0x6d2b_79f5 } else { folded },
        }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    fn next_signed(&mut self) -> f32 {
        (self.next_u32() as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn procedural_audio_is_deterministic() {
        let seed = event_seed(0x1234_5678_9abc_def0, 17, 35);
        assert_eq!(
            generate(Effect::Footstep, seed),
            generate(Effect::Footstep, seed)
        );
        assert_ne!(
            generate(Effect::Footstep, seed),
            generate(Effect::Footstep, seed + 1)
        );
    }

    #[test]
    fn audio_mix_stays_below_full_scale() {
        for effect in [
            Effect::Footstep,
            Effect::Break,
            Effect::Place {
                pane_or_fence: false,
            },
            Effect::Place {
                pane_or_fence: true,
            },
            Effect::Water,
            Effect::Inventory,
            Effect::Switch,
        ] {
            assert!(generate(effect, 0xfeed_face).peak() <= 0.95);
        }
    }

    #[test]
    fn pcm_same_seed_is_bitwise_identical() {
        let a = generate(Effect::Break, event_seed(7, 11, 63));
        let b = generate(Effect::Break, event_seed(7, 11, 63));
        let a_bits: Vec<_> = a.samples.iter().map(|value| value.to_bits()).collect();
        let b_bits: Vec<_> = b.samples.iter().map(|value| value.to_bits()).collect();
        assert_eq!(a_bits, b_bits);
    }

    #[test]
    fn inventory_and_switch_durations_match_contract() {
        assert_eq!(inventory().samples.len(), 2_640);
        assert_eq!(switch().samples.len(), 1_680);
        assert!((inventory().duration_seconds() - 0.055).abs() < 1.0 / SAMPLE_RATE as f32);
        assert!((switch().duration_seconds() - 0.035).abs() < 1.0 / SAMPLE_RATE as f32);
    }
}
