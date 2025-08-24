use ahash::{AHashMap, AHashSet};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use serde_json::{json, Map, Value};
use std::cmp::Ordering;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
enum TypeTag {
    Null,
    Boolean,
    Integer,
    Number,
    String,
    Object,
    Array,
}

impl TypeTag {
    fn as_str(&self) -> &'static str {
        match self {
            TypeTag::Null => "null",
            TypeTag::Boolean => "boolean",
            TypeTag::Integer => "integer",
            TypeTag::Number => "number",
            TypeTag::String => "string",
            TypeTag::Object => "object",
            TypeTag::Array => "array",
        }
    }
}

#[derive(Debug, Clone, Default)]
struct Node {
    // Наблюдаемые типы на этом уровне
    types: AHashSet<TypeTag>,
    // Для объектов
    properties: AHashMap<String, Node>,
    // Для массивов
    items: Option<Box<Node>>,
}

impl Node {
    fn observe(&mut self, v: &Value) {
        match v {
            Value::Null => {
                self.types.insert(TypeTag::Null);
            }
            Value::Bool(_) => {
                self.types.insert(TypeTag::Boolean);
            }
            Value::Number(n) => {
                if n.is_i64() || n.is_u64() {
                    self.types.insert(TypeTag::Integer);
                } else {
                    self.types.insert(TypeTag::Number);
                }
            }
            Value::String(_) => {
                self.types.insert(TypeTag::String);
            }
            Value::Array(arr) => {
                self.types.insert(TypeTag::Array);
                let items_node = self.items.get_or_insert_with(|| Box::new(Node::default()));
                for el in arr {
                    items_node.observe(el);
                }
            }
            Value::Object(obj) => {
                self.types.insert(TypeTag::Object);
                for (k, vv) in obj {
                    self.properties
                        .entry(k.to_string())
                        .or_default()
                        .observe(vv);
                }
            }
        }
    }

    fn to_json_schema(&self) -> Value {
        let mut m = Map::new();

        let mut types: Vec<&str> = self.types.iter().map(|t| t.as_str()).collect();
        types.sort_by(|a, b| {
            // небольшая стабильная сортировка для одинакового вывода
            if a == b {
                Ordering::Equal
            } else {
                a.cmp(b)
            }
        });

        match types.as_slice() {
            [one] => {
                m.insert("type".to_string(), Value::String(one.to_string()));
            }
            many if !many.is_empty() => {
                m.insert(
                    "type".to_string(),
                    Value::Array(
                        many.iter()
                            .map(|s| Value::String((*s).to_string()))
                            .collect(),
                    ),
                );
            }
            _ => {}
        }

        if self.types.contains(&TypeTag::Object) && !self.properties.is_empty() {
            let mut props = Map::new();
            for (k, v) in &self.properties {
                props.insert(k.clone(), v.to_json_schema());
            }
            m.insert("properties".to_string(), Value::Object(props));
            // MVP: без вычисления required — добавим на следующей итерации
            // m.insert("required", Value::Array(vec![]));
        }

        if self.types.contains(&TypeTag::Array) {
            if let Some(items) = &self.items {
                m.insert("items".to_string(), items.to_json_schema());
            }
        }

        Value::Object(m)
    }
}

fn parse_samples(samples: &[String]) -> Result<Node, String> {
    let mut root = Node::default();
    for s in samples {
        let v: Value = serde_json::from_str(s).map_err(|e| format!("Invalid JSON: {e}"))?;
        root.observe(&v);
    }
    Ok(root)
}

fn collect_paths(schema: &Value, prefix: &str, acc: &mut AHashSet<String>) {
    if let Some(obj) = schema.as_object() {
        if let Some(props) = obj.get("properties").and_then(|p| p.as_object()) {
            for (k, v) in props {
                let next = if prefix.is_empty() {
                    k.to_string()
                } else {
                    format!("{prefix}.{k}")
                };
                acc.insert(next.clone());
                collect_paths(v, &next, acc);
            }
        }
        if let Some(items) = obj.get("items") {
            let next = if prefix.is_empty() {
                "[]".to_string()
            } else {
                format!("{prefix}[]")
            };
            acc.insert(next.clone());
            collect_paths(items, &next, acc);
        }
    }
}

// Rust-native API used by integration tests
pub fn infer_schema_rs(samples: &[String]) -> Result<String, String> {
    let node = parse_samples(samples).map_err(|e| e)?;
    let schema = node.to_json_schema();
    serde_json::to_string_pretty(&json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": schema.get("properties").cloned().unwrap_or_else(|| json!({}))
    }))
    .map_err(|e| format!("Serialize error: {e}"))
}

pub fn diff_schemas_rs(a: &str, b: &str) -> Result<String, String> {
    let va: Value = serde_json::from_str(a)
        .map_err(|e| format!("schema A parse error: {e}"))?;
    let vb: Value = serde_json::from_str(b)
        .map_err(|e| format!("schema B parse error: {e}"))?;

    let mut ka = AHashSet::default();
    let mut kb = AHashSet::default();
    collect_paths(&va, "", &mut ka);
    collect_paths(&vb, "", &mut kb);

    let added: Vec<String> = kb.difference(&ka).cloned().collect();
    let removed: Vec<String> = ka.difference(&kb).cloned().collect();
    let common: Vec<String> = ka.intersection(&kb).cloned().collect();

    let out = json!({
        "added": added,
        "removed": removed,
        "common": common
    });
    serde_json::to_string_pretty(&out)
        .map_err(|e| format!("Serialize error: {e}"))
}

/// infer_schema(samples: List[str]) -> str(JSON)
#[pyfunction]
fn infer_schema(samples: Vec<String>) -> PyResult<String> {
    infer_schema_rs(&samples).map_err(PyValueError::new_err)
}

/// diff_schemas(a: str(JSON), b: str(JSON)) -> str(JSON)
#[pyfunction]
fn diff_schemas(a: String, b: String) -> PyResult<String> {
    diff_schemas_rs(&a, &b).map_err(PyValueError::new_err)
}

#[pymodule]
fn aif_core(_py: Python, m: &Bound<pyo3::types::PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(infer_schema, m)?)?;
    m.add_function(wrap_pyfunction!(diff_schemas, m)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    #[test]
    fn infer_simple_schema_object() {
        let samples = vec![
            r#"{"id":1,"name":"Alice","tags":["a","b"]}"#.to_string(),
            r#"{"id":2,"name":"Bob","tags":[]}"#.to_string(),
        ];
        let out = infer_schema(samples).expect("infer ok");
        let v: Value = serde_json::from_str(&out).unwrap();

        assert_eq!(v["type"], "object");
        assert!(v["properties"].get("id").is_some());
        assert!(v["properties"].get("name").is_some());
        assert!(v["properties"].get("tags").is_some());
    }

    #[test]
    fn diff_detects_added_removed_common() {
        // Схема A: только id
        let a = r#"{
          "$schema":"https://json-schema.org/draft/2020-12/schema",
          "type":"object",
          "properties":{"id":{"type":"integer"}}
        }"#.to_string();

        // Схема B: id + name + tags
        let b = r#"{
          "$schema":"https://json-schema.org/draft/2020-12/schema",
          "type":"object",
          "properties":{
            "id":{"type":"integer"},
            "name":{"type":"string"},
            "tags":{"type":"array","items":{"type":"string"}}
          }
        }"#.to_string();

        let out = diff_schemas(a, b).expect("diff ok");
        let d: Value = serde_json::from_str(&out).unwrap();

        let added = d["added"].as_array().unwrap();
        let added_set: std::collections::HashSet<_> =
            added.iter().filter_map(|s| s.as_str()).collect();
        assert!(added_set.contains("name"));
        assert!(added_set.contains("tags"));   // для массивов будет ещё "tags[]", это нормально

        let removed = d["removed"].as_array().unwrap();
        assert!(removed.is_empty());

        let common = d["common"].as_array().unwrap();
        assert!(common.iter().any(|s| s.as_str() == Some("id")));
    }

    #[test]
    fn roundtrip_infer_then_diff() {
        let s1 = vec![r#"{"a":{"x":1}}"#.to_string()];
        let s2 = vec![r#"{"a":{"x":1,"y":"u"},"b":[1,2]}"#.to_string()];

        let a = infer_schema(s1).unwrap();
        let b = infer_schema(s2).unwrap();
        let out = diff_schemas(a, b).unwrap();
        let d: Value = serde_json::from_str(&out).unwrap();
        let added = d["added"].as_array().unwrap();

        let added_set: std::collections::HashSet<_> =
            added.iter().filter_map(|s| s.as_str()).collect();
        assert!(added_set.contains("a.y"));
        assert!(added_set.contains("b"));      // и, скорее всего, "b[]" тоже появится
    }
}
