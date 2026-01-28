use toml::value::Table;

use super::callsign::FileCallsignSource;
use super::types::{
    Contest, Exchange, ExchangeField, FieldKind, SettingField, SettingFieldGroup, SettingFieldKind,
    ValidationResult,
};
use crate::cty::CtyDat;

pub const CONTEST_ID: &str = "cqww";
pub const DISPLAY_NAME: &str = "CQ World Wide";

pub struct CqWwContest {
    cty: CtyDat,
}

pub fn make_contest() -> Box<dyn Contest> {
    Box::new(CqWwContest::new())
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

    fn get_string(settings: &toml::Value, key: &str, default: &str) -> String {
        settings
            .get(key)
            .and_then(|v| v.as_str())
            .unwrap_or(default)
            .to_string()
    }
}

impl Contest for CqWwContest {
    fn id(&self) -> &'static str {
        CONTEST_ID
    }

    fn display_name(&self) -> &'static str {
        DISPLAY_NAME
    }

    fn exchange_fields(&self) -> Vec<ExchangeField> {
        vec![
            ExchangeField::new("rst", "RST", "5NN", 3, FieldKind::Text).with_default_value("5NN"),
            ExchangeField::new("zone", "Zone", "05", 2, FieldKind::Number).focus_on_enter(),
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
                key: "user_zone",
                label: "Your Zone",
                placeholder: "05",
                width_chars: 2,
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
            "user_zone".to_string(),
            toml::Value::String("05".to_string()),
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

    fn generate_exchange(&self, callsign: &str, _serial: u32, _settings: &toml::Value) -> Exchange {
        let zone = self.zone_for_callsign(callsign);
        Exchange::new(vec!["5NN".to_string(), format!("{:02}", zone)])
    }

    fn user_exchange_fields(
        &self,
        _user_callsign: &str,
        _serial: u32,
        settings: &toml::Value,
    ) -> Vec<String> {
        let zone_str = Self::get_string(settings, "user_zone", "05");
        let zone = zone_str.parse::<u8>().unwrap_or(5);
        vec!["5NN".to_string(), format!("{:02}", zone)]
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

        let expected_rst = expected_exchange.fields.get(0);
        let expected_zone = expected_exchange
            .fields
            .get(1)
            .and_then(|z| z.parse::<u8>().ok());

        let received_rst = received_fields.get(0);
        let received_zone = received_fields.get(1).and_then(|z| z.parse::<u8>().ok());

        let rst_ok = match (expected_rst, received_rst) {
            (Some(expected), Some(received)) => expected.eq_ignore_ascii_case(received),
            _ => false,
        };

        let zone_ok = match (expected_zone, received_zone) {
            (Some(expected), Some(received)) => expected == received,
            _ => false,
        };

        let exchange_correct = rst_ok && zone_ok;

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
