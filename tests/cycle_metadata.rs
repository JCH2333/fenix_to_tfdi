use fenix_to_tfdi::adapter::tfdi::{render_cycle_files, write_cycle_files};
use fenix_to_tfdi::source::fenix::{load_cycle_metadata, parse_cycle_metadata};
use rusqlite::Connection;

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

    assert_eq!(
        config[0],
        serde_json::json!({"key":"CycleEndDate","val":"05AUG26"})
    );
    assert_eq!(
        config[1],
        serde_json::json!({"key":"CycleName","val":"2607"})
    );
    assert_eq!(
        config[2],
        serde_json::json!({"key":"CycleStartDate","val":"09JUL26"})
    );
    assert_eq!(cycle_json["cycle"], "2607");
    assert_eq!(cycle_json["revision"], "2");
    assert_eq!(cycle_json["name"], "TFDi Design MD-11");
}

#[test]
fn cycle_metadata_is_loaded_from_the_fenix_config_table() {
    let db_path = std::env::temp_dir().join(format!(
        "fenix_cycle_{}_{}.db3",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    let connection = Connection::open(&db_path).unwrap();
    connection
        .execute("CREATE TABLE config (key TEXT, val TEXT)", [])
        .unwrap();
    connection
        .execute(
            "INSERT INTO config VALUES ('CycleEndDate','05AUG26'),('CycleName','2607n2'),('CycleStartDate','09JUL26')",
            [],
        )
        .unwrap();
    drop(connection);

    let cycle = load_cycle_metadata(&db_path).expect("load Fenix cycle metadata");

    assert_eq!(cycle.cycle, "2607");
    assert_eq!(cycle.revision, "2");
    std::fs::remove_file(db_path).unwrap();
}

#[test]
fn tfdi_candidate_cycle_files_replace_template_metadata() {
    let output_dir = std::env::temp_dir().join(format!(
        "tfdi_cycle_output_{}_{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    std::fs::create_dir_all(&output_dir).unwrap();
    std::fs::write(output_dir.join("Config.json"), "[]").unwrap();
    std::fs::write(output_dir.join("cycle.json"), "{}").unwrap();
    let cycle = parse_cycle_metadata([
        ("CycleStartDate", "09JUL26"),
        ("CycleName", "2607n2"),
        ("CycleEndDate", "05AUG26"),
    ])
    .unwrap();

    write_cycle_files(&output_dir, &cycle).expect("write TFDI cycle files");

    let config: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("Config.json")).unwrap()).unwrap();
    let cycle_json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output_dir.join("cycle.json")).unwrap()).unwrap();
    assert_eq!(config[1]["val"], "2607");
    assert_eq!(cycle_json["revision"], "2");
    std::fs::remove_dir_all(output_dir).unwrap();
}
