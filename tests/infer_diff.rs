use aif_core::{diff_schemas_rs, infer_schema_rs};

#[test]
fn infer_then_diff_works() {
    let s1 = vec![r#"{"id":1,"name":"Alice"}"#.to_string()];
    let s2 = vec![r#"{"id":2,"name":"Bob","tags":["x"]}"#.to_string()];
    let a = infer_schema_rs(&s1).unwrap();
    let b = infer_schema_rs(&s2).unwrap();
    let d = diff_schemas_rs(&a, &b).unwrap();
    assert!(d.contains("added"));
}
