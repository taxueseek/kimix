//! Coerce imperfect OSS tool-call arguments into schema-shaped JSON.
//!
//! Open-source models often emit valid *intent* with invalid JSON shape
//! (stringified objects, single-element arrays, stringified numbers, field
//! aliases). [`repair_tool_input`] runs a bounded, schema-guided repair
//! catalogue **before** `serde_json::from_value` so tools fail less often
//! without loosening typed `Args` structs.
//!
//! Rules are intentionally small and pure — no network, no tool registry.
//! Unknown / hostile payloads stay unchanged; serde still owns final validation.

use serde_json::{Map, Number, Value};

/// One successful coercion applied during repair.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepairEvent {
    /// Stable rule id for logs / tests (snake_case).
    pub rule: &'static str,
    /// JSON-pointer-ish path (`""` = root, `".timeout"`, `".items[0]"`).
    pub path: String,
}

/// Result of running the repair catalogue.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepairReport {
    pub events: Vec<RepairEvent>,
}

impl RepairReport {
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn push(&mut self, rule: &'static str, path: &str) {
        self.events.push(RepairEvent {
            rule,
            path: path.to_owned(),
        });
    }
}

/// Common OSS field aliases → canonical names used by kimix tool Args.
/// Applied only when the target key is absent; with a schema, only when the
/// target is a declared property.
const FIELD_ALIASES: &[(&str, &str)] = &[
    ("file", "path"),
    ("filepath", "path"),
    ("file_path", "path"),
    ("filename", "path"),
    ("dir", "path"),
    ("directory", "path"),
    ("cmd", "command"),
    ("command_line", "command"),
    ("shell_command", "command"),
    ("timeout_ms", "timeout"),
    ("timeout_secs", "timeout"),
    ("timeout_sec", "timeout"),
    ("working_directory", "cwd"),
    ("workdir", "cwd"),
    ("work_dir", "cwd"),
];

/// Repair `value` toward `schema` (JSON Schema object, optional).
///
/// When `schema` is `None`, only root-level heuristics run (parse stringified
/// JSON, unwrap single-element object array, apply global aliases).
pub fn repair_tool_input(value: Value, schema: Option<&Value>) -> (Value, RepairReport) {
    let mut report = RepairReport::default();
    let mut value = coerce_root(value, schema, &mut report);

    if let Some(schema) = schema {
        value = repair_value(value, schema, "", &mut report, 0);
    } else if let Value::Object(map) = value {
        value = Value::Object(apply_aliases(map, None, "", &mut report));
    }

    (value, report)
}

/// Repair in place; returns the report of applied rules.
pub fn repair_tool_input_in_place(value: &mut Value, schema: Option<&Value>) -> RepairReport {
    let owned = std::mem::replace(value, Value::Null);
    let (repaired, report) = repair_tool_input(owned, schema);
    *value = repaired;
    report
}

const MAX_DEPTH: u8 = 12;

fn coerce_root(value: Value, schema: Option<&Value>, report: &mut RepairReport) -> Value {
    let mut value = value;

    // Stringified JSON root: `"{\"path\":\"a\"}"` → object
    if let Value::String(s) = &value {
        let trimmed = s.trim();
        if (trimmed.starts_with('{') || trimmed.starts_with('['))
            && let Ok(parsed) = serde_json::from_str::<Value>(trimmed)
        {
            report.push("parse_stringified_json", "");
            value = parsed;
        }
    }

    // Single-element array root when schema wants object: `[{...}]` → `{...}`
    if let Value::Array(arr) = &value
        && arr.len() == 1
        && arr[0].is_object()
        && schema_is_object(schema)
    {
        report.push("unwrap_singleton_array", "");
        value = arr[0].clone();
    }

    value
}

fn schema_is_object(schema: Option<&Value>) -> bool {
    match schema {
        None => true, // OSS tools almost always take object args
        Some(s) => {
            primary_type(s) == Some("object")
                || s.get("properties").is_some()
                || s.get("$ref").is_some()
        }
    }
}

fn repair_value(
    value: Value,
    schema: &Value,
    path: &str,
    report: &mut RepairReport,
    depth: u8,
) -> Value {
    if depth >= MAX_DEPTH {
        return value;
    }

    let expected = primary_type(schema);

    let mut value = match (&value, expected) {
        (Value::String(s), Some(t)) if t == "object" || t == "array" => {
            let trimmed = s.trim();
            if (trimmed.starts_with('{') || trimmed.starts_with('['))
                && let Ok(parsed) = serde_json::from_str::<Value>(trimmed)
            {
                report.push("parse_stringified_json", path);
                parsed
            } else {
                value
            }
        }
        (Value::Array(arr), Some(t)) if t != "array" && arr.len() == 1 => {
            report.push("unwrap_singleton_array", path);
            arr[0].clone()
        }
        (Value::String(s), Some("number" | "integer")) => {
            if let Some(n) = parse_number(s) {
                report.push("string_to_number", path);
                Value::Number(n)
            } else {
                value
            }
        }
        (Value::String(s), Some("boolean")) => match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => {
                report.push("string_to_bool", path);
                Value::Bool(true)
            }
            "false" | "0" | "no" => {
                report.push("string_to_bool", path);
                Value::Bool(false)
            }
            _ => value,
        },
        (Value::Number(n), Some("string")) => {
            report.push("number_to_string", path);
            Value::String(n.to_string())
        }
        (Value::Bool(b), Some("string")) => {
            report.push("bool_to_string", path);
            Value::String(b.to_string())
        }
        _ => value,
    };

    if matches!(value, Value::Null) {
        return value;
    }

    if let Value::Object(map) = value {
        let props = schema.get("properties").and_then(Value::as_object);
        let mut map = apply_aliases(map, props, path, report);

        if let Some(props) = props {
            for (key, prop_schema) in props {
                if let Some(child) = map.remove(key) {
                    let child_path = format!("{path}.{key}");
                    let repaired =
                        repair_value(child, prop_schema, &child_path, report, depth + 1);
                    map.insert(key.clone(), repaired);
                }
            }
        }
        value = Value::Object(map);
    } else if let Value::Array(items) = value {
        if let Some(item_schema) = schema.get("items") {
            let item_schema = match item_schema {
                Value::Object(_) => item_schema,
                Value::Array(arr) => arr.first().unwrap_or(item_schema),
                _ => item_schema,
            };
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                let child_path = format!("{path}[{i}]");
                out.push(repair_value(
                    item,
                    item_schema,
                    &child_path,
                    report,
                    depth + 1,
                ));
            }
            value = Value::Array(out);
        } else {
            value = Value::Array(items);
        }
    }

    value
}

fn apply_aliases(
    mut map: Map<String, Value>,
    props: Option<&Map<String, Value>>,
    path: &str,
    report: &mut RepairReport,
) -> Map<String, Value> {
    for &(from, to) in FIELD_ALIASES {
        if map.contains_key(to) {
            continue;
        }
        if !map.contains_key(from) {
            continue;
        }
        if let Some(props) = props
            && !props.contains_key(to)
        {
            continue;
        }
        if let Some(v) = map.remove(from) {
            report.push("rename_alias", &format!("{path}.{from}->{to}"));
            map.insert(to.to_owned(), v);
        }
    }
    map
}

fn primary_type(schema: &Value) -> Option<&str> {
    match schema.get("type") {
        Some(Value::String(s)) => Some(s.as_str()),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null"),
        _ => {
            for key in ["anyOf", "oneOf"] {
                if let Some(branches) = schema.get(key).and_then(Value::as_array) {
                    for b in branches {
                        if let Some(t) = primary_type(b)
                            && t != "null"
                        {
                            return Some(t);
                        }
                    }
                }
            }
            if schema.get("properties").is_some() {
                return Some("object");
            }
            None
        }
    }
}

fn parse_number(s: &str) -> Option<Number> {
    let t = s.trim();
    if let Ok(i) = t.parse::<i64>() {
        return Some(Number::from(i));
    }
    if let Ok(u) = t.parse::<u64>() {
        return Some(Number::from(u));
    }
    t.parse::<f64>().ok().and_then(Number::from_f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parse_stringified_root_object() {
        let raw = Value::String(r#"{"path":"/tmp/a","cmd":"ls"}"#.into());
        let schema = json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "command": { "type": "string" }
            }
        });
        let (out, report) = repair_tool_input(raw, Some(&schema));
        assert_eq!(out["path"], "/tmp/a");
        assert_eq!(out["command"], "ls");
        assert!(
            report
                .events
                .iter()
                .any(|e| e.rule == "parse_stringified_json")
        );
        assert!(report.events.iter().any(|e| e.rule == "rename_alias"));
    }

    #[test]
    fn unwrap_singleton_array_root() {
        let raw = json!([{ "text": "hi" }]);
        let schema = json!({
            "type": "object",
            "properties": { "text": { "type": "string" } }
        });
        let (out, report) = repair_tool_input(raw, Some(&schema));
        assert_eq!(out, json!({ "text": "hi" }));
        assert!(
            report
                .events
                .iter()
                .any(|e| e.rule == "unwrap_singleton_array")
        );
    }

    #[test]
    fn coerce_string_number_and_bool() {
        let raw = json!({ "timeout": "30", "enabled": "true" });
        let schema = json!({
            "type": "object",
            "properties": {
                "timeout": { "type": "integer" },
                "enabled": { "type": "boolean" }
            }
        });
        let (out, report) = repair_tool_input(raw, Some(&schema));
        assert_eq!(out["timeout"], 30);
        assert_eq!(out["enabled"], true);
        assert!(report.events.iter().any(|e| e.rule == "string_to_number"));
        assert!(report.events.iter().any(|e| e.rule == "string_to_bool"));
    }

    #[test]
    fn unwrap_singleton_property_array() {
        let raw = json!({ "path": ["/a/b"] });
        let schema = json!({
            "type": "object",
            "properties": { "path": { "type": "string" } }
        });
        let (out, _) = repair_tool_input(raw, Some(&schema));
        assert_eq!(out["path"], "/a/b");
    }

    #[test]
    fn alias_file_to_path() {
        let raw = json!({ "file": "x.rs" });
        let schema = json!({
            "type": "object",
            "properties": { "path": { "type": "string" } }
        });
        let (out, report) = repair_tool_input(raw, Some(&schema));
        assert_eq!(out["path"], "x.rs");
        assert!(!out.as_object().unwrap().contains_key("file"));
        assert!(report.events.iter().any(|e| e.rule == "rename_alias"));
    }

    #[test]
    fn leaves_valid_payload_untouched() {
        let raw = json!({ "text": "ok", "n": 1 });
        let schema = json!({
            "type": "object",
            "properties": {
                "text": { "type": "string" },
                "n": { "type": "integer" }
            }
        });
        let (out, report) = repair_tool_input(raw.clone(), Some(&schema));
        assert_eq!(out, raw);
        assert!(report.is_empty());
    }

    #[test]
    fn does_not_invent_json_from_plain_string() {
        let raw = Value::String("not json".into());
        let (out, report) = repair_tool_input(raw.clone(), None);
        assert_eq!(out, raw);
        assert!(report.is_empty());
    }
}
