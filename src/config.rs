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
    #[serde(default = "default_true")]
    pub show_status_line: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ContestSettings {
    pub contest_type: ContestType,
    pub callsign_file: String,
    pub cwt_callsign_file: String,
    pub cq_message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AudioSettings {
    pub sample_rate: u32,
    pub tone_frequency_hz: f32,
    pub noise_level: f32,
    pub master_volume: f32,
    #[serde(default = "default_true")]
    pub mute_noise_during_tx: bool,
    /// Noise filter bandwidth in Hz (simulates receiver CW filter)
    #[serde(default = "default_noise_bandwidth")]
    pub noise_bandwidth: f32,
    #[serde(default)]
    pub noise: NoiseSettings,
    #[serde(default)]
    pub qsb: QsbSettings,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QsbSettings {
    /// Whether QSB (fading) is enabled
    pub enabled: bool,
    /// Depth of fading (0.0 = none, 1.0 = full fade to silence)
    pub depth: f32,
    /// Average fading cycle rate in cycles per minute
    pub rate: f32,
}

fn default_true() -> bool {
    true
}

fn default_noise_bandwidth() -> f32 {
    400.0
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NoiseSettings {
    /// Rate of static crashes per second (0.0 to disable)
    pub crash_rate: f32,
    /// Intensity of crashes (0.0 - 1.0)
    pub crash_intensity: f32,
    /// Rate of pops/clicks per second (0.0 to disable)
    pub pop_rate: f32,
    /// Intensity of pops (0.0 - 1.0)
    pub pop_intensity: f32,
    /// QRN (atmospheric noise) intensity (0.0 - 1.0)
    pub qrn_intensity: f32,
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
    #[serde(default)]
    pub agn_request_probability: f32,
    /// Whether to filter callers based on country
    #[serde(default)]
    pub same_country_filter_enabled: bool,
    /// Probability of a caller being from the same country as the user (0.0 - 1.0)
    #[serde(default)]
    pub same_country_probability: f32,
    /// Pileup persistence settings
    #[serde(default)]
    pub pileup: PileupSettings,
    /// Call correction settings
    #[serde(default)]
    pub call_correction: CallCorrectionSettings,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CallCorrectionSettings {
    /// Probability caller will correct a busted callsign (vs just proceeding)
    pub correction_probability: f32,
    /// Max times caller will try to correct before giving up
    pub max_correction_attempts: u8,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct PileupSettings {
    /// Minimum patience (call attempts) for new callers
    pub min_patience: u8,
    /// Maximum patience (call attempts) for new callers
    pub max_patience: u8,
    /// Minimum delay before retry (ms)
    pub retry_delay_min_ms: u32,
    /// Maximum delay before retry (ms)
    pub retry_delay_max_ms: u32,
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
            show_status_line: true,
        }
    }
}

impl Default for ContestSettings {
    fn default() -> Self {
        Self {
            contest_type: ContestType::Cwt,
            callsign_file: "callsigns.txt".to_string(),
            cwt_callsign_file: "cwt_callsigns.txt".to_string(),
            cq_message: "CQ TEST".to_string(),
        }
    }
}

impl Default for AudioSettings {
    fn default() -> Self {
        Self {
            sample_rate: 44100,
            tone_frequency_hz: 600.0,
            noise_level: 0.25,
            master_volume: 0.7,
            mute_noise_during_tx: true,
            noise_bandwidth: 350.0,
            noise: NoiseSettings::default(),
            qsb: QsbSettings::default(),
        }
    }
}

impl Default for NoiseSettings {
    fn default() -> Self {
        Self {
            crash_rate: 0.4,
            crash_intensity: 0.2,
            pop_rate: 0.6,
            pop_intensity: 0.73,
            qrn_intensity: 0.3,
        }
    }
}

impl Default for QsbSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            depth: 0.5,
            rate: 4.0, // 6 cycles per minute = 10 second period
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
            agn_request_probability: 0.1,
            same_country_filter_enabled: false,
            same_country_probability: 0.1,
            pileup: PileupSettings::default(),
            call_correction: CallCorrectionSettings::default(),
        }
    }
}

impl Default for CallCorrectionSettings {
    fn default() -> Self {
        Self {
            correction_probability: 0.8,
            max_correction_attempts: 2,
        }
    }
}

impl Default for PileupSettings {
    fn default() -> Self {
        Self {
            min_patience: 2,
            max_patience: 5,
            retry_delay_min_ms: 200,
            retry_delay_max_ms: 1200,
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
                #[cfg(debug_assertions)]
                eprintln!("Loaded settings from {}", path.display());
                settings
            }
            Err(_) => {
                #[cfg(debug_assertions)]
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
        #[cfg(debug_assertions)]
        eprintln!("Saved settings to {}", path.display());
        Ok(())
    }
}
