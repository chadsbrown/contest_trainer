use rand::rngs::SmallRng;
use rand::Rng;
use rand::SeedableRng;

use crate::config::NoiseSettings;

/// Generates band noise with realistic static, crashes, and pops
pub struct NoiseGenerator {
    rng: SmallRng,
    sample_rate: u32,

    // Base noise filter
    filter_state: f32,
    filter_coefficient: f32,

    // Static crash state
    crash_remaining_samples: u32,
    crash_amplitude: f32,
    crash_decay: f32,

    // Pop/click state
    pop_remaining_samples: u32,
    pop_amplitude: f32,

    // QRN (atmospheric) state - slow rumbling variations
    qrn_phase: f32,
    qrn_frequency: f32,
    qrn_mod_phase: f32,
}

impl NoiseGenerator {
    pub fn new(sample_rate: u32) -> Self {
        // Low-pass filter cutoff around 2.5kHz for realistic CW band noise
        let cutoff_hz = 2500.0;
        let filter_coefficient =
            (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32).exp();

        Self {
            rng: SmallRng::from_entropy(),
            sample_rate,
            filter_state: 0.0,
            filter_coefficient,
            crash_remaining_samples: 0,
            crash_amplitude: 0.0,
            crash_decay: 0.0,
            pop_remaining_samples: 0,
            pop_amplitude: 0.0,
            qrn_phase: 0.0,
            qrn_frequency: 0.3, // Very slow oscillation
            qrn_mod_phase: 0.0,
        }
    }

    /// Check if we should start a new crash
    fn maybe_start_crash(&mut self, crash_rate: f32, crash_intensity: f32) {
        if self.crash_remaining_samples == 0 && crash_rate > 0.0 {
            // Probability per sample
            let prob_per_sample = crash_rate / self.sample_rate as f32;
            if self.rng.gen::<f32>() < prob_per_sample {
                // Start a crash - duration 50-200ms
                let duration_ms = self.rng.gen_range(50.0..200.0);
                self.crash_remaining_samples =
                    (duration_ms * self.sample_rate as f32 / 1000.0) as u32;
                self.crash_amplitude = crash_intensity * self.rng.gen_range(0.5..1.0);
                // Decay rate so amplitude reaches ~10% at end
                self.crash_decay = (0.1_f32).powf(1.0 / self.crash_remaining_samples as f32);
            }
        }
    }

    /// Check if we should start a new pop/click
    fn maybe_start_pop(&mut self, pop_rate: f32, pop_intensity: f32) {
        if self.pop_remaining_samples == 0 && pop_rate > 0.0 {
            let prob_per_sample = pop_rate / self.sample_rate as f32;
            if self.rng.gen::<f32>() < prob_per_sample {
                // Start a pop - very short, 1-5ms
                let duration_ms = self.rng.gen_range(1.0..5.0);
                self.pop_remaining_samples =
                    (duration_ms * self.sample_rate as f32 / 1000.0) as u32;
                self.pop_amplitude = pop_intensity * self.rng.gen_range(0.6..1.0);
                // Random polarity
                if self.rng.gen::<bool>() {
                    self.pop_amplitude = -self.pop_amplitude;
                }
            }
        }
    }

    /// Generate crash sample (filtered noise burst)
    fn crash_sample(&mut self) -> f32 {
        if self.crash_remaining_samples > 0 {
            self.crash_remaining_samples -= 1;
            let noise: f32 = self.rng.gen_range(-1.0..1.0);
            let sample = noise * self.crash_amplitude;
            self.crash_amplitude *= self.crash_decay;
            sample
        } else {
            0.0
        }
    }

    /// Generate pop/click sample
    fn pop_sample(&mut self) -> f32 {
        if self.pop_remaining_samples > 0 {
            self.pop_remaining_samples -= 1;
            // Sharp attack, quick decay
            let progress = 1.0 - (self.pop_remaining_samples as f32 / 5.0).min(1.0);
            self.pop_amplitude * (1.0 - progress)
        } else {
            0.0
        }
    }

    /// Generate QRN (atmospheric rumble) sample
    fn qrn_sample(&mut self, qrn_intensity: f32) -> f32 {
        if qrn_intensity <= 0.0 {
            return 0.0;
        }

        // Slow-varying rumble using multiple low-frequency oscillators
        let base_freq = self.qrn_frequency / self.sample_rate as f32;
        self.qrn_phase += base_freq * 2.0 * std::f32::consts::PI;
        if self.qrn_phase > 2.0 * std::f32::consts::PI {
            self.qrn_phase -= 2.0 * std::f32::consts::PI;
        }

        // Modulation frequency for variation
        self.qrn_mod_phase += (base_freq * 0.1) * 2.0 * std::f32::consts::PI;
        if self.qrn_mod_phase > 2.0 * std::f32::consts::PI {
            self.qrn_mod_phase -= 2.0 * std::f32::consts::PI;
        }

        // Combine oscillators with noise for organic feel
        let osc1 = self.qrn_phase.sin();
        let osc2 = (self.qrn_phase * 1.7).sin();
        let mod_depth = 0.5 + 0.5 * self.qrn_mod_phase.sin();

        let noise: f32 = self.rng.gen_range(-0.3..0.3);

        qrn_intensity * mod_depth * (osc1 * 0.6 + osc2 * 0.3 + noise)
    }

    /// Generate a single noise sample with all effects
    pub fn next_sample(&mut self, level: f32, settings: &NoiseSettings) -> f32 {
        // Check for new events
        self.maybe_start_crash(settings.crash_rate, settings.crash_intensity);
        self.maybe_start_pop(settings.pop_rate, settings.pop_intensity);

        // Generate white noise base
        let white: f32 = self.rng.gen_range(-1.0..1.0);

        // Simple low-pass filter for more realistic band noise
        self.filter_state =
            self.filter_coefficient * self.filter_state + (1.0 - self.filter_coefficient) * white;

        let base_noise = self.filter_state * level;

        // Add effects
        let crash = self.crash_sample() * level;
        let pop = self.pop_sample() * level;
        let qrn = self.qrn_sample(settings.qrn_intensity) * level;

        base_noise + crash + pop + qrn
    }

    /// Generate a single noise sample (legacy interface without settings)
    pub fn next_sample_simple(&mut self, level: f32) -> f32 {
        // Generate white noise
        let white: f32 = self.rng.gen_range(-1.0..1.0);

        // Simple low-pass filter for more realistic band noise
        self.filter_state =
            self.filter_coefficient * self.filter_state + (1.0 - self.filter_coefficient) * white;

        self.filter_state * level
    }

    /// Fill a buffer with noise samples (additive)
    pub fn fill_buffer(&mut self, buffer: &mut [f32], level: f32, settings: &NoiseSettings) {
        for sample in buffer.iter_mut() {
            *sample += self.next_sample(level, settings);
        }
    }
}
