use rand::seq::SliceRandom;
use rand::Rng;
use std::collections::HashSet;
use std::path::Path;
use toml::value::Table;

use super::types::{
    Contest, Exchange, ExchangeField, FieldKind, SettingField, SettingFieldGroup, SettingFieldKind,
    ValidationResult,
};

pub const CONTEST_ID: &str = "sweepstakes";
pub const DISPLAY_NAME: &str = "ARRL Sweepstakes";

const PRECEDENCES: &[char] = &['Q', 'A', 'B', 'U', 'M', 'S'];
const SERIAL_MIN_DEFAULT: i64 = 100;
const SERIAL_MAX_DEFAULT: i64 = 400;
const SERIAL_MIN_ALLOWED: i64 = 1;
const SERIAL_MAX_ALLOWED: i64 = 12000;

pub struct SweepstakesContest;

pub fn make_contest() -> Box<dyn Contest> {
    Box::new(SweepstakesContest::new())
}

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

    fn get_string(settings: &toml::Value, key: &str, default: &str) -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }

    fn parse_integer(settings: &toml::Value, key: &str) -> Option<i64> {
        settings.get(key).and_then(|v| v.as_integer()).or_else(|| {
            settings
                .get(key)
                .and_then(|v| v.as_str())
                .and_then(|s| s.trim().parse::<i64>().ok())
        })
    }

    fn serial_range(settings: &toml::Value) -> (u32, u32) {
        let min = Self::parse_integer(settings, "serial_min").unwrap_or(SERIAL_MIN_DEFAULT);
        let max = Self::parse_integer(settings, "serial_max").unwrap_or(SERIAL_MAX_DEFAULT);
        let min = min.clamp(SERIAL_MIN_ALLOWED, SERIAL_MAX_ALLOWED);
        let max = max.clamp(SERIAL_MIN_ALLOWED, SERIAL_MAX_ALLOWED);
        let (min, max) = if min <= max { (min, max) } else { (max, min) };
        (min as u32, max as u32)
    }

    fn format_serial(serial: u32) -> String {
        if serial < 100 {
            format!("{:03}", serial)
        } else {
            serial.to_string()
        }
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

fn parse_serial(value: &str) -> Option<u32> {
    let normalized = normalize_cw_digits(value);
    if normalized.is_empty() || !normalized.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    normalized.parse::<u32>().ok()
}

#[derive(Clone, Debug)]
struct SweepstakesStation {
    callsign: String,
    section: String,
    check: String,
}

struct SweepstakesCallsignSource {
    stations: Vec<SweepstakesStation>,
    used: HashSet<String>,
}

impl SweepstakesCallsignSource {
    fn load<P: AsRef<Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let stations: Vec<SweepstakesStation> = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
            .filter_map(|line| {
                let fields: Vec<&str> = line.split(',').map(|f| f.trim()).collect();
                if fields.len() < 4 {
                    return None;
                }

                let callsign = fields.get(0).unwrap_or(&"").to_uppercase();
                let section = fields.get(1).unwrap_or(&"").to_uppercase();
                let check = fields.get(3).unwrap_or(&"").to_uppercase();

                if callsign.is_empty() || section.is_empty() || check.is_empty() {
                    return None;
                }

                if !check.chars().all(|c| c.is_ascii_digit()) {
                    return None;
                }

                if !is_valid_callsign(&callsign) {
                    return None;
                }

                Some(SweepstakesStation {
                    callsign,
                    section,
                    check,
                })
            })
            .collect();

        if stations.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "No valid Sweepstakes stations found in file",
            ));
        }

        Ok(Self {
            stations,
            used: HashSet::new(),
        })
    }

    fn default_pool() -> Self {
        let stations = vec![
            SweepstakesStation {
                callsign: "W1AW".to_string(),
                section: "CT".to_string(),
                check: "38".to_string(),
            },
            SweepstakesStation {
                callsign: "K5ZD".to_string(),
                section: "EMA".to_string(),
                check: "90".to_string(),
            },
            SweepstakesStation {
                callsign: "N0AX".to_string(),
                section: "WCF".to_string(),
                check: "72".to_string(),
            },
            SweepstakesStation {
                callsign: "K3LR".to_string(),
                section: "WPA".to_string(),
                check: "79".to_string(),
            },
        ];

        Self {
            stations,
            used: HashSet::new(),
        }
    }

    fn random_station(&mut self) -> Option<SweepstakesStation> {
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

impl super::types::CallsignSource for SweepstakesCallsignSource {
    fn random(
        &mut self,
        _contest: &dyn Contest,
        _serial: u32,
        settings: &toml::Value,
    ) -> Option<(String, Exchange)> {
        let station = self.random_station()?;
        let (min, max) = SweepstakesContest::serial_range(settings);
        let serial = rand::thread_rng().gen_range(min..=max);
        let precedence = *PRECEDENCES.choose(&mut rand::thread_rng()).unwrap_or(&'A');
        let check = station.check.parse::<u16>().ok()?;

        Some((
            station.callsign.clone(),
            Exchange::new(vec![
                SweepstakesContest::format_serial(serial),
                precedence.to_string(),
                station.callsign.clone(),
                format!("{:02}", check),
                station.section.clone(),
            ]),
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

impl Contest for SweepstakesContest {
    fn id(&self) -> &'static str {
        CONTEST_ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn exchange_fields(&self) -> Vec<ExchangeField> {
        vec![
            ExchangeField::new("NR", "001", 4, FieldKind::Number),
            ExchangeField::new("Prec", "A", 1, FieldKind::Text),
            ExchangeField::new("CK", "99", 2, FieldKind::Number),
            ExchangeField::new("Sec", "CT", 3, FieldKind::Section),
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
                label: "Sweepstakes Callsign File",
                placeholder: "ss_callsigns.txt",
                width_chars: 24,
                kind: SettingFieldKind::FilePath,
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "serial_min",
                label: "Serial Min",
                placeholder: "100",
                width_chars: 5,
                kind: SettingFieldKind::Integer {
                    min: SERIAL_MIN_ALLOWED,
                    max: SERIAL_MAX_ALLOWED,
                },
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "serial_max",
                label: "Serial Max",
                placeholder: "400",
                width_chars: 5,
                kind: SettingFieldKind::Integer {
                    min: SERIAL_MIN_ALLOWED,
                    max: SERIAL_MAX_ALLOWED,
                },
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "user_precedence",
                label: "Your Precedence",
                placeholder: "A",
                width_chars: 1,
                kind: SettingFieldKind::Text,
                group: SettingFieldGroup::UserExchange,
            },
            SettingField {
                key: "user_check",
                label: "Your Check",
                placeholder: "99",
                width_chars: 2,
                kind: SettingFieldKind::Text,
                group: SettingFieldGroup::UserExchange,
            },
            SettingField {
                key: "user_section",
                label: "Your Section",
                placeholder: "CT",
                width_chars: 3,
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
            toml::Value::String("ss_callsigns.txt".to_string()),
        );
        table.insert(
            "serial_min".to_string(),
            toml::Value::Integer(SERIAL_MIN_DEFAULT),
        );
        table.insert(
            "serial_max".to_string(),
            toml::Value::Integer(SERIAL_MAX_DEFAULT),
        );
        table.insert(
            "user_precedence".to_string(),
            toml::Value::String("A".to_string()),
        );
        table.insert(
            "user_check".to_string(),
            toml::Value::String("99".to_string()),
        );
        table.insert(
            "user_section".to_string(),
            toml::Value::String("CT".to_string()),
        );
        toml::Value::Table(table)
    }

    fn cq_message(&self, settings: &toml::Value) -> String {
        Self::get_string(settings, "cq_message", "CQ TEST")
    }

    fn callsign_source(
        &self,
        settings: &toml::Value,
    ) -> Result<Box<dyn super::types::CallsignSource>, String> {
        let path = Self::get_string(settings, "callsign_file", "ss_callsigns.txt");
        match SweepstakesCallsignSource::load(&path) {
            Ok(source) => Ok(Box::new(source)),
            Err(_) => Ok(Box::new(SweepstakesCallsignSource::default_pool())),
        }
    }

    fn generate_exchange(&self, callsign: &str, _serial: u32, settings: &toml::Value) -> Exchange {
        let mut rng = rand::thread_rng();
        let precedence = *PRECEDENCES
            .get(rng.gen_range(0..PRECEDENCES.len()))
            .unwrap_or(&'A');
        let (min, max) = Self::serial_range(settings);
        let serial = rng.gen_range(min..=max);
        let check = rng.gen_range(60..=99) as u16;
        let section = Self::section_for_callsign(callsign);

        Exchange::new(vec![
            Self::format_serial(serial),
            precedence.to_string(),
            callsign.to_string(),
            format!("{:02}", check),
            section,
        ])
    }

    fn user_exchange_fields(
        &self,
        user_callsign: &str,
        serial: u32,
        settings: &toml::Value,
    ) -> Vec<String> {
        let precedence = Self::get_string(settings, "user_precedence", "A");
        let check = Self::get_string(settings, "user_check", "99");
        let section = Self::get_string(settings, "user_section", "CT");

        vec![
            Self::format_serial(serial),
            precedence,
            user_callsign.to_string(),
            check,
            section,
        ]
    }

    fn validate_settings(&self, settings: &toml::Value) -> Result<(), String> {
        let min = Self::parse_integer(settings, "serial_min")
            .ok_or_else(|| "Serial Min must be an integer between 1 and 12000.".to_string())?;
        let max = Self::parse_integer(settings, "serial_max")
            .ok_or_else(|| "Serial Max must be an integer between 1 and 12000.".to_string())?;

        if !(SERIAL_MIN_ALLOWED..=SERIAL_MAX_ALLOWED).contains(&min) {
            return Err("Serial Min must be between 1 and 12000.".to_string());
        }
        if !(SERIAL_MIN_ALLOWED..=SERIAL_MAX_ALLOWED).contains(&max) {
            return Err("Serial Max must be between 1 and 12000.".to_string());
        }
        if min > max {
            return Err("Serial Min must be less than or equal to Serial Max.".to_string());
        }

        Ok(())
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

        let exchange_correct = if received_fields.len() >= 4 && expected_exchange.fields.len() >= 5
        {
            let serial_ok = match (expected_exchange.fields.get(0), received_fields.get(0)) {
                (Some(expected), Some(received)) => {
                    parse_serial(expected) == parse_serial(received)
                }
                _ => false,
            };
            let prec_ok = received_fields
                .get(1)
                .and_then(|v| v.chars().next())
                .map(|c| c.to_ascii_uppercase().to_string())
                == expected_exchange.fields.get(1).map(|v| v.to_uppercase());
            let check_ok = received_fields.get(2).and_then(|v| v.parse::<u16>().ok())
                == expected_exchange
                    .fields
                    .get(3)
                    .and_then(|v| v.parse::<u16>().ok());
            let section_ok = received_fields.get(3).map(|v| v.to_uppercase())
                == expected_exchange.fields.get(4).map(|v| v.to_uppercase());
            serial_ok && prec_ok && check_ok && section_ok
        } else {
            false
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
