use serde::{Deserialize, Serialize};

/// Supported contest types
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ContestType {
    CqWw,
    NaSprint,
    Sweepstakes,
    Cwt,
}

impl ContestType {
    pub fn display_name(&self) -> &'static str {
        match self {
            ContestType::CqWw => "CQ World Wide",
            ContestType::NaSprint => "NA Sprint",
            ContestType::Sweepstakes => "ARRL Sweepstakes",
            ContestType::Cwt => "CWT",
        }
    }
}

/// Contest exchange data (varies by contest type)
#[derive(Clone, Debug)]
pub enum Exchange {
    CqWw {
        rst: String,
        zone: u8,
    },
    Sprint {
        serial: u32,
        name: String,
        qth: String,
    },
    Sweepstakes {
        serial: u32,
        precedence: char,
        check: u16,
        section: String,
    },
    Cwt {
        name: String,
        number: String, // member number or state/country
    },
}

/// User's parsed exchange entry
#[derive(Clone, Debug, Default)]
pub struct ParsedExchange {
    pub rst: Option<String>,
    pub zone: Option<u8>,
    pub serial: Option<u32>,
    pub name: Option<String>,
    pub qth: Option<String>,
    pub precedence: Option<char>,
    pub check: Option<u16>,
    pub section: Option<String>,
}

/// Result of validating user's exchange against expected
#[derive(Clone, Debug)]
pub struct ValidationResult {
    pub callsign_correct: bool,
    pub exchange_correct: bool,
    pub points: u32,
}

/// Trait for contest-specific behavior
pub trait Contest: Send + Sync {
    /// Get the contest type
    fn contest_type(&self) -> ContestType;

    /// Get display name
    fn name(&self) -> &'static str;

    /// Generate a random exchange for a calling station
    fn generate_exchange(&self, callsign: &str, serial: u32) -> Exchange;

    /// Format exchange for Morse transmission
    fn format_sent_exchange(&self, exchange: &Exchange) -> String;

    /// Get the user's exchange to send
    fn user_exchange(
        &self,
        callsign: &str,
        serial: u32,
        zone: u8,
        section: &str,
        name: &str,
    ) -> String;

    /// Validate user's logged exchange against expected
    fn validate(
        &self,
        expected_call: &str,
        expected_exchange: &Exchange,
        received_call: &str,
        received_exchange: &str,
    ) -> ValidationResult;
}
