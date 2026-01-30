use rand::Rng;
use toml::value::Table;

use super::callsign::FileCallsignSource;
use super::types::{
    CallsignSource, Contest, Exchange, ExchangeField, FieldKind, SettingField, SettingFieldGroup,
    SettingFieldKind, ValidationResult,
};

pub const CONTEST_ID: &str = "cqwpx";
pub const DISPLAY_NAME: &str = "CQ WPX";

const SERIAL_MIN_DEFAULT: i64 = 1000;
const SERIAL_MAX_DEFAULT: i64 = 2500;
const SERIAL_MIN_ALLOWED: i64 = 1;
const SERIAL_MAX_ALLOWED: i64 = 12000;

pub struct CqWpxContest;

pub fn make_contest() -> Box<dyn Contest> {
    Box::new(CqWpxContest::new())
}

impl CqWpxContest {
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

    fn parse_integer(settings: &toml::Value, key: &str) -> Option<i64> {
        settings
            .get(key)
            .and_then(|v| v.as_integer())
            .or_else(|| {
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

impl Contest for CqWpxContest {
    fn id(&self) -> &'static str {
        CONTEST_ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn exchange_fields(&self) -> Vec<ExchangeField> {
        vec![
            ExchangeField::new("RST", "5NN", 3, FieldKind::Text).with_default_value("5NN"),
            ExchangeField::new("SER", "SER", 5, FieldKind::Alnum).focus_on_enter(),
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
                placeholder: "callsigns.txt",
                width_chars: 24,
                kind: SettingFieldKind::FilePath,
                group: SettingFieldGroup::Contest,
            },
            SettingField {
                key: "serial_min",
                label: "Serial Min",
                placeholder: "1000",
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
                placeholder: "2500",
                width_chars: 5,
                kind: SettingFieldKind::Integer {
                    min: SERIAL_MIN_ALLOWED,
                    max: SERIAL_MAX_ALLOWED,
                },
                group: SettingFieldGroup::Contest,
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
            toml::Value::String("callsigns.txt".to_string()),
        );
        table.insert(
            "serial_min".to_string(),
            toml::Value::Integer(SERIAL_MIN_DEFAULT),
        );
        table.insert(
            "serial_max".to_string(),
            toml::Value::Integer(SERIAL_MAX_DEFAULT),
        );
        toml::Value::Table(table)
    }

    fn validate_settings(&self, settings: &toml::Value) -> Result<(), String> {
        let min = Self::parse_integer(settings, "serial_min").ok_or_else(|| {
            "Serial Min must be an integer between 1 and 12000.".to_string()
        })?;
        let max = Self::parse_integer(settings, "serial_max").ok_or_else(|| {
            "Serial Max must be an integer between 1 and 12000.".to_string()
        })?;

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

    fn cq_message(&self, settings: &toml::Value) -> String {
        Self::get_string(settings, "cq_message", "CQ TEST")
    }

    fn callsign_source(&self, settings: &toml::Value) -> Result<Box<dyn CallsignSource>, String> {
        let path = Self::get_string(settings, "callsign_file", "callsigns.txt");
        match FileCallsignSource::load(&path) {
            Ok(source) => Ok(Box::new(source)),
            Err(_) => Ok(Box::new(FileCallsignSource::default_pool())),
        }
    }

    fn generate_exchange(&self, _callsign: &str, _serial: u32, settings: &toml::Value) -> Exchange {
        let (min, max) = Self::serial_range(settings);
        let serial = rand::thread_rng().gen_range(min..=max);
        Exchange::new(vec!["5NN".to_string(), Self::format_serial(serial)])
    }

    fn user_exchange_fields(
        &self,
        _user_callsign: &str,
        serial: u32,
        _settings: &toml::Value,
    ) -> Vec<String> {
        vec!["5NN".to_string(), Self::format_serial(serial)]
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

        let serial_ok = match (expected_exchange.fields.get(1), received_fields.get(1)) {
            (Some(expected), Some(received)) => parse_serial(expected) == parse_serial(received),
            _ => false,
        };

        let exchange_correct = rst_ok && serial_ok;

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
