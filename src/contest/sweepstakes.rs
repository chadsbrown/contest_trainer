use rand::Rng;
use toml::value::Table;

use super::callsign::FileCallsignSource;
use super::types::{
    Contest, Exchange, ExchangeField, FieldKind, SettingField, SettingFieldGroup, SettingFieldKind,
    ValidationResult,
};

pub const CONTEST_ID: &str = "sweepstakes";
pub const DISPLAY_NAME: &str = "ARRL Sweepstakes";

const PRECEDENCES: &[char] = &['A', 'B', 'M', 'Q', 'S', 'U'];

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
            ExchangeField::new("serial", "NR", "001", 4, FieldKind::Number),
            ExchangeField::new("precedence", "Prec", "A", 1, FieldKind::Text),
            ExchangeField::new("check", "CK", "99", 2, FieldKind::Number),
            ExchangeField::new("section", "Sec", "CT", 3, FieldKind::Section),
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
            toml::Value::String("callsigns.txt".to_string()),
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
        let path = Self::get_string(settings, "callsign_file", "callsigns.txt");
        match FileCallsignSource::load(&path) {
            Ok(source) => Ok(Box::new(source)),
            Err(_) => Ok(Box::new(FileCallsignSource::default_pool())),
        }
    }

    fn generate_exchange(&self, callsign: &str, serial: u32, _settings: &toml::Value) -> Exchange {
        let mut rng = rand::thread_rng();
        let precedence = *PRECEDENCES
            .get(rng.gen_range(0..PRECEDENCES.len()))
            .unwrap_or(&'A');
        let check = rng.gen_range(60..=99) as u16;
        let section = Self::section_for_callsign(callsign);

        Exchange::new(vec![
            serial.to_string(),
            precedence.to_string(),
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
            serial.to_string(),
            precedence,
            user_callsign.to_string(),
            check,
            section,
        ]
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

        let exchange_correct = if received_fields.len() >= 4 && expected_exchange.fields.len() >= 4
        {
            let serial_ok = received_fields.get(0).and_then(|v| v.parse::<u32>().ok())
                == expected_exchange
                    .fields
                    .get(0)
                    .and_then(|v| v.parse::<u32>().ok());
            let prec_ok = received_fields
                .get(1)
                .and_then(|v| v.chars().next())
                .map(|c| c.to_ascii_uppercase().to_string())
                == expected_exchange.fields.get(1).map(|v| v.to_uppercase());
            let check_ok = received_fields.get(2).and_then(|v| v.parse::<u16>().ok())
                == expected_exchange
                    .fields
                    .get(2)
                    .and_then(|v| v.parse::<u16>().ok());
            let section_ok = received_fields.get(3).map(|v| v.to_uppercase())
                == expected_exchange.fields.get(3).map(|v| v.to_uppercase());
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
