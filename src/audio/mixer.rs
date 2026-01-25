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
    pub elements: Vec<MorseElement>,
    pub current_element_idx: usize,
    pub samples_in_element: usize,
    pub samples_elapsed: usize,
    pub tone_generator: ToneGenerator,
    pub timer: MorseTimer,
    pub amplitude: f32,
    pub completed: bool,
    pub qsb: QsbOscillator,
    /// Radio index for stereo routing: 0 = left channel, 1 = right channel
    pub radio_index: u8,
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
            elements,
            current_element_idx: 0,
            samples_in_element,
            samples_elapsed: 0,
            tone_generator,
            timer,
            amplitude: params.amplitude,
            completed: false,
            qsb: QsbOscillator::new(sample_rate, qsb_settings),
            radio_index: params.radio_index,
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
    /// Original message for TX indicator display
    pub message: String,
    /// Maps element index to character index in message (for TX indicator)
    pub element_to_char: Vec<usize>,
}

impl UserStation {
    pub fn new(message: &str, wpm: u8, sample_rate: u32, frequency_hz: f32) -> Self {
        let elements = text_to_morse(message);
        let element_to_char = Self::build_element_to_char_map(message);
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
            message: message.to_string(),
            element_to_char,
        }
    }

    /// Build a mapping from element index to character index
    fn build_element_to_char_map(message: &str) -> Vec<usize> {
        let mut map = Vec::new();
        let mut char_idx = 0;

        for ch in message.chars() {
            if ch.is_whitespace() {
                // Word gap maps to space character
                map.push(char_idx);
                char_idx += 1;
            } else if let Some(code) = super::morse::char_to_morse(ch) {
                // Each element of the character maps to this char index
                for (elem_idx, _) in code.iter().enumerate() {
                    map.push(char_idx);
                    // Add entry for element gap (except after last element)
                    if elem_idx < code.len() - 1 {
                        map.push(char_idx);
                    }
                }
                // Add entry for character gap (will be added after char)
                map.push(char_idx);
                char_idx += 1;
            }
        }
        map
    }

    /// Get number of characters fully sent (for TX indicator)
    pub fn chars_sent(&self) -> usize {
        if self.completed || self.element_to_char.is_empty() {
            return self.message.len();
        }
        if self.current_element_idx >= self.element_to_char.len() {
            return self.message.len();
        }
        // Return the character index that the current element belongs to
        self.element_to_char
            .get(self.current_element_idx)
            .copied()
            .unwrap_or(self.message.len())
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
    /// Noise generator for mono output and Radio 1 (left channel) in stereo
    pub noise: NoiseGenerator,
    /// Noise generator for Radio 2 (right channel) in stereo mode
    pub noise_right: NoiseGenerator,
    pub settings: AudioSettings,
    /// 2BSIQ: Whether stereo separation is enabled (true = L/R split, false = focused to both ears)
    pub stereo_enabled: bool,
    /// 2BSIQ: Which radio is focused (0 = Radio 1/left, 1 = Radio 2/right)
    pub focused_radio: u8,
    /// 2BSIQ: Whether 2BSIQ mode is enabled (disables sidetone when true)
    pub two_bsiq_enabled: bool,
    /// 2BSIQ: Latch mode - route other radio to both ears during TX
    pub latch_mode: bool,
    /// Which radio is currently transmitting (for TX indicator and latch mode)
    pub transmitting_radio: u8,
}

impl Mixer {
    pub fn new(sample_rate: u32, settings: AudioSettings) -> Self {
        Self {
            stations: Vec::new(),
            user_station: None,
            noise: NoiseGenerator::new(sample_rate),
            noise_right: NoiseGenerator::new(sample_rate),
            settings,
            stereo_enabled: true,
            focused_radio: 0,
            two_bsiq_enabled: false,
            latch_mode: false,
            transmitting_radio: 0,
        }
    }

    /// Update 2BSIQ stereo routing mode
    pub fn update_stereo_mode(&mut self, stereo_enabled: bool, focused_radio: u8) {
        self.stereo_enabled = stereo_enabled;
        self.focused_radio = focused_radio;
    }

    /// Update 2BSIQ mode (controls sidetone muting)
    pub fn update_2bsiq_mode(&mut self, enabled: bool) {
        self.two_bsiq_enabled = enabled;
    }

    /// Update latch mode (routes other radio to both ears during TX)
    pub fn update_latch_mode(&mut self, enabled: bool) {
        self.latch_mode = enabled;
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

    /// Start playing a user message on the specified radio
    pub fn play_user_message(&mut self, message: &str, wpm: u8, radio_index: u8) {
        self.transmitting_radio = radio_index;
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
        // Update noise filters to match tone frequency and bandwidth
        self.noise
            .update_filter(settings.tone_frequency_hz, settings.noise_bandwidth);
        self.noise_right
            .update_filter(settings.tone_frequency_hz, settings.noise_bandwidth);
        self.settings = settings;
    }

    /// Clear all stations
    pub fn clear_all(&mut self) {
        self.stations.clear();
        self.user_station = None;
    }

    /// Fill a buffer with mixed audio (mono), returns list of completed stations (id, radio_index)
    pub fn fill_buffer(&mut self, buffer: &mut [f32]) -> (Vec<(StationId, u8)>, bool) {
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
                completed_stations.push((station.id, station.radio_index));
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

    /// Get current TX progress for visual indicator
    /// Returns (message, chars_sent, radio_index) if user is transmitting, None otherwise
    pub fn get_tx_progress(&self) -> Option<(&str, usize, u8)> {
        self.user_station
            .as_ref()
            .map(|u| (u.message.as_str(), u.chars_sent(), self.transmitting_radio))
    }

    /// Fill a buffer with mixed stereo audio (interleaved L/R pairs)
    /// When stereo_enabled: stations routed based on radio_index (0 = left, 1 = right)
    /// When !stereo_enabled: focused radio goes to both ears
    /// Returns list of completed stations (id, radio_index) and whether user message completed
    pub fn fill_stereo_buffer(&mut self, buffer: &mut [f32]) -> (Vec<(StationId, u8)>, bool) {
        let mut completed_stations = Vec::new();
        let mut user_completed = false;

        // Buffer is interleaved stereo: [L0, R0, L1, R1, ...]
        let num_frames = buffer.len() / 2;

        // Clear buffer
        for sample in buffer.iter_mut() {
            *sample = 0.0;
        }

        // Determine audio routing mode:
        // - Latch active: user is transmitting in 2BSIQ with latch mode enabled
        //   -> route OTHER radio (not focused) to both ears
        // - Stereo mode: normal L/R separation
        // - Mono mode: focused radio to both ears
        let is_transmitting = self.user_station.is_some();
        let latch_active = self.two_bsiq_enabled && self.latch_mode && is_transmitting;

        // When latch is active, we want to hear the OTHER radio (not the one we're TXing on)
        // transmitting_radio is the one sending, so we hear the opposite
        let latch_radio = if self.transmitting_radio == 0 {
            1u8
        } else {
            0u8
        };

        // Add noise based on routing mode
        let mute_noise = self.settings.mute_noise_during_tx && is_transmitting;
        if !mute_noise {
            if latch_active {
                // Latch mode: other radio's noise to both ears
                for frame_idx in 0..num_frames {
                    let noise_sample = if latch_radio == 0 {
                        self.noise
                            .next_sample(self.settings.noise_level, &self.settings.noise)
                    } else {
                        self.noise_right
                            .next_sample(self.settings.noise_level, &self.settings.noise)
                    };
                    buffer[frame_idx * 2] += noise_sample; // Left
                    buffer[frame_idx * 2 + 1] += noise_sample; // Right
                }
            } else if self.stereo_enabled {
                // Stereo mode: independent noise per channel
                for frame_idx in 0..num_frames {
                    let noise_left = self
                        .noise
                        .next_sample(self.settings.noise_level, &self.settings.noise);
                    let noise_right = self
                        .noise_right
                        .next_sample(self.settings.noise_level, &self.settings.noise);
                    buffer[frame_idx * 2] += noise_left; // Left (Radio 1)
                    buffer[frame_idx * 2 + 1] += noise_right; // Right (Radio 2)
                }
            } else {
                // Mono mode: focused radio's noise to both ears
                for frame_idx in 0..num_frames {
                    let noise_sample = if self.focused_radio == 0 {
                        self.noise
                            .next_sample(self.settings.noise_level, &self.settings.noise)
                    } else {
                        self.noise_right
                            .next_sample(self.settings.noise_level, &self.settings.noise)
                    };
                    buffer[frame_idx * 2] += noise_sample; // Left
                    buffer[frame_idx * 2 + 1] += noise_sample; // Right
                }
            }
        }

        // Mix each calling station based on routing mode
        for station in &mut self.stations {
            if latch_active {
                // Latch mode: only hear other radio (not focused), route to both ears
                let dominated = station.radio_index == latch_radio;
                for frame_idx in 0..num_frames {
                    if let Some(station_sample) = station.next_sample() {
                        if dominated {
                            buffer[frame_idx * 2] += station_sample; // Left
                            buffer[frame_idx * 2 + 1] += station_sample; // Right
                        }
                        // Focused radio stations are silenced during latch
                    } else {
                        break;
                    }
                }
            } else if self.stereo_enabled {
                // Stereo mode: route to appropriate channel based on radio_index
                let channel_offset = station.radio_index as usize; // 0 = left, 1 = right
                for frame_idx in 0..num_frames {
                    if let Some(station_sample) = station.next_sample() {
                        buffer[frame_idx * 2 + channel_offset] += station_sample;
                    } else {
                        break;
                    }
                }
            } else {
                // Mono mode: only hear focused radio, route to both ears
                let dominated = station.radio_index == self.focused_radio;
                for frame_idx in 0..num_frames {
                    if let Some(station_sample) = station.next_sample() {
                        if dominated {
                            buffer[frame_idx * 2] += station_sample; // Left
                            buffer[frame_idx * 2 + 1] += station_sample; // Right
                        }
                        // Non-focused radio stations are silenced in mono mode
                    } else {
                        break;
                    }
                }
            }
            if station.is_completed() {
                completed_stations.push((station.id, station.radio_index));
            }
        }

        // Remove completed stations
        self.stations.retain(|s| !s.is_completed());

        // Mix user station to both channels (sidetone)
        // In 2BSIQ mode, sidetone is disabled - user sees TX indicator instead
        if let Some(ref mut user) = self.user_station {
            for frame_idx in 0..num_frames {
                if let Some(user_sample) = user.next_sample() {
                    // Only add audio if not in 2BSIQ mode
                    if !self.two_bsiq_enabled {
                        buffer[frame_idx * 2] += user_sample; // Left
                        buffer[frame_idx * 2 + 1] += user_sample; // Right
                    }
                } else {
                    break;
                }
            }
            if user.is_completed() {
                user_completed = true;
                self.user_station = None;
            }
        }

        // Apply master volume, dither, and soft clipping to all samples
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
}
