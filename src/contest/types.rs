#[derive(Clone, Debug)]
pub struct Exchange {
    pub fields: Vec<String>,
}

impl Exchange {
    pub fn new(fields: Vec<String>) -> Self {
        Self { fields }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Number,
    Alnum,
    Section,
}

pub fn normalize_exchange_input(value: &str, kind: FieldKind) -> String {
    let mut cleaned = value.trim().to_uppercase();
    cleaned.retain(|c| !c.is_whitespace());
    if kind == FieldKind::Number {
        cleaned.retain(|c| c.is_ascii_digit());
    }
    cleaned
}

#[derive(Clone, Debug)]
pub struct ExchangeField {
    pub label: &'static str,
    pub placeholder: &'static str,
    pub width_chars: u8,
    pub kind: FieldKind,
    pub default_value: Option<&'static str>,
    pub focus_on_enter: bool,
}

impl ExchangeField {
    pub fn new(
        label: &'static str,
        placeholder: &'static str,
        width_chars: u8,
        kind: FieldKind,
    ) -> Self {
        Self {
            label,
            placeholder,
            width_chars,
            kind,
            default_value: None,
            focus_on_enter: false,
        }
    }

    pub fn with_default_value(mut self, value: &'static str) -> Self {
        self.default_value = Some(value);
        self
    }

    pub fn focus_on_enter(mut self) -> Self {
        self.focus_on_enter = true;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingFieldKind {
    Text,
    FilePath,
    Integer { min: i64, max: i64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingFieldGroup {
    Contest,
    UserExchange,
}

#[derive(Clone, Debug)]
pub struct SettingField {
    pub key: &'static str,
    pub label: &'static str,
    pub placeholder: &'static str,
    pub width_chars: u8,
    pub kind: SettingFieldKind,
    pub group: SettingFieldGroup,
}

/// Result of validating user's exchange against expected
#[derive(Clone, Debug)]
pub struct ValidationResult {
    pub callsign_correct: bool,
    pub exchange_correct: bool,
    pub points: u32,
}

/// Source of callsigns and exchanges for callers
pub trait CallsignSource: Send + Sync {
    fn random(
        &mut self,
        contest: &dyn Contest,
        serial: u32,
        settings: &toml::Value,
    ) -> Option<(String, Exchange)>;
}

/// Trait for contest-specific behavior
pub trait Contest: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;

    /// Exchange fields the user must log
    fn exchange_fields(&self) -> Vec<ExchangeField>;

    /// Contest settings schema (includes user exchange settings)
    fn settings_fields(&self) -> Vec<SettingField>;

    /// Default contest settings (stored as a TOML table)
    fn default_settings(&self) -> toml::Value;

    /// Validate contest settings (default is no-op)
    fn validate_settings(&self, _settings: &toml::Value) -> Result<(), String> {
        Ok(())
    }

    /// CQ message for this contest
    fn cq_message(&self, settings: &toml::Value) -> String;

    /// Create a callsign source for this contest
    fn callsign_source(&self, settings: &toml::Value) -> Result<Box<dyn CallsignSource>, String>;

    /// Generate an exchange for a calling station
    fn generate_exchange(&self, callsign: &str, serial: u32, settings: &toml::Value) -> Exchange;

    /// Format exchange for Morse transmission
    fn format_exchange(&self, exchange: &Exchange) -> String {
        exchange.fields.join(" ")
    }

    /// Get the user's exchange to send (as fields)
    fn user_exchange_fields(
        &self,
        user_callsign: &str,
        serial: u32,
        settings: &toml::Value,
    ) -> Vec<String>;

    /// Format the user's exchange for Morse transmission
    fn format_user_exchange(&self, fields: &[String]) -> String {
        fields.join(" ")
    }

    /// Validate user's logged exchange against expected
    fn validate(
        &self,
        expected_call: &str,
        expected_exchange: &Exchange,
        received_call: &str,
        received_fields: &[String],
        settings: &toml::Value,
    ) -> ValidationResult;

    /// Format received exchange for display/logging
    fn format_received_exchange(&self, fields: &[String]) -> String {
        fields.join(" ")
    }
}

pub struct ContestDescriptor {
    pub id: &'static str,
    pub display_name: &'static str,
    pub factory: fn() -> Box<dyn Contest>,
}
