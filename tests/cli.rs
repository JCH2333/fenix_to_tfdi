use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::Connection;

#[test]
fn help_lists_explicit_conversion_paths() {
    let output = Command::new(env!("CARGO_BIN_EXE_fenix_to_tfdi"))
        .arg("--help")
        .output()
        .expect("run converter help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is utf-8");
    for option in ["--db", "--rte-seg", "--reference", "--output", "--validate"] {
        assert!(
            stdout.contains(option),
            "missing {option} in help:\n{stdout}"
        );
    }
}

#[test]
fn validate_mode_reports_missing_candidate_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let candidate_dir = std::env::temp_dir().join(format!("fenix_to_tfdi_validate_{unique}"));
    std::fs::create_dir_all(&candidate_dir).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_fenix_to_tfdi"))
        .arg("--validate")
        .arg(&candidate_dir)
        .output()
        .expect("run candidate validation");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("required TFDI file is missing"), "{stderr}");
    std::fs::remove_dir_all(candidate_dir).unwrap();
}

#[test]
fn running_without_paths_cannot_auto_write_the_active_simulator_data() {
    let output = Command::new(env!("CARGO_BIN_EXE_fenix_to_tfdi"))
        .output()
        .expect("run converter without arguments");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("explicit conversion paths are required"));
}

#[test]
fn explicit_paths_reach_conversion_input_validation() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let output_dir = std::env::temp_dir().join(format!("fenix_to_tfdi_cli_{unique}"));
    let output = Command::new(env!("CARGO_BIN_EXE_fenix_to_tfdi"))
        .args([
            "--db",
            "missing-nd.db3",
            "--rte-seg",
            "missing-RTE_SEG.csv",
            "--reference",
            "missing-Nav-Primary",
            "--output",
        ])
        .arg(&output_dir)
        .output()
        .expect("run converter with explicit paths");

    assert!(!output.status.success());
    assert!(
        !output_dir.exists(),
        "invalid inputs created candidate output"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unsupported arguments"), "{stderr}");
    assert!(
        stderr.contains("RTE_SEG") || stderr.contains("database"),
        "{stderr}"
    );
    let _ = std::fs::remove_dir_all(&output_dir);
}

#[test]
fn explicit_conversion_initializes_candidate_from_reference() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("fenix_to_tfdi_run_{unique}"));
    let reference = root.join("official").join("Nav-Primary");
    let output_dir = root.join("candidate").join("Nav-Primary");
    let db_path = root.join("nd.db3");
    let rte_seg_path = root.join("RTE_SEG.csv");
    std::fs::create_dir_all(&reference).unwrap();
    std::fs::write(reference.join("SurfaceTypes.json"), "[]").unwrap();
    std::fs::write(&rte_seg_path, "RTE_SEG_ID,TXT_DESIG\n").unwrap();

    let connection = Connection::open(&db_path).unwrap();
    for table in [
        "AirportCommunication",
        "AirportLookup",
        "Airports",
        "AirwayLegs",
        "Airways",
        "config",
        "Gls",
        "GridMora",
        "Holdings",
        "ILSes",
        "Markers",
        "MarkerTypes",
        "NavaidLookup",
        "Navaids",
        "NavaidTypes",
        "Runways",
        "SurfaceTypes",
        "TerminalLegs",
        "TerminalLegsEx",
        "Terminals",
        "TrmLegTypes",
        "WaypointLookup",
        "Waypoints",
    ] {
        connection
            .execute(&format!("CREATE TABLE \"{table}\" (ID INTEGER)"), [])
            .unwrap();
    }
    drop(connection);

    let result = Command::new(env!("CARGO_BIN_EXE_fenix_to_tfdi"))
        .arg("--db")
        .arg(&db_path)
        .arg("--rte-seg")
        .arg(&rte_seg_path)
        .arg("--reference")
        .arg(&reference)
        .arg("--output")
        .arg(&output_dir)
        .output()
        .expect("run converter with minimal inputs");

    assert!(
        !result.status.success(),
        "minimal fixture unexpectedly converted"
    );
    assert_eq!(
        std::fs::read_to_string(output_dir.join("SurfaceTypes.json")).unwrap(),
        "[]"
    );
    std::fs::remove_dir_all(root).unwrap();
}
