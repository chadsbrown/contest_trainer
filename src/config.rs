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
    pub wpm: u8,
    pub font_size: f32,
    pub agn_message: String,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ContestSettings {
    pub contest_type: ContestType,
    pub callsign_file: String,
    pub cwt_callsign_file: String,
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
            callsign: "N9UNX".to_string(),
            name: "OP".to_string(),
            zone: 5,
            section: "CT".to_string(),
            wpm: 32,
            font_size: 14.0,
            agn_message: "?".to_string(),
        }
    }
}

impl Default for ContestSettings {
    fn default() -> Self {
        Self {
            contest_type: ContestType::Cwt,
            callsign_file: "callsigns.txt".to_string(),
            cwt_callsign_file: "cwt_callsigns.txt".to_string(),
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
            station_probability: 0.7,
            wpm_min: 28,
            wpm_max: 36,
            frequency_spread_hz: 400.0,
            amplitude_min: 0.4,
            amplitude_max: 1.0,
        }
    }
}

impl AppSettings {
    /// Get the default config file path
    pub fn config_path() -> std::path::PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("contest_trainer").join("settings.toml")
        } else {
            std::path::PathBuf::from("settings.toml")
        }
    }

    /// Load settings from the default config path, or return defaults if not found
    pub fn load_or_default() -> Self {
        let path = Self::config_path();
        match Self::load(&path) {
            Ok(settings) => {
                eprintln!("Loaded settings from {}", path.display());
                settings
            }
            Err(_) => {
                eprintln!("Using default settings (no config at {})", path.display());
                Self::default()
            }
        }
    }

    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let settings: Self = toml::from_str(&content)?;
        Ok(settings)
    }

    pub fn save(&self) -> Result<(), Box<dyn std::error::Error>> {
        let path = Self::config_path();

        // Create parent directory if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        eprintln!("Saved settings to {}", path.display());
        Ok(())
    }
}
