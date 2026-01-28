pub mod callsign;
pub mod types;

#[allow(unused_imports)]
pub use callsign::{CallsignPool, FileCallsignSource};
#[allow(unused_imports)]
pub use types::{
    normalize_exchange_input, CallsignSource, Contest, ContestDescriptor, Exchange, ExchangeField,
    FieldKind, SettingField, SettingFieldGroup, SettingFieldKind, ValidationResult,
};

include!(concat!(env!("OUT_DIR"), "/contest_registry.rs"));

pub fn registry() -> Vec<ContestDescriptor> {
    generated_contest_registry()
}

pub fn create_contest(id: &str) -> Option<Box<dyn Contest>> {
    registry()
        .into_iter()
        .find(|entry| entry.id == id)
        .map(|entry| (entry.factory)())
}

pub fn default_contest_id() -> Option<&'static str> {
    registry().first().map(|entry| entry.id)
}
