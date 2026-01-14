use super::types::{Contest, ContestType, Exchange, ValidationResult};
use rand::seq::SliceRandom;

const NAMES: &[&str] = &[
    "BOB", "JIM", "TOM", "JOHN", "MIKE", "DAVE", "BILL", "JOE", "DAN", "RICK",
    "PAUL", "MARK", "GARY", "KEN", "RON", "DON", "JACK", "PETE", "AL", "ED",
    "STEVE", "FRED", "GEORGE", "FRANK", "LARRY", "JERRY", "RAY", "CARL", "RALPH", "BRUCE",
];

const STATES: &[&str] = &[
    "CT", "MA", "ME", "NH", "RI", "VT", "NJ", "NY", "DE", "MD", "PA",
    "AL", "FL", "GA", "KY", "NC", "SC", "TN", "VA", "AR", "LA", "MS", "NM", "OK", "TX",
    "CA", "AZ", "ID", "MT", "NV", "OR", "UT", "WA", "WY", "CO",
    "IA", "KS", "MN", "MO", "NE", "ND", "SD", "IL", "IN", "WI",
    "MI", "OH", "WV",
];

pub struct NaSprintContest;

impl NaSprintContest {
    pub fn new() -> Self {
        Self
    }

    fn qth_for_callsign(callsign: &str) -> String {
        // Simple mapping based on call area
        let digit = callsign.chars().find(|c| c.is_ascii_digit());

        match digit {
            Some('1') => "CT",
            Some('2') => "NY",
            Some('3') => "PA",
            Some('4') => "VA",
            Some('5') => "TX",
            Some('6') => "CA",
            Some('7') => "WA",
            Some('8') => "OH",
            Some('9') => "IL",
            Some('0') => "CO",
            _ => "CA",
        }.to_string()
    }
}

impl Contest for NaSprintContest {
    fn contest_type(&self) -> ContestType {
        ContestType::NaSprint
    }

    fn name(&self) -> &'static str {
        "North American Sprint"
    }

    fn generate_exchange(&self, callsign: &str, serial: u32) -> Exchange {
        let mut rng = rand::thread_rng();
        let name = NAMES.choose(&mut rng).unwrap_or(&"BOB").to_string();
        let qth = Self::qth_for_callsign(callsign);

        Exchange::Sprint {
            serial,
            name,
            qth,
        }
    }

    fn format_sent_exchange(&self, exchange: &Exchange) -> String {
        match exchange {
            Exchange::Sprint { serial, name, qth } => {
                format!("{} {} {}", serial, name, qth)
            }
            _ => String::new(),
        }
    }

    fn user_exchange(&self, _callsign: &str, serial: u32, _zone: u8, section: &str, name: &str) -> String {
        format!("{} {} {}", serial, name, section)
    }

    fn validate(&self, expected_call: &str, expected_exchange: &Exchange,
                received_call: &str, received_exchange: &str) -> ValidationResult {
        let callsign_correct = expected_call.eq_ignore_ascii_case(received_call);

        let exchange_correct = match expected_exchange {
            Exchange::Sprint { serial, name, qth } => {
                let parts: Vec<&str> = received_exchange.split_whitespace().collect();
                if parts.len() >= 3 {
                    let serial_ok = parts[0].parse::<u32>().map(|s| s == *serial).unwrap_or(false);
                    let name_ok = parts[1].eq_ignore_ascii_case(name);
                    let qth_ok = parts[2].eq_ignore_ascii_case(qth);
                    serial_ok && name_ok && qth_ok
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
