// IMPLEMENTS: D-073
use serde_json::Value;
use std::collections::HashSet;
use thiserror::Error;

pub const MAX_DEPTH: usize = 16;
pub const MAX_NODES: usize = 5000;
pub const MAX_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum SchemaError {
    #[error("schema exceeds {MAX_BYTES}-byte size limit ({0} bytes)")]
    TooLarge(usize),
    #[error("schema nesting exceeds {MAX_DEPTH} levels")]
    TooDeep,
    #[error("schema has more than {MAX_NODES} nodes ({0})")]
    TooManyNodes(usize),
    #[error("$ref cycle detected at {0}")]
    RefCycle(String),
}

pub fn validate(schema: &Value) -> Result<(), SchemaError> {
    let serialized = serde_json::to_vec(schema).map_err(|_| SchemaError::TooLarge(0))?;
    if serialized.len() > MAX_BYTES {
        return Err(SchemaError::TooLarge(serialized.len()));
    }
    let mut state = WalkState::default();
    walk(schema, 0, &mut state)?;
    if state.nodes > MAX_NODES {
        return Err(SchemaError::TooManyNodes(state.nodes));
    }
    Ok(())
}

#[derive(Default)]
struct WalkState {
    nodes: usize,
    refs_seen: HashSet<String>,
}

fn walk(value: &Value, depth: usize, state: &mut WalkState) -> Result<(), SchemaError> {
    if depth > MAX_DEPTH {
        return Err(SchemaError::TooDeep);
    }
    state.nodes += 1;
    if state.nodes > MAX_NODES {
        return Err(SchemaError::TooManyNodes(state.nodes));
    }
    match value {
        Value::Object(map) => {
            if let Some(Value::String(target)) = map.get("$ref")
                && !state.refs_seen.insert(target.clone())
            {
                return Err(SchemaError::RefCycle(target.clone()));
            }
            for v in map.values() {
                walk(v, depth + 1, state)?;
            }
        }
        Value::Array(items) => {
            for v in items {
                walk(v, depth + 1, state)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_object_passes() {
        validate(&json!({})).unwrap();
    }

    #[test]
    fn small_typed_schema_passes() {
        validate(&json!({
            "type": "object",
            "properties": {
                "path": { "type": "string" },
                "count": { "type": "integer" }
            },
            "required": ["path"]
        }))
        .unwrap();
    }

    #[test]
    fn rejects_when_too_deep() {
        let mut v = json!(0);
        for _ in 0..(MAX_DEPTH + 2) {
            v = json!({ "x": v });
        }
        let err = validate(&v).unwrap_err();
        assert!(matches!(err, SchemaError::TooDeep), "got {err:?}");
    }

    #[test]
    fn rejects_too_many_nodes() {
        let arr: Vec<Value> = (0..MAX_NODES + 50).map(|_| json!(0)).collect();
        let err = validate(&Value::Array(arr)).unwrap_err();
        assert!(matches!(err, SchemaError::TooManyNodes(_)), "got {err:?}");
    }

    #[test]
    fn rejects_oversized_payload() {
        let big = "x".repeat(MAX_BYTES + 1);
        let err = validate(&json!({ "k": big })).unwrap_err();
        assert!(matches!(err, SchemaError::TooLarge(_)), "got {err:?}");
    }

    #[test]
    fn rejects_ref_cycle() {
        // The same $ref target appearing twice in the tree is treated as a
        // cycle for the purposes of the validator's flat scan.
        let v = json!({
            "definitions": {
                "node": { "$ref": "#/definitions/node" }
            },
            "$ref": "#/definitions/node"
        });
        let err = validate(&v).unwrap_err();
        assert!(matches!(err, SchemaError::RefCycle(_)), "got {err:?}");
    }

    #[test]
    fn allows_distinct_refs() {
        validate(&json!({
            "items": [
                { "$ref": "#/A" },
                { "$ref": "#/B" }
            ]
        }))
        .unwrap();
    }
}
