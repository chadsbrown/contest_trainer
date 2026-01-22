use super::morse::{text_to_morse, MorseElement, MorseTimer, ToneGenerator};
use super::noise::NoiseGenerator;
use crate::config::{AudioSettings, QsbSettings};
use crate::messages::{StationId, StationParams};
use rand::Rng;

/// QSB (fading) oscillator that produces natural-sounding signal fading
/// Uses multiple layered sine waves with different periods for a non-repetitive pattern
pub struct QsbOscillator {
    /// Phase accumulators for each oscillator layer (in radians)
    phases: [f32; 3],
    /// Angular velocities for each layer (radians per sample)
    velocities: [f32; 3],
    /// Depth of fading (0.0 = none, 1.0 = full fade to silence)
    depth: f32,
    /// Whether QSB is enabled
    enabled: bool,
}

impl QsbOscillator {
    pub fn new(sample_rate: u32, settings: &QsbSettings) -> Self {
        let mut rng = rand::thread_rng();

        // Convert cycles per minute to radians per sample
        // base_rate is cycles/minute, we need radians/sample
        // radians/sample = (cycles/minute) * (2π radians/cycle) * (1 minute/60 seconds) * (1 second/sample_rate samples)
        let base_angular_velocity =
            settings.rate * 2.0 * std::f32::consts::PI / 60.0 / sample_rate as f32;

        // Create three oscillators with different rates for natural variation
        // Rates are roughly 1x, 0.7x, and 1.3x the base rate, with some randomness
        let velocities = [
            base_angular_velocity * (0.9 + rng.gen::<f32>() * 0.2),
            base_angular_velocity * (0.6 + rng.gen::<f32>() * 0.2),
            base_angular_velocity * (1.2 + rng.gen::<f32>() * 0.2),
        ];

        // Random starting phases so each station sounds different
        let phases = [
            rng.gen::<f32>() * 2.0 * std::f32::consts::PI,
            rng.gen::<f32>() * 2.0 * std::f32::consts::PI,
            rng.gen::<f32>() * 2.0 * std::f32::consts::PI,
        ];

        Self {
            phases,
            velocities,
            depth: settings.depth,
            enabled: settings.enabled,
        }
    }

    /// Get the current QSB amplitude factor (0.0 to 1.0) and advance the oscillator
    pub fn next_factor(&mut self) -> f32 {
        if !self.enabled {
            return 1.0;
        }

        // Combine three sine waves with different weights
        // This creates a complex, non-repeating pattern
        let combined =
            0.5 * self.phases[0].sin() + 0.3 * self.phases[1].sin() + 0.2 * self.phases[2].sin();

        // combined ranges from -1.0 to 1.0, normalize to 0.0 to 1.0
        let normalized = (combined + 1.0) / 2.0;

        // Apply depth: at depth=0, always return 1.0; at depth=1, return full range
        let factor = 1.0 - self.depth + self.depth * normalized;

        // Advance all phases
        for i in 0..3 {
            self.phases[i] += self.velocities[i];
            // Keep phases in reasonable range to avoid floating point issues
            if self.phases[i] > 2.0 * std::f32::consts::PI {
                self.phases[i] -= 2.0 * std::f32::consts::PI;
            }
        }

        factor
    }

    /// Update settings (called when user changes QSB settings)
    pub fn update_settings(&mut self, settings: &QsbSettings) {
        self.depth = settings.depth;
        self.enabled = settings.enabled;
        // Note: we don't update velocities to avoid jarring changes mid-fade
    }
}

/// State for an active station being rendered
pub struct ActiveStation {
    pub id: StationId,
    pub callsign: String,
    pub elements: Vec<MorseElement>,
    pub current_element_idx: usize,
    pub samples_in_element: usize,
    pub samples_elapsed: usize,
    pub tone_generator: ToneGenerator,
    pub timer: MorseTimer,
    pub amplitude: f32,
    pub completed: bool,
    pub qsb: QsbOscillator,
}

impl ActiveStation {
    pub fn new(
        params: &StationParams,
        message: &str,
        sample_rate: u32,
        center_freq: f32,
        qsb_settings: &QsbSettings,
    ) -> Self {
        let elements = text_to_morse(message);
        let timer = MorseTimer::new(sample_rate, params.wpm);
        let mut tone_generator =
            ToneGenerator::new(center_freq + params.frequency_offset_hz, sample_rate);
        tone_generator.reset_phase();

        let samples_in_element = if elements.is_empty() {
            0
        } else {
            timer.element_samples(elements[0])
        };

        Self {
            id: params.id,
            callsign: params.callsign.clone(),
            elements,
            current_element_idx: 0,
            samples_in_element,
            samples_elapsed: 0,
            tone_generator,
            timer,
            amplitude: params.amplitude,
            completed: false,
            qsb: QsbOscillator::new(sample_rate, qsb_settings),
        }
    }

    /// Generate the next sample for this station
    /// Returns None if the station is done sending
    pub fn next_sample(&mut self) -> Option<f32> {
        if self.completed || self.current_element_idx >= self.elements.len() {
            self.completed = true;
            return None;
        }

        let element = self.elements[self.current_element_idx];

        // Get QSB factor (always advances the oscillator to keep fading continuous)
        let qsb_factor = self.qsb.next_factor();

        let sample = if element.is_tone() {
            // Generate tone with envelope and QSB
            let raw = self.tone_generator.next_sample();
            let envelope = self
                .tone_generator
                .envelope(self.samples_elapsed, self.samples_in_element);
            raw * envelope * self.amplitude * qsb_factor
        } else {
            // Silence for gaps - but still advance the tone generator phase
            // to maintain phase continuity
            0.0
        };

        self.samples_elapsed += 1;

        // Check if we need to move to next element
        if self.samples_elapsed >= self.samples_in_element {
            self.current_element_idx += 1;
            self.samples_elapsed = 0;

            if self.current_element_idx < self.elements.len() {
                self.samples_in_element = self
                    .timer
                    .element_samples(self.elements[self.current_element_idx]);
            }
        }

        Some(sample)
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}

/// User station for playing CQ, exchanges, etc.
pub struct UserStation {
    pub elements: Vec<MorseElement>,
    pub current_element_idx: usize,
    pub samples_in_element: usize,
    pub samples_elapsed: usize,
    pub tone_generator: ToneGenerator,
    pub timer: MorseTimer,
    pub completed: bool,
}

impl UserStation {
    pub fn new(message: &str, wpm: u8, sample_rate: u32, frequency_hz: f32) -> Self {
        let elements = text_to_morse(message);
        let timer = MorseTimer::new(sample_rate, wpm);
        let mut tone_generator = ToneGenerator::new(frequency_hz, sample_rate);
        tone_generator.reset_phase();

        let samples_in_element = if elements.is_empty() {
            0
        } else {
            timer.element_samples(elements[0])
        };

        Self {
            elements,
            current_element_idx: 0,
            samples_in_element,
            samples_elapsed: 0,
            tone_generator,
            timer,
            completed: false,
        }
    }

    pub fn next_sample(&mut self) -> Option<f32> {
        if self.completed || self.current_element_idx >= self.elements.len() {
            self.completed = true;
            return None;
        }

        let element = self.elements[self.current_element_idx];

        let sample = if element.is_tone() {
            let raw = self.tone_generator.next_sample();
            let envelope = self
                .tone_generator
                .envelope(self.samples_elapsed, self.samples_in_element);
            raw * envelope * 0.8 // User's own signal at consistent level
        } else {
            0.0
        };

        self.samples_elapsed += 1;

        if self.samples_elapsed >= self.samples_in_element {
            self.current_element_idx += 1;
            self.samples_elapsed = 0;

            if self.current_element_idx < self.elements.len() {
                self.samples_in_element = self
                    .timer
                    .element_samples(self.elements[self.current_element_idx]);
            }
        }

        Some(sample)
    }

    pub fn is_completed(&self) -> bool {
        self.completed
    }
}

/// Mixes multiple audio sources together
pub struct Mixer {
    pub stations: Vec<ActiveStation>,
    pub user_station: Option<UserStation>,
    pub noise: NoiseGenerator,
    pub settings: AudioSettings,
}

impl Mixer {
    pub fn new(sample_rate: u32, settings: AudioSettings) -> Self {
        Self {
            stations: Vec::new(),
            user_station: None,
            noise: NoiseGenerator::new(sample_rate),
            settings,
        }
    }

    /// Add a new calling station
    pub fn add_station(&mut self, params: &StationParams, message: &str) {
        let station = ActiveStation::new(
            params,
            message,
            self.settings.sample_rate,
            self.settings.tone_frequency_hz,
            &self.settings.qsb,
        );
        self.stations.push(station);
    }

    /// Remove a station by ID
    pub fn remove_station(&mut self, id: StationId) {
        self.stations.retain(|s| s.id != id);
    }

    /// Start playing a user message
    pub fn play_user_message(&mut self, message: &str, wpm: u8) {
        self.user_station = Some(UserStation::new(
            message,
            wpm,
            self.settings.sample_rate,
            self.settings.tone_frequency_hz,
        ));
    }

    /// Update audio settings
    pub fn update_settings(&mut self, settings: AudioSettings) {
        // Update QSB settings on all active stations
        for station in &mut self.stations {
            station.qsb.update_settings(&settings.qsb);
        }
        // Update noise filter to match tone frequency and bandwidth
        self.noise
            .update_filter(settings.tone_frequency_hz, settings.noise_bandwidth);
        self.settings = settings;
    }

    /// Clear all stations
    pub fn clear_all(&mut self) {
        self.stations.clear();
        self.user_station = None;
    }

    /// Fill a buffer with mixed audio, returns list of completed station IDs
    pub fn fill_buffer(&mut self, buffer: &mut [f32]) -> (Vec<StationId>, bool) {
        let mut completed_stations = Vec::new();
        let mut user_completed = false;

        // Clear buffer
        for sample in buffer.iter_mut() {
            *sample = 0.0;
        }

        // Add noise (optionally muted while user is transmitting)
        let mute_noise = self.settings.mute_noise_during_tx && self.user_station.is_some();
        if !mute_noise {
            self.noise
                .fill_buffer(buffer, self.settings.noise_level, &self.settings.noise);
        }

        // Mix each calling station
        for station in &mut self.stations {
            for sample in buffer.iter_mut() {
                if let Some(station_sample) = station.next_sample() {
                    *sample += station_sample;
                } else {
                    break;
                }
            }
            if station.is_completed() {
                completed_stations.push(station.id);
            }
        }

        // Remove completed stations
        self.stations.retain(|s| !s.is_completed());

        // Mix user station if active
        if let Some(ref mut user) = self.user_station {
            for sample in buffer.iter_mut() {
                if let Some(user_sample) = user.next_sample() {
                    *sample += user_sample;
                } else {
                    break;
                }
            }
            if user.is_completed() {
                user_completed = true;
                self.user_station = None;
            }
        }

        // Apply master volume, dither, and soft clipping
        let mut rng = rand::thread_rng();
        for sample in buffer.iter_mut() {
            *sample *= self.settings.master_volume;
            // Add very small triangular dither to prevent audio artifacts
            let dither = (rng.gen::<f32>() - 0.5) * 0.001;
            *sample += dither;
            // Soft clipping using tanh
            if sample.abs() > 0.8 {
                *sample = sample.signum() * (0.8 + 0.2 * ((*sample).abs() - 0.8).tanh());
            }
        }

        (completed_stations, user_completed)
    }

    /// Check if user station is currently playing
    pub fn is_user_playing(&self) -> bool {
        self.user_station.is_some()
    }

    /// Get count of active calling stations
    pub fn active_station_count(&self) -> usize {
        self.stations.len()
    }
}
