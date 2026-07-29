use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use fenix_to_tfdi::adapter::tfdi::validate_candidate;

fn candidate_fixture() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("tfdi_validation_{unique}"));
    fs::create_dir_all(root.join("ProcedureLegs")).unwrap();
    for file in [
        "Airports.json",
        "Runways.json",
        "Terminals.json",
        "Navaids.json",
        "NavaidLookup.json",
        "Waypoints.json",
        "WaypointLookup.json",
        "Airways.json",
        "AirwayLegs.json",
        "ILSes.json",
    ] {
        fs::write(root.join(file), "[]").unwrap();
    }
    fs::write(
        root.join("Config.json"),
        r#"[{"key":"CycleEndDate","val":"05AUG26"},{"key":"CycleName","val":"2607"},{"key":"CycleStartDate","val":"09JUL26"}]"#,
    )
    .unwrap();
    fs::write(
        root.join("cycle.json"),
        r#"{"cycle":"2607","revision":"2","name":"TFDi Design MD-11"}"#,
    )
    .unwrap();

    root
}

#[test]
fn validator_rejects_procedure_file_without_matching_terminal() {
    let root = candidate_fixture();
    fs::write(root.join("ProcedureLegs").join("TermID_99.json"), "[]").unwrap();

    let error = validate_candidate(&root).expect_err("orphan procedure must fail validation");

    assert!(error.to_string().contains("has no matching terminal"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_missing_required_main_json() {
    let root = candidate_fixture();
    fs::remove_file(root.join("Airports.json")).unwrap();

    let error = validate_candidate(&root).expect_err("missing main JSON must fail validation");

    assert!(error.to_string().contains("Airports.json"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_malformed_required_main_json() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), "not JSON").unwrap();

    let error = validate_candidate(&root).expect_err("malformed main JSON must fail validation");

    assert!(error.to_string().contains("Airports.json"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_duplicate_ids_in_primary_tables() {
    for file_name in [
        "Airports.json",
        "Runways.json",
        "Terminals.json",
        "Navaids.json",
        "Waypoints.json",
        "Airways.json",
        "AirwayLegs.json",
        "ILSes.json",
    ] {
        let root = candidate_fixture();
        fs::write(root.join(file_name), r#"[{"ID":7},{"ID":7}]"#).unwrap();

        let error = validate_candidate(&root).expect_err("duplicate ID must fail validation");

        assert!(error.to_string().contains(file_name));
        assert!(error.to_string().contains("duplicate ID 7"));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn validator_rejects_runway_without_matching_airport() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), r#"[{"ID":1}]"#).unwrap();
    fs::write(root.join("Runways.json"), r#"[{"ID":2,"AirportID":99}]"#).unwrap();

    let error = validate_candidate(&root).expect_err("orphan runway must fail validation");

    assert!(error.to_string().contains("Runways.json"));
    assert!(error.to_string().contains("AirportID 99"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_terminal_without_matching_airport() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), r#"[{"ID":1}]"#).unwrap();
    fs::write(root.join("Terminals.json"), r#"[{"ID":3,"AirportID":99}]"#).unwrap();

    let error = validate_candidate(&root).expect_err("orphan terminal must fail validation");

    assert!(error.to_string().contains("Terminals.json"));
    assert!(error.to_string().contains("AirportID 99"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_terminal_without_matching_runway() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), r#"[{"ID":1}]"#).unwrap();
    fs::write(
        root.join("Terminals.json"),
        r#"[{"ID":3,"AirportID":1,"RwyID":99}]"#,
    )
    .unwrap();

    let error = validate_candidate(&root).expect_err("orphan runway ID must fail validation");

    assert!(error.to_string().contains("Terminals.json"));
    assert!(error.to_string().contains("RwyID 99"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_ils_without_matching_runway() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), r#"[{"ID":1}]"#).unwrap();
    fs::write(root.join("Runways.json"), r#"[{"ID":2,"AirportID":1}]"#).unwrap();
    fs::write(root.join("ILSes.json"), r#"[{"ID":4,"RunwayID":99}]"#).unwrap();

    let error = validate_candidate(&root).expect_err("orphan ILS must fail validation");

    assert!(error.to_string().contains("ILSes.json"));
    assert!(error.to_string().contains("RunwayID 99"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_waypoint_without_matching_navaid() {
    let root = candidate_fixture();
    fs::write(root.join("Waypoints.json"), r#"[{"ID":5,"NavaidID":99}]"#).unwrap();

    let error = validate_candidate(&root).expect_err("orphan navaid ID must fail validation");

    assert!(error.to_string().contains("Waypoints.json"));
    assert!(error.to_string().contains("NavaidID 99"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_airway_leg_with_missing_references() {
    for (field, airway_id, waypoint1_id, waypoint2_id) in [
        ("AirwayID 99", 99, 20, 21),
        ("Waypoint1ID 99", 10, 99, 21),
        ("Waypoint2ID 99", 10, 20, 99),
    ] {
        let root = candidate_fixture();
        fs::write(root.join("Airways.json"), r#"[{"ID":10}]"#).unwrap();
        fs::write(root.join("Waypoints.json"), r#"[{"ID":20},{"ID":21}]"#).unwrap();
        fs::write(
            root.join("AirwayLegs.json"),
            format!(
                r#"[{{"ID":30,"AirwayID":{airway_id},"Waypoint1ID":{waypoint1_id},"Waypoint2ID":{waypoint2_id}}}]"#
            ),
        )
        .unwrap();

        let error = validate_candidate(&root).expect_err("orphan airway leg must fail validation");

        assert!(error.to_string().contains("AirwayLegs.json"));
        assert!(error.to_string().contains(field));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn validator_rejects_lookup_without_matching_primary_row() {
    for (lookup_file, primary_file) in [
        ("NavaidLookup.json", "Navaids.json"),
        ("WaypointLookup.json", "Waypoints.json"),
    ] {
        let root = candidate_fixture();
        fs::write(root.join(primary_file), r#"[{"ID":1}]"#).unwrap();
        fs::write(root.join(lookup_file), r#"[{"ID":99}]"#).unwrap();

        let error = validate_candidate(&root).expect_err("orphan lookup must fail validation");

        assert!(error.to_string().contains(lookup_file));
        assert!(error.to_string().contains("ID 99"));
        fs::remove_dir_all(root).unwrap();
    }
}

#[test]
fn validator_rejects_terminal_without_procedure_file() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), r#"[{"ID":1}]"#).unwrap();
    fs::write(
        root.join("Terminals.json"),
        r#"[{"ID":3,"AirportID":1,"RwyID":null}]"#,
    )
    .unwrap();

    let error = validate_candidate(&root).expect_err("missing procedure file must fail validation");

    assert!(error.to_string().contains("TermID_3.json"));
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn validator_rejects_malformed_procedure_file() {
    let root = candidate_fixture();
    fs::write(root.join("Airports.json"), r#"[{"ID":1}]"#).unwrap();
    fs::write(
        root.join("Terminals.json"),
        r#"[{"ID":3,"AirportID":1,"RwyID":null}]"#,
    )
    .unwrap();
    fs::write(root.join("ProcedureLegs").join("TermID_3.json"), "not JSON").unwrap();

    let error = validate_candidate(&root).expect_err("malformed procedure must fail validation");

    assert!(error.to_string().contains("TermID_3.json"));
    fs::remove_dir_all(root).unwrap();
}
