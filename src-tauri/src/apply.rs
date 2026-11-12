use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldAction {
    Keep,
    Remove,
    Set,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldRule {
    pub key: String,
    pub action: FieldAction,
    #[serde(default)]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApplyOptions {
    pub output_mode: crate::OutputMode,
    pub output_dir: Option<String>,
    #[serde(default)]
    pub rules: Vec<FieldRule>,
    #[serde(default)]
    pub clear_all: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedAction<'a> {
    Keep,
    Remove,
    Set(&'a str),
}

pub fn resolve_action<'a>(key: &str, rules: &'a [FieldRule]) -> ResolvedAction<'a> {
    for rule in rules {
        if rule_matches(&rule.key, key) {
            return match rule.action {
                FieldAction::Keep => ResolvedAction::Keep,
                FieldAction::Remove => ResolvedAction::Remove,
                FieldAction::Set => match rule.value.as_deref() {
                    Some(value) if !value.trim().is_empty() => ResolvedAction::Set(value.trim()),
                    _ => ResolvedAction::Remove,
                },
            };
        }
    }
    ResolvedAction::Keep
}

fn rule_matches(rule_key: &str, field_key: &str) -> bool {
    if rule_key == field_key {
        return true;
    }
    if let Some(prefix) = rule_key.strip_suffix(":*") {
        return field_key.starts_with(prefix);
    }
    false
}

pub fn should_clear_all(_rules: &[FieldRule], clear_all: bool) -> bool {
    clear_all
}
