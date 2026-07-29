use fenix_to_tfdi::adapter::tfdi::render_cycle_files;
use fenix_to_tfdi::source::fenix::parse_cycle_metadata;

#[test]
fn fenix_cycle_revision_is_normalized_into_the_source_model() {
    let cycle = parse_cycle_metadata([
        ("CycleStartDate", "09JUL26"),
        ("CycleName", "2607n2"),
        ("CycleEndDate", "05AUG26"),
    ])
    .expect("valid Fenix cycle metadata");

    assert_eq!(cycle.cycle, "2607");
    assert_eq!(cycle.revision, "2");
    assert_eq!(cycle.start_date, "09JUL26");
    assert_eq!(cycle.end_date, "05AUG26");
}

#[test]
fn tfdi_cycle_files_share_the_normalized_cycle_and_revision() {
    let cycle = parse_cycle_metadata([
        ("CycleStartDate", "09JUL26"),
        ("CycleName", "2607n2"),
        ("CycleEndDate", "05AUG26"),
    ])
    .unwrap();

    let files = render_cycle_files(&cycle).expect("render TFDI cycle files");
    let config: serde_json::Value = serde_json::from_str(&files.config_json).unwrap();
    let cycle_json: serde_json::Value = serde_json::from_str(&files.cycle_json).unwrap();

    assert_eq!(config[0], serde_json::json!({"key":"CycleEndDate","val":"05AUG26"}));
    assert_eq!(config[1], serde_json::json!({"key":"CycleName","val":"2607"}));
    assert_eq!(config[2], serde_json::json!({"key":"CycleStartDate","val":"09JUL26"}));
    assert_eq!(cycle_json["cycle"], "2607");
    assert_eq!(cycle_json["revision"], "2");
    assert_eq!(cycle_json["name"], "TFDi Design MD-11");
}
