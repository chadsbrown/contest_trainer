use crate::config::AudioSettings;
use crate::contest::Exchange;

/// Unique identifier for a calling station
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StationId(pub u32);

/// Parameters defining a calling station
#[derive(Clone, Debug)]
pub struct StationParams {
    pub id: StationId,
    pub callsign: String,
    pub exchange: Exchange,
    pub frequency_offset_hz: f32,
    pub wpm: u8,
    pub amplitude: f32,
    /// Radio index for 2BSIQ mode: 0 = Radio 1 (left), 1 = Radio 2 (right)
    pub radio_index: u8,
}

/// Messages from UI thread to Audio thread
#[derive(Clone, Debug)]
pub enum AudioCommand {
    /// Start playing morse for a station
    StartStation(StationParams),
    /// Play a message as the user's station (CQ, exchange, TU)
    PlayUserMessage {
        message: String,
        wpm: u8,
        /// Which radio is transmitting (0 = Radio 1, 1 = Radio 2)
        radio_index: u8,
    },
    /// Update global audio settings
    UpdateSettings(AudioSettings),
    /// Stop all audio (except noise) - use StopRadio in 2BSIQ mode
    StopAll,
    /// Stop audio for a specific radio only (for 2BSIQ mode)
    StopRadio {
        /// Which radio to stop (0 = Radio 1, 1 = Radio 2)
        radio_index: u8,
    },
    /// Update 2BSIQ stereo routing mode
    UpdateStereoMode {
        /// Whether stereo separation is enabled (true = L/R split, false = focused to both)
        stereo_enabled: bool,
        /// Which radio is focused (0 = Radio 1/left, 1 = Radio 2/right)
        focused_radio: u8,
    },
    /// Update 2BSIQ mode state (controls sidetone muting)
    Update2BsiqMode {
        /// Whether 2BSIQ mode is enabled
        enabled: bool,
    },
    /// Update latch mode setting (routes other radio to both ears during TX)
    UpdateLatchMode {
        /// Whether latch mode is enabled
        enabled: bool,
    },
    /// Update per-radio volume levels
    UpdateRadioVolumes {
        /// Volume for Radio 1 (0.0 to 1.0)
        radio1_volume: f32,
        /// Volume for Radio 2 (0.0 to 1.0)
        radio2_volume: f32,
    },
}

/// Messages from Audio thread to UI thread
#[derive(Clone, Debug)]
pub enum AudioEvent {
    /// Station finished sending its message (includes radio_index for 2BSIQ routing)
    StationComplete { id: StationId, radio_index: u8 },
    /// User message finished playing (includes radio_index for 2BSIQ routing)
    UserMessageComplete { radio_index: u8 },
}
