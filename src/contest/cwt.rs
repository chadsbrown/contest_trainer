use std::collections::HashSet;
use std::path::Path;

use rand::seq::SliceRandom;
use toml::value::Table;

use super::types::{
    CallsignSource, Contest, Exchange, ExchangeField, FieldKind, SettingField, SettingFieldGroup,
    SettingFieldKind, ValidationResult,
};

pub const CONTEST_ID: &str = "cwt";
pub const DISPLAY_NAME: &str = "CWT";

/// CWT (CW Ops CW Test) contest
/// Exchange: Name + Member Number (or state/country if not a member)
pub struct CwtContest;

pub fn make_contest() -> Box<dyn Contest> {
    Box::new(CwtContest::new())
}

impl CwtContest {
    pub fn new() -> Self {
        Self
    }

    fn get_string(settings: &toml::Value, key: &str, default: &str) -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }
}

/// CWT station data (callsign + name + member number)
#[derive(Clone, Debug)]
struct CwtStation {
    callsign: String,
    name: String,
    number: String,
}

/// Pool of CWT stations with name/number data
struct CwtCallsignSource {
    stations: Vec<CwtStation>,
    used: HashSet<String>,
}

impl CwtCallsignSource {
    /// Load CWT stations from a file
    ///
    /// Format: CSV with fields: callsign, name, number (member # or state/country)
    /// Lines starting with # or ! are ignored
    /// Only lines with non-blank first three fields are accepted
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let stations: Vec<CwtStation> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
            .filter_map(|line| {
                let fields: Vec<&str> = line.split(',').map(|f| f.trim()).collect();
                if fields.len() >= 3 {
                    let callsign = fields[0].to_uppercase();
                    let name = fields[1].to_uppercase();
                    let number = fields[2].to_uppercase();

                    if !callsign.is_empty()
                        && !name.is_empty()
                        && !number.is_empty()
                        && is_valid_callsign(&callsign)
                    {
                        return Some(CwtStation {
                            callsign,
                            name,
                            number,
                        });
                    }
                }
                None
            })
            .collect();

        if stations.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid CWT stations found in file",
            ));
        }

        Ok(Self {
            stations,
            used: HashSet::new(),
        })
    }

    fn default_pool() -> Self {
        let stations = vec![
            CwtStation {
                callsign: "W1AW".to_string(),
                name: "JOE".to_string(),
                number: "1".to_string(),
            },
            CwtStation {
                callsign: "K5ZD".to_string(),
                name: "RANDY".to_string(),
                number: "2".to_string(),
            },
            CwtStation {
                callsign: "N1MM".to_string(),
                name: "TOM".to_string(),
                number: "100".to_string(),
            },
            CwtStation {
                callsign: "K3LR".to_string(),
                name: "TIM".to_string(),
                number: "55".to_string(),
            },
            CwtStation {
                callsign: "W9RE".to_string(),
                name: "MIKE".to_string(),
                number: "IN".to_string(),
            },
        ];

        Self {
            stations,
            used: HashSet::new(),
        }
    }

    fn random_station(&mut self) -> Option<CwtStation> {
        let available: Vec<_> = self
            .stations
            .iter()
            .filter(|s| !self.used.contains(&s.callsign))
            .collect();

        let station = if available.is_empty() {
            self.used.clear();
            self.stations.choose(&mut rand::thread_rng())?
        } else {
            *available.choose(&mut rand::thread_rng())?
        };

        self.used.insert(station.callsign.clone());
        Some(station.clone())
    }
}

impl CallsignSource for CwtCallsignSource {
    fn random(
        &mut self,
        _contest: &dyn Contest,
        _serial: u32,
        _settings: &toml::Value,
    ) -> Option<(String, Exchange)> {
        let station = self.random_station()?;
        Some((
            station.callsign.clone(),
            Exchange::new(vec![station.name.clone(), station.number.clone()]),
        ))
    }
}

fn is_valid_callsign(call: &str) -> bool {
    if call.len() < 3 || call.len() > 10 {
        return false;
    }
    call.chars().any(|c| c.is_ascii_alphabetic())
        && call.chars().any(|c| c.is_ascii_digit())
        && call.chars().all(|c| c.is_ascii_alphanumeric() || c == '/')
}

impl Contest for CwtContest {
    fn id(&self) -> &'static str {
        CONTEST_ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn exchange_fields(&self) -> Vec<ExchangeField> {
        vec![
            ExchangeField::new("name", "Name", "BOB", 8, FieldKind::Text),
            ExchangeField::new("number", "Number", "123", 6, FieldKind::Alnum),
        ]
    }

    fn settings_fields(&self) -> Vec<SettingField> {
        vec![
            SettingField {
                key: "cq_message",
                label: "CQ Message",
                placeholder: "CQ TEST",
                width_chars: 12,
                kind: SettingFieldKind::Text,
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "callsign_file",
                label: "CWT Callsign File",
                placeholder: "cwt_callsigns.txt",
                width_chars: 24,
                kind: SettingFieldKind::FilePath,
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "user_name",
                label: "Your Name",
                placeholder: "OP",
                width_chars: 8,
                kind: SettingFieldKind::Text,
                group: SettingFieldGroup::UserExchange,
            },
            SettingField {
                key: "user_number",
                label: "Your Number",
                placeholder: "CT",
                width_chars: 6,
                kind: SettingFieldKind::Text,
                group: SettingFieldGroup::UserExchange,
            },
        ]
    }

    fn default_settings(&self) -> toml::Value {
        let mut table = Table::new();
        table.insert(
            "cq_message".to_string(),
            toml::Value::String("CQ TEST".to_string()),
        );
        table.insert(
            "callsign_file".to_string(),
            toml::Value::String("cwt_callsigns.txt".to_string()),
        );
        table.insert(
            "user_name".to_string(),
            toml::Value::String("OP".to_string()),
        );
        table.insert(
            "user_number".to_string(),
            toml::Value::String("CT".to_string()),
        );
        toml::Value::Table(table)
    }

    fn cq_message(&self, settings: &toml::Value) -> String {
        Self::get_string(settings, "cq_message", "CQ TEST")
    }

    fn callsign_source(&self, settings: &toml::Value) -> Result<Box<dyn CallsignSource>, String> {
        let path = Self::get_string(settings, "callsign_file", "cwt_callsigns.txt");
        match CwtCallsignSource::load(&path) {
            Ok(source) => Ok(Box::new(source)),
            Err(_) => Ok(Box::new(CwtCallsignSource::default_pool())),
        }
    }

    fn generate_exchange(
        &self,
        _callsign: &str,
        _serial: u32,
        _settings: &toml::Value,
    ) -> Exchange {
        Exchange::new(vec!["BOB".to_string(), "1234".to_string()])
    }

    fn user_exchange_fields(
        &self,
        _user_callsign: &str,
        _serial: u32,
        settings: &toml::Value,
    ) -> Vec<String> {
        let name = Self::get_string(settings, "user_name", "OP");
        let number = Self::get_string(settings, "user_number", "CT");
        vec![name, number]
    }

    fn validate(
        &self,
        expected_call: &str,
        expected_exchange: &Exchange,
        received_call: &str,
        received_fields: &[String],
        _settings: &toml::Value,
    ) -> ValidationResult {
        let callsign_correct = expected_call.eq_ignore_ascii_case(received_call);

        let exchange_correct = if expected_exchange.fields.len() >= 2 && received_fields.len() >= 2
        {
            let name_correct =
                received_fields[0].eq_ignore_ascii_case(&expected_exchange.fields[0]);
            let number_correct =
                received_fields[1].eq_ignore_ascii_case(&expected_exchange.fields[1]);
            name_correct && number_correct
        } else {
            false
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
