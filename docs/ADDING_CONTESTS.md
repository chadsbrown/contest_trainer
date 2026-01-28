# Adding Contests

Each contest lives in a single file under `src/contest/<id>.rs`. The build script
auto-registers every `.rs` file in `src/contest/` except:

- `mod.rs`
- `types.rs`
- `callsign.rs`

If the file exists and compiles, it is included at build time.

## Required Items in a Contest File

Your contest file must define:

```rust
pub const CONTEST_ID: &str = "<id>";       // must match the filename
pub const DISPLAY_NAME: &str = "Nice Name";
pub fn make_contest() -> Box<dyn Contest>;
```

And a type that implements `Contest`.

## Contest Trait

Your contest struct must implement the following trait from `src/contest/types.rs`:

```rust
pub trait Contest: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn exchange_fields(&self) -> Vec<ExchangeField>;
    fn settings_fields(&self) -> Vec<SettingField>;
    fn default_settings(&self) -> toml::Value;
    fn validate_settings(&self, _settings: &toml::Value) -> Result<(), String>;
    fn cq_message(&self, settings: &toml::Value) -> String;
    fn callsign_source(&self, settings: &toml::Value)
        -> Result<Box<dyn CallsignSource>, String>;
    fn generate_exchange(
        &self,
        callsign: &str,
        serial: u32,
        settings: &toml::Value,
    ) -> Exchange;
    fn user_exchange_fields(
        &self,
        user_callsign: &str,
        serial: u32,
        settings: &toml::Value,
    ) -> Vec<String>;
    fn validate(
        &self,
        expected_call: &str,
        expected_exchange: &Exchange,
        received_call: &str,
        received_fields: &[String],
        settings: &toml::Value,
    ) -> ValidationResult;
}
```

Defaults exist for `format_exchange`, `format_user_exchange`, and
`format_received_exchange`, which all join fields with spaces.

## Exchange Fields (User Entry)

`exchange_fields()` defines the fields the user logs on the main screen. Each
field has:

- `label` and `placeholder`
- `width_chars` (drives the input width)
- `kind` (Text, Number, Alnum, Section)
- `optional` (reserved for future use)
- `default_value` (auto-populated but editable, e.g. `5NN` in CQWW)
- `focus_on_enter` (first field marked true receives focus after a callsign is entered)

The UI renders one row of labeled fields. Space and Tab move to the next field.

## Contest Settings Fields

`settings_fields()` defines the schema for settings shown in the Settings UI.
Use the `group` to choose where the field appears:

- `SettingFieldGroup::Contest`
- `SettingFieldGroup::UserExchange`

All settings are stored under `contest.contests.<id>` in `settings.toml`. The
default values come from `default_settings()`.

## Callsign Source and Parsing

Each contest owns its callsign parsing:

- For a simple “one callsign per line” file, use
  `contest::callsign::FileCallsignSource`.
- For custom formats (e.g., CWT’s callsign, name, number CSV), implement your
  own `CallsignSource` in the contest file.

`callsign_source()` should return a usable source even if the file is missing
or invalid (e.g., by falling back to a small default pool).

### Example: Custom Callsign Parser

Below is a minimal custom parser that reads `callsign,name,number` CSV lines
and implements `CallsignSource`:

```rust
struct MyCallsignSource {
    stations: Vec<(String, String, String)>,
    used: std::collections::HashSet<String>,
}

impl MyCallsignSource {
    fn load<P: AsRef<std::path::Path>>(path: P) -> Result<Self, std::io::Error> {
        let content = std::fs::read_to_string(path)?;
        let stations = content
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                let fields: Vec<&str> = line.split(',').map(|f| f.trim()).collect();
                if fields.len() >= 3 {
                    let callsign = fields[0].to_uppercase();
                    let name = fields[1].to_uppercase();
                    let number = fields[2].to_uppercase();
                    if !callsign.is_empty() && !name.is_empty() && !number.is_empty() {
                        return Some((callsign, name, number));
                    }
                }
                None
            })
            .collect();
        Ok(Self {
            stations,
            used: std::collections::HashSet::new(),
        })
    }
}

impl CallsignSource for MyCallsignSource {
    fn random(
        &mut self,
        _contest: &dyn Contest,
        _serial: u32,
        _settings: &toml::Value,
    ) -> Option<(String, Exchange)> {
        let station = self.stations.pop()?;
        Some((
            station.0.clone(),
            Exchange::new(vec![station.1.clone(), station.2.clone()]),
        ))
    }
}
```

## Config Behavior

If an existing `settings.toml` is incompatible with the current schema, the
app renames it to `settings.toml.bak.<timestamp>` and writes defaults.

## Minimal Example

```rust
use super::types::*;

pub const CONTEST_ID: &str = "example";
pub const DISPLAY_NAME: &str = "Example Contest";

pub struct ExampleContest;

pub fn make_contest() -> Box<dyn Contest> {
    Box::new(ExampleContest)
}

impl Contest for ExampleContest {
    fn id(&self) -> &'static str { CONTEST_ID }
    fn display_name(&self) -> &'static str { DISPLAY_NAME }
    fn exchange_fields(&self) -> Vec<ExchangeField> {
        vec![ExchangeField::new("zone", "Zone", "05", 2, FieldKind::Number)]
    }
    fn settings_fields(&self) -> Vec<SettingField> { Vec::new() }
    fn default_settings(&self) -> toml::Value { toml::Value::Table(Default::default()) }
    fn cq_message(&self, _settings: &toml::Value) -> String { "CQ TEST".into() }
    fn callsign_source(
        &self,
        _settings: &toml::Value,
    ) -> Result<Box<dyn CallsignSource>, String> {
        Err("provide a callsign source".into())
    }
    fn generate_exchange(
        &self,
        _callsign: &str,
        _serial: u32,
        _settings: &toml::Value,
    ) -> Exchange {
        Exchange::new(vec!["05".into()])
    }
    fn user_exchange_fields(
        &self,
        _user_callsign: &str,
        _serial: u32,
        _settings: &toml::Value,
    ) -> Vec<String> {
        vec!["5NN".into(), "05".into()]
    }
    fn validate(
        &self,
        expected_call: &str,
        expected_exchange: &Exchange,
        received_call: &str,
        received_fields: &[String],
        _settings: &toml::Value,
    ) -> ValidationResult {
        ValidationResult {
            callsign_correct: expected_call.eq_ignore_ascii_case(received_call),
            exchange_correct: expected_exchange.fields == *received_fields,
            points: 1,
        }
    }
}
```
