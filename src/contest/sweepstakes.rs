use super::types::{Contest, Exchange, ValidationResult};
use rand::Rng;

const PRECEDENCES: &[char] = &['A', 'B', 'M', 'Q', 'S', 'U'];

pub struct SweepstakesContest;

impl SweepstakesContest {
    pub fn new() -> Self {
        Self
    }

    fn section_for_callsign(callsign: &str) -> String {
        // Simple mapping based on call area
        let digit = callsign.chars().find(|c| c.is_ascii_digit());

        match digit {
            Some('1') => "CT",
            Some('2') => "NNJ",
            Some('3') => "EPA",
            Some('4') => "VA",
            Some('5') => "NTX",
            Some('6') => "SDG",
            Some('7') => "OR",
            Some('8') => "OH",
            Some('9') => "IL",
            Some('0') => "CO",
            _ => "SDG",
        }
        .to_string()
    }
}

impl Contest for SweepstakesContest {
    fn generate_exchange(&self, callsign: &str, serial: u32) -> Exchange {
        let mut rng = rand::thread_rng();
        let precedence = *PRECEDENCES
            .get(rng.gen_range(0..PRECEDENCES.len()))
            .unwrap_or(&'A');
        let check = rng.gen_range(60..=99) as u16; // Year first licensed
        let section = Self::section_for_callsign(callsign);

        Exchange::Sweepstakes {
            serial,
            precedence,
            check,
            section,
        }
    }

    fn format_sent_exchange(&self, exchange: &Exchange) -> String {
        match exchange {
            Exchange::Sweepstakes {
                serial,
                precedence,
                check,
                section,
            } => {
                // SS format: NR PREC CALL CK SEC
                format!("{} {} {:02} {}", serial, precedence, check, section)
            }
            _ => String::new(),
        }
    }

    fn user_exchange(
        &self,
        callsign: &str,
        serial: u32,
        _zone: u8,
        section: &str,
        _name: &str,
    ) -> String {
        // User sends: NR PREC CALL CK SEC
        // For simplicity, use 'A' precedence and '99' check
        format!("{} A {} 99 {}", serial, callsign, section)
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
            Exchange::Sweepstakes {
                serial,
                precedence,
                check,
                section,
            } => {
                let parts: Vec<&str> = received_exchange.split_whitespace().collect();
                // Expect: NR PREC CK SEC (4 parts minimum)
                if parts.len() >= 4 {
                    let serial_ok = parts[0]
                        .parse::<u32>()
                        .map(|s| s == *serial)
                        .unwrap_or(false);
                    let prec_ok = parts[1]
                        .chars()
                        .next()
                        .map(|c| c.to_ascii_uppercase() == *precedence)
                        .unwrap_or(false);
                    let check_ok = parts[2]
                        .parse::<u16>()
                        .map(|c| c == *check)
                        .unwrap_or(false);
                    let section_ok = parts[3].eq_ignore_ascii_case(section);
                    serial_ok && prec_ok && check_ok && section_ok
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
                2
            } else {
                0
            },
        }
    }
}
