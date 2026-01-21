use super::types::{Contest, ContestType, Exchange, ValidationResult};
use crate::cty::CtyDat;

pub struct CqWwContest {
    cty: CtyDat,
}

impl CqWwContest {
    pub fn new() -> Self {
        // Load embedded cty.dat
        let cty_data = include_str!("../../data/cty.dat");
        let cty = CtyDat::parse(cty_data);
        Self { cty }
    }

    /// Determine CQ zone from callsign using CTY database
    fn zone_for_callsign(&self, callsign: &str) -> u8 {
        self.cty.lookup_cq_zone(callsign).unwrap_or(5)
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
            zone: self.zone_for_callsign(callsign),
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

    fn user_exchange(
        &self,
        _callsign: &str,
        _serial: u32,
        zone: u8,
        _section: &str,
        _name: &str,
    ) -> String {
        format!("5NN {:02}", zone)
    }

    fn validate(
        &self,
        expected_call: &str,
        expected_exchange: &Exchange,
        received_call: &str,
        received_exchange: &str,
    ) -> ValidationResult {
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
            points: if callsign_correct && exchange_correct {
                1
            } else {
                0
            },
        }
    }
}
