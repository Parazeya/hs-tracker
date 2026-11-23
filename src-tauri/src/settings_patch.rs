//! Three-way settings edits: untouched fields always come from current state.
use serde_json::{Map, Value};
use std::sync::Mutex;

pub static WRITER: Mutex<()> = Mutex::new(());

pub fn merge(base: &Value, edited: &Value, current: &Value) -> Result<Value, String> {
    fn object(
        base: &Map<String, Value>,
        edited: &Map<String, Value>,
        current: &Map<String, Value>,
    ) -> Map<String, Value> {
        let mut result = current.clone();
        for (key, value) in edited {
            if base.get(key) == Some(value) {
                continue;
            }
            let next = match (base.get(key), value, current.get(key)) {
                (Some(Value::Object(a)), Value::Object(b), Some(Value::Object(c))) => {
                    Value::Object(object(a, b, c))
                }
                _ => value.clone(),
            };
            result.insert(key.clone(), next);
        }
        for key in base.keys() {
            if !edited.contains_key(key) {
                result.remove(key);
            }
        }
        result
    }
    match (base.as_object(), edited.as_object(), current.as_object()) {
        (Some(a), Some(b), Some(c)) => Ok(Value::Object(object(a, b, c))),
        _ => Err("settings edits must be JSON objects".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn a_pending_ui_edit_does_not_undo_native_or_nested_changes() {
        let base = json!({"opacity":0.7,"locked":false,"sound":{"volume":0.5,"enabled":true}});
        let edited = json!({"opacity":0.9,"locked":false,"sound":{"volume":0.8,"enabled":true}});
        let current = json!({"opacity":0.7,"locked":true,"sound":{"volume":0.5,"enabled":false}});
        assert_eq!(
            merge(&base, &edited, &current).unwrap(),
            json!({"opacity":0.9,"locked":true,"sound":{"volume":0.8,"enabled":false}})
        );
    }
    #[test]
    fn successive_stale_requests_preserve_unrelated_committed_fields() {
        let base = json!({"opacity":0.7,"locked":false,"scale":1});
        let first = merge(
            &base,
            &json!({"opacity":0.8,"locked":false,"scale":1}),
            &json!({"opacity":0.7,"locked":true,"scale":1.2}),
        )
        .unwrap();
        let second = merge(
            &base,
            &json!({"opacity":0.9,"locked":false,"scale":1}),
            &first,
        )
        .unwrap();
        assert_eq!(second, json!({"opacity":0.9,"locked":true,"scale":1.2}));
        assert!(merge(&Value::Null, &base, &base).is_err());
    }
}
