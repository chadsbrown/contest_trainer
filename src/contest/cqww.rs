use super::types::{Contest, ContestType, Exchange, ValidationResult};

pub struct CqWwContest;

impl CqWwContest {
    pub fn new() -> Self {
        Self
    }

    /// Determine CQ zone from callsign prefix (simplified)
    fn zone_for_callsign(callsign: &str) -> u8 {
        let prefix = callsign.chars().take(2).collect::<String>().to_uppercase();

        // Simplified zone mapping based on prefix
        match prefix.as_str() {
            // USA
            p if p.starts_with('W') || p.starts_with('K') || p.starts_with('N') || p.starts_with("AA") => {
                // Very simplified: just use zone 5 for all US
                5
            }
            // Canada
            p if p.starts_with("VE") || p.starts_with("VA") => 4,
            // Japan
            p if p.starts_with("JA") || p.starts_with("JH") || p.starts_with("JR") => 25,
            // Germany
            p if p.starts_with("DL") || p.starts_with("DF") || p.starts_with("DK") => 14,
            // UK
            p if p.starts_with('G') || p.starts_with('M') => 14,
            // Spain
            p if p.starts_with("EA") => 14,
            // Italy
            p if p.starts_with('I') => 15,
            // France
            p if p.starts_with('F') => 14,
            // Russia
            p if p.starts_with("UA") || p.starts_with("RU") => 16,
            // Australia
            p if p.starts_with("VK") => 30,
            // New Zealand
            p if p.starts_with("ZL") => 32,
            // South Africa
            p if p.starts_with("ZS") => 38,
            // Brazil
            p if p.starts_with("PY") || p.starts_with("PP") => 11,
            // Argentina
            p if p.starts_with("LU") => 13,
            _ => 5, // Default
        }
    }
}

impl Contest for CqWwContest {
    fn contest_type(&self) -> ContestType {
        ContestType::CqWw
    }

    fn name(&self) -> &'static str {
        "CQ World Wide DX"
    }

    fn generate_exchange(&self, callsign: &str, _serial: u32) -> Exchange {
        Exchange::CqWw {
            rst: "5NN".to_string(), // Contest shorthand for 599
            zone: Self::zone_for_callsign(callsign),
        }
    }

    fn format_sent_exchange(&self, exchange: &Exchange) -> String {
        match exchange {
            Exchange::CqWw { rst, zone } => {
                format!("{} {:02}", rst, zone)
            }
            _ => String::new(),
        }
    }

    fn user_exchange(&self, _callsign: &str, _serial: u32, zone: u8, _section: &str, _name: &str) -> String {
        format!("5NN {:02}", zone)
    }

    fn validate(&self, expected_call: &str, expected_exchange: &Exchange,
                received_call: &str, received_exchange: &str) -> ValidationResult {
        let callsign_correct = expected_call.eq_ignore_ascii_case(received_call);

        let exchange_correct = match expected_exchange {
            Exchange::CqWw { zone, .. } => {
                // Parse received exchange - expect "599 ZZ" or "5NN ZZ" format
                let parts: Vec<&str> = received_exchange.split_whitespace().collect();
                if parts.len() >= 2 {
                    parts[1].parse::<u8>().map(|z| z == *zone).unwrap_or(false)
                } else if parts.len() == 1 {
                    // Just the zone number
                    parts[0].parse::<u8>().map(|z| z == *zone).unwrap_or(false)
                } else {
                    false
                }
            }
            _ => false,
        };

        ValidationResult {
            callsign_correct,
            exchange_correct,
            points: if callsign_correct && exchange_correct { 1 } else { 0 },
        }
    }
}
