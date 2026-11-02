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

/// Output sample rate at which noise levels were tuned. White excitation is
/// scaled relative to this rate so noise power per Hz, and therefore the level
/// heard through the receiver filter, does not depend on the device rate.
const REFERENCE_SAMPLE_RATE: f32 = 44_100.0;

// Kellet pink-noise approximation, designed for the reference rate.
const PINK_POLES: [f32; 3] = [0.99765, 0.96300, 0.57000];
const PINK_GAINS: [f32; 3] = [0.0990460, 0.2965164, 1.0526913];

/// Simple pink noise generator (Kellet-style filtered white noise)
struct PinkNoise {
    poles: [f32; 3],
    gains: [f32; 3],
    state: [f32; 3],
}

impl PinkNoise {
    /// Adapts the reference-rate design to `sample_rate`, keeping each stage's
    /// corner frequency and DC gain, so the spectral tilt is rate-independent.
    fn new(sample_rate: u32) -> Self {
        let ratio = REFERENCE_SAMPLE_RATE / sample_rate as f32;
        let mut poles = PINK_POLES;
        let mut gains = PINK_GAINS;
        for (pole, gain) in poles.iter_mut().zip(gains.iter_mut()) {
            let adapted = pole.powf(ratio);
            *gain *= (1.0 - adapted) / (1.0 - *pole);
            *pole = adapted;
        }
        Self {
            poles,
            gains,
            state: [0.0; 3],
        }
    }

    fn next(&mut self, white: f32) -> f32 {
        // Lightweight approximation of 1/f noise.
        for i in 0..3 {
            self.state[i] = self.poles[i] * self.state[i] + white * self.gains[i];
        }
        // Scale to keep level reasonable.
        (self.state[0] + self.state[1] + self.state[2] + white * 0.1848) * 0.05
    }
}

/// Generates band noise with realistic static, crashes, and pops
pub struct NoiseGenerator {
    rng: SmallRng,
    sample_rate: u32,
    /// sqrt(sample_rate / REFERENCE_SAMPLE_RATE), applied to white excitation
    excitation_scale: f32,

    // Bandpass filter for realistic receiver noise
    filter: BiquadFilter,
    // Pink noise state for more realistic HF noise coloration
    pink: PinkNoise,

    // Static crash state
    crash_remaining_samples: u32,
    crash_amplitude: f32,
    crash_decay: f32,

    // Pop/click state
    pop_remaining_samples: u32,
    pop_amplitude: f32,
    pop_decay: f32,

    // Slow band-noise variation, independent of receiver AGC
    noise_floor: f32,
    noise_floor_alpha: f32,
    noise_floor_target: f32,
    noise_floor_target_remaining: u32,
}

impl NoiseGenerator {
    pub fn new(sample_rate: u32, center_freq: f32, bandwidth: f32) -> Self {
        Self::with_rng(sample_rate, center_freq, bandwidth, SmallRng::from_os_rng())
    }

    #[cfg(test)]
    pub fn with_seed(sample_rate: u32, center_freq: f32, bandwidth: f32, seed: u64) -> Self {
        Self::with_rng(
            sample_rate,
            center_freq,
            bandwidth,
            SmallRng::seed_from_u64(seed),
        )
    }

    fn with_rng(sample_rate: u32, center_freq: f32, bandwidth: f32, rng: SmallRng) -> Self {
        let filter = BiquadFilter::new(sample_rate, center_freq, bandwidth);

        // Low cutoff (~0.3 Hz) for slow noise-floor variation
        let cutoff_hz = 0.3_f32;
        let noise_floor_alpha =
            1.0 - (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32).exp();

        Self {
            rng,
            sample_rate,
            excitation_scale: (sample_rate as f32 / REFERENCE_SAMPLE_RATE).sqrt(),
            filter,
            pink: PinkNoise::new(sample_rate),
            crash_remaining_samples: 0,
            crash_amplitude: 0.0,
            crash_decay: 0.0,
            pop_remaining_samples: 0,
            pop_amplitude: 0.0,
            pop_decay: 0.0,
            noise_floor: 0.0,
            noise_floor_alpha,
            noise_floor_target: 0.0,
            noise_floor_target_remaining: 0,
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
            if self.rng.random::<f32>() < prob_per_sample {
                // Start a crash - duration 50-200ms
                let duration_ms = self.rng.random_range(50.0..200.0);
                self.crash_remaining_samples =
                    (duration_ms * self.sample_rate as f32 / 1000.0) as u32;
                self.crash_amplitude = crash_intensity * self.rng.random_range(0.5..1.0);
                // Decay rate so amplitude reaches ~10% at end
                self.crash_decay = (0.1_f32).powf(1.0 / self.crash_remaining_samples as f32);
            }
        }
    }

    /// Check if we should start a new pop/click
    fn maybe_start_pop(&mut self, pop_rate: f32, pop_intensity: f32) {
        if self.pop_remaining_samples == 0 && pop_rate > 0.0 {
            let prob_per_sample = pop_rate / self.sample_rate as f32;
            if self.rng.random::<f32>() < prob_per_sample {
                // A short exponential excitation, timed in seconds so its strength
                // and shape remain comparable across output sample rates.
                let time_constant = self.rng.random_range(0.0002..0.0005);
                self.pop_decay = (-1.0 / (time_constant * self.sample_rate as f32)).exp();
                self.pop_remaining_samples =
                    (time_constant * 8.0 * self.sample_rate as f32).ceil() as u32;
                self.pop_amplitude = pop_intensity * self.rng.random_range(0.6..1.0);
                // Random polarity
                if self.rng.random::<bool>() {
                    self.pop_amplitude = -self.pop_amplitude;
                }
            }
        }
    }

    /// Generate crash sample (filtered noise burst)
    fn crash_sample(&mut self) -> f32 {
        if self.crash_remaining_samples > 0 {
            self.crash_remaining_samples -= 1;
            let noise: f32 = self.rng.random_range(-1.0..1.0);
            let sample = noise * self.crash_amplitude * self.excitation_scale;
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
            let sample = self.pop_amplitude;
            self.pop_amplitude *= self.pop_decay;
            sample
        } else {
            0.0
        }
    }

    /// Generate a broadband QRN excitation; the receiver filter must process it.
    fn qrn_sample(&mut self, qrn_intensity: f32) -> f32 {
        if qrn_intensity <= 0.0 {
            return 0.0;
        }

        qrn_intensity * self.rng.random_range(-0.3..0.3) * self.excitation_scale
    }

    fn noise_floor_gain(&mut self, qrn_intensity: f32) -> f32 {
        // Holding each target lets the slow smoother follow it instead of
        // averaging independent per-sample targets almost completely to zero.
        if self.noise_floor_target_remaining == 0 {
            self.noise_floor_target = self.rng.random_range(-1.0..1.0);
            let hold_seconds = self.rng.random_range(0.2..2.0);
            self.noise_floor_target_remaining =
                (hold_seconds * self.sample_rate as f32).ceil() as u32;
        }
        self.noise_floor_target_remaining -= 1;
        self.noise_floor += self.noise_floor_alpha * (self.noise_floor_target - self.noise_floor);
        // Up to +/-6 dB of slowly varying background level at maximum QRN.
        10.0_f32.powf(self.noise_floor * qrn_intensity * 6.0 / 20.0)
    }

    /// Generate a single noise sample with all effects
    pub fn next_sample(&mut self, level: f32, settings: &NoiseSettings) -> f32 {
        // Check for new events
        self.maybe_start_crash(settings.crash_rate, settings.crash_intensity);
        self.maybe_start_pop(settings.pop_rate, settings.pop_intensity);

        // Generate white noise base and color it to pink (closer to HF band noise)
        let white: f32 = self.rng.random_range(-1.0..1.0) * self.excitation_scale;
        let pink = self.pink.next(white);

        let floor_gain = self.noise_floor_gain(settings.qrn_intensity);
        let qrn = self.qrn_sample(settings.qrn_intensity);

        // Every noise source passes through the receiver bandpass. Slow level
        // drift modulates the background, rather than adding sub-audio tones.
        let crash = self.crash_sample();
        let pop = self.pop_sample();
        self.filter.process((pink + qrn) * floor_gain + crash + pop) * level
    }

    /// Fill a buffer with noise samples (additive)
    pub fn fill_buffer(&mut self, buffer: &mut [f32], level: f32, settings: &NoiseSettings) {
        for sample in buffer.iter_mut() {
            *sample += self.next_sample(level, settings);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(qrn_intensity: f32) -> NoiseSettings {
        NoiseSettings {
            crash_rate: 0.0,
            crash_intensity: 0.0,
            pop_rate: 0.0,
            pop_intensity: 0.0,
            qrn_intensity,
        }
    }

    fn render(sample_rate: u32, seconds: usize, settings: &NoiseSettings) -> Vec<f32> {
        let mut generator = NoiseGenerator::with_seed(sample_rate, 600.0, 350.0, 42);
        for _ in 0..sample_rate {
            generator.next_sample(0.25, settings);
        }
        let mut samples = vec![0.0; sample_rate as usize * seconds];
        generator.fill_buffer(&mut samples, 0.25, settings);
        samples
    }

    // Average equally windowed frequency measurements over many frames so an
    // individual random spectral null cannot decide the regression result.
    fn spectral_power(samples: &[f32], sample_rate: u32, frequency: f64) -> f64 {
        let n = 4096;
        let weights: Vec<_> = (0..n)
            .map(|i| {
                let window = 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos();
                let phase = std::f64::consts::TAU * frequency * i as f64 / sample_rate as f64;
                (window * phase.cos(), window * phase.sin())
            })
            .collect();
        let mut total = 0.0;
        let mut frames = 0;
        for frame in samples.chunks_exact(n) {
            let (mut re, mut im) = (0.0, 0.0);
            for (&sample, &(cos, sin)) in frame.iter().zip(&weights) {
                re += sample as f64 * cos;
                im += sample as f64 * sin;
            }
            total += re * re + im * im;
            frames += 1;
        }
        assert!(frames > 0);
        total / frames as f64
    }

    fn db(power_ratio: f64) -> f64 {
        10.0 * power_ratio.log10()
    }

    fn rms(samples: &[f32]) -> f64 {
        (samples.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / samples.len() as f64).sqrt()
    }

    fn heavy_effects() -> NoiseSettings {
        NoiseSettings {
            crash_rate: 5.0,
            crash_intensity: 1.0,
            pop_rate: 10.0,
            pop_intensity: 1.0,
            qrn_intensity: 1.0,
        }
    }

    #[test]
    fn all_noise_effects_are_band_limited_at_supported_sample_rates() {
        for sample_rate in [44100, 48000] {
            let samples = render(sample_rate, 10, &heavy_effects());
            assert!(samples.iter().all(|sample| sample.is_finite()));
            let center = spectral_power(&samples, sample_rate, 600.0);
            let high = db(spectral_power(&samples, sample_rate, 5000.0) / center);
            assert!(
                high < -15.0,
                "wideband hiss at {sample_rate} Hz: {high:.1} dB"
            );

            // The 2nd-order filter's own skirt puts 20 Hz only ~20 dB below
            // center, so compare against plain background noise through the
            // same filter: effects must not add low-frequency energy beyond it.
            let reference = render(sample_rate, 10, &settings(0.0));
            let reference_low = db(spectral_power(&reference, sample_rate, 20.0)
                / spectral_power(&reference, sample_rate, 600.0));
            let low = db(spectral_power(&samples, sample_rate, 20.0) / center);
            assert!(
                low < reference_low + 3.0,
                "sub-audio leakage at {sample_rate} Hz: {low:.1} dB vs background {reference_low:.1} dB"
            );

            let mean = samples.iter().map(|&x| x as f64).sum::<f64>() / samples.len() as f64;
            assert!(mean.abs() < 0.0001, "DC offset {mean} at {sample_rate} Hz");
        }
    }

    #[test]
    fn background_level_is_independent_of_sample_rate() {
        let reference_rms = rms(&render(44100, 20, &settings(0.0)));
        for sample_rate in [48000, 96000] {
            let level = db((rms(&render(sample_rate, 20, &settings(0.0))) / reference_rms).powi(2));
            println!("background at {sample_rate} Hz: {level:+.3} dB");
            assert!(
                level.abs() < 0.2,
                "level at {sample_rate} Hz: {level:+.2} dB"
            );
        }
    }

    #[test]
    fn pink_noise_response_is_independent_of_sample_rate() {
        // Exact magnitude response of the pink stage at an audio frequency.
        // With excitation normalized to constant power per Hz, matching
        // responses means matching noise spectra.
        fn response(pink: &PinkNoise, sample_rate: u32, frequency: f64) -> f64 {
            let omega = std::f64::consts::TAU * frequency / sample_rate as f64;
            let (mut re, mut im) = (0.1848, 0.0);
            for (&pole, &gain) in pink.poles.iter().zip(&pink.gains) {
                // gain / (1 - pole * e^{-j omega})
                let (dr, di) = (1.0 - pole as f64 * omega.cos(), pole as f64 * omega.sin());
                let scale = gain as f64 / (dr * dr + di * di);
                re += scale * dr;
                im -= scale * di;
            }
            0.05 * (re * re + im * im).sqrt()
        }
        let reference = PinkNoise::new(44100);
        for sample_rate in [48000, 96000] {
            let pink = PinkNoise::new(sample_rate);
            // Receiver passbands lie below ~1.5 kHz. Matching is approximate
            // for the fastest stage well above that (about -0.1 dB at 3 kHz).
            for frequency in [100.0, 300.0, 600.0, 1000.0, 1500.0] {
                let difference = db((response(&pink, sample_rate, frequency)
                    / response(&reference, 44100, frequency))
                .powi(2));
                assert!(
                    difference.abs() < 0.1,
                    "pink response at {frequency} Hz, {sample_rate} Hz rate: {difference:+.3} dB"
                );
            }
        }
    }

    #[test]
    fn crash_level_is_independent_of_sample_rate() {
        // Back-to-back crashes so the render averages over many events; crash
        // bursts are white excitation and must be scaled like the background.
        let crashes = NoiseSettings {
            crash_rate: 100.0,
            crash_intensity: 1.0,
            ..settings(0.0)
        };
        let reference_rms = rms(&render(44100, 20, &crashes));
        for sample_rate in [48000, 96000] {
            let level = db((rms(&render(sample_rate, 20, &crashes)) / reference_rms).powi(2));
            println!("crashes at {sample_rate} Hz: {level:+.3} dB");
            assert!(
                level.abs() < 0.75,
                "crash level at {sample_rate} Hz: {level:+.2} dB"
            );
        }
    }

    #[test]
    fn qrn_produces_slow_background_level_variation() {
        fn variation(samples: &[f32], sample_rate: u32) -> f64 {
            let levels: Vec<_> = samples
                .chunks_exact(sample_rate as usize)
                .map(|window| {
                    (window.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / window.len() as f64)
                        .sqrt()
                })
                .collect();
            let mean = levels.iter().sum::<f64>() / levels.len() as f64;
            (levels
                .iter()
                .map(|level| (level - mean).powi(2))
                .sum::<f64>()
                / levels.len() as f64)
                .sqrt()
                / mean
        }
        for sample_rate in [44100, 48000] {
            let steady = variation(&render(sample_rate, 30, &settings(0.0)), sample_rate);
            let drifting = variation(&render(sample_rate, 30, &settings(1.0)), sample_rate);
            assert!(drifting > 0.1 && drifting > steady * 3.0);
        }
    }

    #[test]
    fn seeded_noise_is_independent_of_buffer_boundaries() {
        let settings = NoiseSettings::default();
        let mut whole = NoiseGenerator::with_seed(48000, 800.0, 250.0, 17);
        let mut chunked = NoiseGenerator::with_seed(48000, 800.0, 250.0, 17);
        let mut expected = vec![0.0; 48000];
        let mut actual = expected.clone();
        whole.fill_buffer(&mut expected, 0.25, &settings);
        for chunk in actual.chunks_mut(137) {
            chunked.fill_buffer(chunk, 0.25, &settings);
        }
        assert_eq!(actual, expected);
    }

    #[test]
    fn pops_decay_immediately_with_comparable_energy_across_sample_rates() {
        let mut energies = Vec::new();
        for sample_rate in [44100, 48000] {
            let mut generator = NoiseGenerator::with_seed(sample_rate, 600.0, 350.0, 9);
            // Force an event through the actual event setup rather than manually
            // supplying a decay coefficient in the test.
            generator.maybe_start_pop(sample_rate as f32, 1.0);
            let initial = generator.pop_amplitude.abs();
            let first = generator.pop_sample();
            let second = generator.pop_sample();
            assert_eq!(first.abs(), initial);
            assert!(second.abs() < first.abs());
            let mut energy = 0.0_f64;
            for sample in [first, second] {
                energy += (generator.filter.process(sample) as f64).powi(2);
            }
            for _ in 0..sample_rate / 50 {
                let sample = generator.pop_sample();
                let filtered = generator.filter.process(sample);
                energy += (filtered as f64).powi(2);
            }
            assert_eq!(generator.pop_sample(), 0.0);
            energies.push(energy / sample_rate as f64);
        }
        let ratio = energies[0] / energies[1];
        assert!((0.9..1.1).contains(&ratio), "pop energy ratio: {ratio}");
    }

    #[test]
    fn noise_filter_uses_configured_passband_at_startup() {
        for sample_rate in [44100, 48000] {
            let settings = crate::config::AudioSettings {
                sample_rate,
                tone_frequency_hz: 1000.0,
                noise_bandwidth: 100.0,
                ..crate::config::AudioSettings::default()
            };
            let mut mixer = crate::audio::mixer::Mixer::new(sample_rate, settings);
            mixer.noise.rng = SmallRng::seed_from_u64(42);
            let quiet = crate::config::NoiseSettings {
                crash_rate: 0.0,
                crash_intensity: 0.0,
                pop_rate: 0.0,
                pop_intensity: 0.0,
                qrn_intensity: 0.0,
            };
            let mut samples = vec![0.0; sample_rate as usize * 4];
            mixer.noise.fill_buffer(&mut samples, 0.25, &quiet);
            let center = spectral_power(&samples, sample_rate, 1000.0);
            let old_center = spectral_power(&samples, sample_rate, 600.0);
            assert!(center > old_center * 10.0);
        }
    }

    #[test]
    #[ignore = "diagnostic report; run explicitly with --ignored --nocapture"]
    fn report_noise_spectrum() {
        let cases = [
            ("background only", settings(0.0)),
            ("background + QRN", settings(0.3)),
            ("defaults", NoiseSettings::default()),
            (
                "heavy effects",
                NoiseSettings {
                    crash_rate: 5.0,
                    crash_intensity: 1.0,
                    pop_rate: 10.0,
                    pop_intensity: 1.0,
                    qrn_intensity: 1.0,
                },
            ),
        ];
        for sample_rate in [44100, 48000] {
            println!("\n{sample_rate} Hz; seed=42; center=600 Hz; bandwidth=350 Hz; level=0.25");
            println!("20 seconds after 1 second warm-up; generator output before mixer processing");
            for (name, settings) in &cases {
                let samples = render(sample_rate, 20, settings);
                let rms = (samples.iter().map(|&x| (x as f64).powi(2)).sum::<f64>()
                    / samples.len() as f64)
                    .sqrt();
                let mean = samples.iter().map(|&x| x as f64).sum::<f64>() / samples.len() as f64;
                println!("{name}: total RMS={rms:.6}; mean={mean:.8}");
                let center = spectral_power(&samples, sample_rate, 600.0);
                for frequency in [20.0, 100.0, 300.0, 600.0, 1000.0, 5000.0, 12000.0, 18000.0] {
                    let power = spectral_power(&samples, sample_rate, frequency);
                    let relative_db = 10.0 * (power / center).log10();
                    println!("  {frequency:7.0} Hz: {relative_db:7.2} dB relative to 600 Hz");
                }
            }
        }
    }
}
