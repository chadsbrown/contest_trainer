use super::types::{Contest, Exchange, ValidationResult};

/// CWT (CW Ops CW Test) contest
/// Exchange: Name + Member Number (or state/country if not a member)
pub struct CwtContest;

impl CwtContest {
    pub fn new() -> Self {
        Self
    }
}

impl Contest for CwtContest {
    fn generate_exchange(&self, _callsign: &str, _serial: u32) -> Exchange {
        // This will be overridden by the CWT callsign pool which provides name/number
        Exchange::Cwt {
            name: "BOB".to_string(),
            number: "1234".to_string(),
        }
    }

    fn format_sent_exchange(&self, exchange: &Exchange) -> String {
        match exchange {
            Exchange::Cwt { name, number } => {
                format!("{} {}", name, number)
            }
            _ => String::new(),
        }
    }

    fn user_exchange(
        &self,
        _callsign: &str,
        _serial: u32,
        _zone: u8,
        _section: &str,
        name: &str,
    ) -> String {
        // For CWT, user sends their name and member number
        // We'll use section field to store member number for now
        format!("{} {}", name, _section)
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
            Exchange::Cwt { name, number } => {
                // Parse received exchange - expect "NAME NUMBER" format
                // Both parts are required for a correct exchange
                let parts: Vec<&str> = received_exchange.split_whitespace().collect();
                if parts.len() >= 2 {
                    let name_correct = parts[0].eq_ignore_ascii_case(name);
                    let number_correct = parts[1].eq_ignore_ascii_case(number);
                    name_correct && number_correct
                } else {
                    // Must have both name and number
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
