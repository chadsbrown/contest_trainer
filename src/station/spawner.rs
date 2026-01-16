use rand::Rng;
use std::time::Instant;

use super::callsign::{CallsignPool, CwtCallsignPool};
use crate::config::SimulationSettings;
use crate::contest::{Contest, Exchange};
use crate::messages::{StationId, StationParams};

/// Source of callsigns - either regular or CWT-specific
pub enum CallsignSource {
    Regular(CallsignPool),
    Cwt(CwtCallsignPool),
}

impl CallsignSource {
    /// Get a random callsign and exchange
    pub fn random(&mut self, contest: &dyn Contest, serial: u32) -> Option<(String, Exchange)> {
        match self {
            CallsignSource::Regular(pool) => {
                let callsign = pool.random()?;
                let exchange = contest.generate_exchange(&callsign, serial);
                Some((callsign, exchange))
            }
            CallsignSource::Cwt(pool) => pool.random(),
        }
    }

    pub fn reset(&mut self) {
        match self {
            CallsignSource::Regular(pool) => pool.reset(),
            CallsignSource::Cwt(pool) => pool.reset(),
        }
    }
}

pub struct StationSpawner {
    callsigns: CallsignSource,
    settings: SimulationSettings,
    next_id: u32,
    active_count: usize,
    last_spawn_time: Instant,
    serial_counter: u32,
}

impl StationSpawner {
    pub fn new(callsigns: CallsignPool, settings: SimulationSettings) -> Self {
        Self {
            callsigns: CallsignSource::Regular(callsigns),
            settings,
            next_id: 0,
            active_count: 0,
            last_spawn_time: Instant::now(),
            serial_counter: 1,
        }
    }

    pub fn new_cwt(callsigns: CwtCallsignPool, settings: SimulationSettings) -> Self {
        Self {
            callsigns: CallsignSource::Cwt(callsigns),
            settings,
            next_id: 0,
            active_count: 0,
            last_spawn_time: Instant::now(),
            serial_counter: 1,
        }
    }

    /// Update settings
    pub fn update_settings(&mut self, settings: SimulationSettings) {
        self.settings = settings;
    }

    /// Update callsign pool (regular)
    pub fn update_callsigns(&mut self, callsigns: CallsignPool) {
        self.callsigns = CallsignSource::Regular(callsigns);
    }

    /// Update callsign pool (CWT)
    pub fn update_cwt_callsigns(&mut self, callsigns: CwtCallsignPool) {
        self.callsigns = CallsignSource::Cwt(callsigns);
    }

    /// Called each frame to potentially spawn new stations
    /// Returns a list of stations to spawn (may be empty or have multiple)
    pub fn tick(&mut self, contest: &dyn Contest) -> Vec<StationParams> {
        let mut stations = Vec::new();
        let mut rng = rand::thread_rng();

        // Minimum time between spawn attempts
        if self.last_spawn_time.elapsed().as_millis() < 200 {
            return stations;
        }

        // Check if we can spawn more stations
        while self.active_count < self.settings.max_simultaneous_stations as usize {
            // Check probability for spawning
            if rng.gen::<f32>() > self.settings.station_probability {
                break;
            }

            // Pick a random callsign and get exchange
            let Some((callsign, exchange)) = self.callsigns.random(contest, self.serial_counter)
            else {
                break;
            };
            self.serial_counter += 1;

            // Random parameters within configured ranges
            let wpm = rng.gen_range(self.settings.wpm_min..=self.settings.wpm_max);
            let freq_offset = rng
                .gen_range(-self.settings.frequency_spread_hz..self.settings.frequency_spread_hz);
            let amplitude = rng.gen_range(self.settings.amplitude_min..self.settings.amplitude_max);

            self.next_id += 1;
            self.active_count += 1;

            stations.push(StationParams {
                id: StationId(self.next_id),
                callsign,
                exchange,
                frequency_offset_hz: freq_offset,
                wpm,
                amplitude,
            });

            // Small delay between multiple spawns within same tick
            if self.active_count < self.settings.max_simultaneous_stations as usize {
                // Only spawn multiple if we roll for it
                if rng.gen::<f32>() > 0.3 {
                    break;
                }
            }
        }

        if !stations.is_empty() {
            self.last_spawn_time = Instant::now();
        }

        stations
    }

    /// Called when a station completes
    pub fn station_completed(&mut self) {
        self.active_count = self.active_count.saturating_sub(1);
    }

    /// Reset all state
    pub fn reset(&mut self) {
        self.active_count = 0;
        self.callsigns.reset();
    }

    /// Get current active station count
    pub fn active_count(&self) -> usize {
        self.active_count
    }
}
