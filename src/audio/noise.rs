use rand::Rng;
use rand::rngs::SmallRng;
use rand::SeedableRng;

/// Generates band noise for background audio
pub struct NoiseGenerator {
    rng: SmallRng,
    filter_state: f32,
    filter_coefficient: f32,
}

impl NoiseGenerator {
    pub fn new(sample_rate: u32) -> Self {
        // Low-pass filter cutoff around 2.5kHz for realistic CW band noise
        let cutoff_hz = 2500.0;
        let filter_coefficient = (-2.0 * std::f32::consts::PI * cutoff_hz / sample_rate as f32).exp();

        Self {
            rng: SmallRng::from_entropy(),
            filter_state: 0.0,
            filter_coefficient,
        }
    }

    /// Generate a single noise sample
    pub fn next_sample(&mut self, level: f32) -> f32 {
        // Generate white noise
        let white: f32 = self.rng.gen_range(-1.0..1.0);

        // Simple low-pass filter for more realistic band noise
        self.filter_state = self.filter_coefficient * self.filter_state
            + (1.0 - self.filter_coefficient) * white;

        self.filter_state * level
    }

    /// Fill a buffer with noise samples (additive)
    pub fn fill_buffer(&mut self, buffer: &mut [f32], level: f32) {
        for sample in buffer.iter_mut() {
            *sample += self.next_sample(level);
        }
    }
}
