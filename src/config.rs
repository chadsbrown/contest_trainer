use crate::contest::ContestType;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub user: UserSettings,
    pub contest: ContestSettings,
    pub audio: AudioSettings,
    pub simulation: SimulationSettings,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub callsign: String,
    pub name: String,
    pub zone: u8,
    pub section: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ContestSettings {
    pub contest_type: ContestType,
    pub callsign_file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioSettings {
    pub sample_rate: u32,
    pub tone_frequency_hz: f32,
    pub noise_level: f32,
    pub master_volume: f32,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct SimulationSettings {
    pub max_simultaneous_stations: u8,
    pub station_probability: f32,
    pub wpm_min: u8,
    pub wpm_max: u8,
    pub frequency_spread_hz: f32,
    pub amplitude_min: f32,
    pub amplitude_max: f32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            user: UserSettings::default(),
            contest: ContestSettings::default(),
            audio: AudioSettings::default(),
            simulation: SimulationSettings::default(),
        }
    }
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            callsign: "W1ABC".to_string(),
            name: "OP".to_string(),
            zone: 5,
            section: "CT".to_string(),
        }
    }
}

impl Default for ContestSettings {
    fn default() -> Self {
        Self {
            contest_type: ContestType::CqWw,
            callsign_file: "callsigns.txt".to_string(),
        }
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            tone_frequency_hz: 600.0,
            noise_level: 0.15,
            master_volume: 0.7,
        }
    }
}

impl Default for SimulationSettings {
    fn default() -> Self {
        Self {
            max_simultaneous_stations: 2,
            station_probability: 0.3,
            wpm_min: 22,
            wpm_max: 32,
            frequency_spread_hz: 400.0,
            amplitude_min: 0.4,
            amplitude_max: 1.0,
        }
    }
}

impl AppSettings {
    pub fn load(path: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let settings: Self = toml::from_str(&content)?;
        Ok(settings)
    }

    pub fn save(&self, path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
