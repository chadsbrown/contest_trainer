use crate::config::AudioSettings;
use crate::contest::Exchange;

/// Unique identifier for a calling station
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct StationId(pub u32);

/// Type of segment within a user message
/// Used for element-level tracking of what has been transmitted
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MessageSegmentType {
    /// The caller's callsign (e.g., "W1ABC")
    TheirCallsign,
    /// Our exchange (e.g., "5NN 05")
    OurExchange,
    /// CQ call (e.g., "CQ TEST K1ABC")
    Cq,
    /// Thank you / QSO complete
    Tu,
    /// AGN or ? request
    Agn,
}

/// A segment of a user message with its type
#[derive(Clone, Debug)]
pub struct MessageSegment {
    pub content: String,
    pub segment_type: MessageSegmentType,
}

/// Parameters defining a calling station
#[derive(Clone, Debug)]
pub struct StationParams {
    pub id: StationId,
    pub callsign: String,
    pub exchange: Exchange,
    pub frequency_offset_hz: f32,
    pub wpm: u8,
    pub amplitude: f32,
    /// Delay in milliseconds before this station starts transmitting
    pub reaction_delay_ms: u32,
}

/// Messages from UI thread to Audio thread
#[derive(Clone, Debug)]
pub enum AudioCommand {
    /// Start playing morse for a station
    StartStation(StationParams),
    /// Play a segmented message with element-level completion tracking
    /// Each segment will emit a UserSegmentComplete event when finished
    PlayUserMessageSegmented {
        segments: Vec<MessageSegment>,
        wpm: u8,
    },
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
    /// A segment of the user message finished playing
    /// Emitted for each segment in a segmented message before UserMessageComplete
    UserSegmentComplete(MessageSegmentType),
}
