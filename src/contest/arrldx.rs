use std::collections::HashSet;
use std::path::Path;

use rand::seq::SliceRandom;
use toml::value::Table;

use super::types::{
    CallsignSource, Contest, Exchange, ExchangeField, FieldKind, SettingField, SettingFieldGroup,
    SettingFieldKind, ValidationResult,
};

pub const CONTEST_ID: &str = "arrldx";
pub const DISPLAY_NAME: &str = "ARRL DX CW";

pub struct ArrlDxContest;

pub fn make_contest() -> Box<dyn Contest> {
    Box::new(ArrlDxContest::new())
}

impl ArrlDxContest {
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

#[derive(Clone, Debug)]
struct ArrlDxStation {
    callsign: String,
    exchange: String,
}

struct ArrlDxCallsignSource {
    stations: Vec<ArrlDxStation>,
    used: HashSet<String>,
}

impl ArrlDxCallsignSource {
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let stations: Vec<ArrlDxStation> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .filter(|line| !line.starts_with('#') && !line.starts_with('!'))
            .filter_map(|line| {
                let fields: Vec<&str> = line.split(',').collect();
                if fields.len() < 4 {
                    return None;
                }

                let callsign = fields.get(0).unwrap_or(&"").trim().to_uppercase();
                let state = fields.get(2).unwrap_or(&"").trim().to_uppercase();
                let power = fields.get(3).unwrap_or(&"").trim().to_uppercase();

                let has_state = !state.is_empty();
                let has_power = !power.is_empty();

                if (has_state && has_power) || (!has_state && !has_power) {
                    return None;
                }

                if callsign.is_empty() {
                    return None;
                }

                let exchange = if has_state { state } else { power };

                Some(ArrlDxStation { callsign, exchange })
            })
            .collect();

        if stations.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid ARRL DX callsigns found in file",
            ));
        }

        Ok(Self {
            stations,
            used: HashSet::new(),
        })
    }

    fn default_pool() -> Self {
        let stations = vec![
            ArrlDxStation {
                callsign: "VE2FK".to_string(),
                exchange: "QC".to_string(),
            },
            ArrlDxStation {
                callsign: "K3LR".to_string(),
                exchange: "PA".to_string(),
            },
            ArrlDxStation {
                callsign: "DL1ABC".to_string(),
                exchange: "100".to_string(),
            },
            ArrlDxStation {
                callsign: "JA1ABC".to_string(),
                exchange: "500".to_string(),
            },
        ];

        Self {
            stations,
            used: HashSet::new(),
        }
    }

    fn random_station(&mut self) -> Option<ArrlDxStation> {
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

impl CallsignSource for ArrlDxCallsignSource {
    fn random(
        &mut self,
        _contest: &dyn Contest,
        _serial: u32,
        _settings: &toml::Value,
    ) -> Option<(String, Exchange)> {
        let station = self.random_station()?;
        Some((
            station.callsign.clone(),
            Exchange::new(vec!["5NN".to_string(), station.exchange.clone()]),
        ))
    }
}

fn normalize_cw_digits(value: &str) -> String {
    value
        .trim()
        .to_uppercase()
        .chars()
        .map(|c| match c {
            'T' => '0',
            'N' => '9',
            _ => c,
        })
        .collect()
}

impl Contest for ArrlDxContest {
    fn id(&self) -> &'static str {
        CONTEST_ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn exchange_fields(&self) -> Vec<ExchangeField> {
        vec![
            ExchangeField::new("rst", "RST", "5NN", 3, FieldKind::Text).with_default_value("5NN"),
            ExchangeField::new("exchange", "Exchange", "ST/PWR", 6, FieldKind::Alnum)
                .focus_on_enter(),
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
                label: "Callsign File",
                placeholder: "arrldx_callsigns.txt",
                width_chars: 24,
                kind: SettingFieldKind::FilePath,
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "user_exchange",
                label: "Your Exchange",
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
            toml::Value::String("arrldx_callsigns.txt".to_string()),
        );
        table.insert(
            "user_exchange".to_string(),
            toml::Value::String("CT".to_string()),
        );
        toml::Value::Table(table)
    }

    fn cq_message(&self, settings: &toml::Value) -> String {
        Self::get_string(settings, "cq_message", "CQ TEST")
    }

    fn callsign_source(&self, settings: &toml::Value) -> Result<Box<dyn CallsignSource>, String> {
        let path = Self::get_string(settings, "callsign_file", "arrldx_callsigns.txt");
        match ArrlDxCallsignSource::load(&path) {
            Ok(source) => Ok(Box::new(source)),
            Err(_) => Ok(Box::new(ArrlDxCallsignSource::default_pool())),
        }
    }

    fn generate_exchange(&self, _callsign: &str, _serial: u32, settings: &toml::Value) -> Exchange {
        let exchange = Self::get_string(settings, "user_exchange", "CT");
        Exchange::new(vec!["5NN".to_string(), exchange])
    }

    fn user_exchange_fields(
        &self,
        _user_callsign: &str,
        _serial: u32,
        settings: &toml::Value,
    ) -> Vec<String> {
        let exchange = Self::get_string(settings, "user_exchange", "CT");
        vec!["5NN".to_string(), exchange]
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

        let rst_ok = match (expected_exchange.fields.get(0), received_fields.get(0)) {
            (Some(expected), Some(received)) => {
                normalize_cw_digits(expected) == normalize_cw_digits(received)
            }
            _ => false,
        };

        let exchange_ok = match (expected_exchange.fields.get(1), received_fields.get(1)) {
            (Some(expected), Some(received)) => {
                normalize_cw_digits(expected) == normalize_cw_digits(received)
            }
            _ => false,
        };

        let exchange_correct = rst_ok && exchange_ok;

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
