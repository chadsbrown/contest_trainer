use rand::rngs::SmallRng;
use rand::Rng;
use rand::SeedableRng;

use crate::config::NoiseSettings;

/// 2nd-order biquad bandpass filter for realistic receiver noise shaping
struct BiquadFilter {
    // Coefficients (a0 normalized to 1.0)
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    // State (previous samples)
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    // Parameters for recalculation
    sample_rate: u32,
}

impl BiquadFilter {
    fn new(sample_rate: u32, center_freq: f32, bandwidth: f32) -> Self {
        let mut filter = Self {
            b0: 0.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            sample_rate,
        };
        filter.update_params(center_freq, bandwidth);
        filter
    }

    fn update_params(&mut self, center_freq: f32, bandwidth: f32) {
        let omega = 2.0 * std::f32::consts::PI * center_freq / self.sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();
        let q = center_freq / bandwidth;
        let alpha = sin_omega / (2.0 * q);

        // Bandpass filter coefficients (constant 0dB peak gain)
        let b0 = alpha;
        let b1 = 0.0;
        let b2 = -alpha;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_omega;
        let a2 = 1.0 - alpha;

        // Normalize by a0
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }

    fn process(&mut self, input: f32) -> f32 {
        let output = self.b0 * input + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;

        // Shift state
        self.x2 = self.x1;
        self.x1 = input;
        self.y2 = self.y1;
        self.y1 = output;

        output
    }
}

/// Generates band noise with realistic static, crashes, and pops
pub struct NoiseGenerator {
    rng: SmallRng,
    sample_rate: u32,

    // Bandpass filter for realistic receiver noise
    filter: BiquadFilter,

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
        // Default bandpass filter centered at 600 Hz with 400 Hz bandwidth
        let filter = BiquadFilter::new(sample_rate, 600.0, 400.0);

        Self {
            rng: SmallRng::from_entropy(),
            sample_rate,
            filter,
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

    /// Update filter parameters when tone frequency or bandwidth changes
    pub fn update_filter(&mut self, center_freq: f32, bandwidth: f32) {
        self.filter.update_params(center_freq, bandwidth);
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

        // Apply bandpass filter for realistic receiver noise
        let filtered = self.filter.process(white);

        let base_noise = filtered * level;

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

        // Apply bandpass filter for realistic receiver noise
        let filtered = self.filter.process(white);

        filtered * level
    }

    /// Fill a buffer with noise samples (additive)
    pub fn fill_buffer(&mut self, buffer: &mut [f32], level: f32, settings: &NoiseSettings) {
        for sample in buffer.iter_mut() {
            *sample += self.next_sample(level, settings);
        }
    }
}
