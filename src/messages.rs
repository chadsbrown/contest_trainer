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
}

/// Messages from UI thread to Audio thread
#[derive(Clone, Debug)]
pub enum AudioCommand {
    /// Start playing morse for a station
    StartStation(StationParams),
    /// Stop a specific station
    StopStation(StationId),
    /// Play a message as the user's station (CQ, exchange, TU)
    PlayUserMessage { message: String, wpm: u8 },
    /// Update global audio settings
    UpdateSettings(AudioSettings),
    /// Stop all audio (except noise)
    StopAll,
}

/// Messages from Audio thread to UI thread
#[derive(Clone, Debug)]
pub enum AudioEvent {
    /// Station finished sending its message
    StationComplete(StationId),
    /// User message finished playing
    UserMessageComplete,
}
