use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn help_lists_explicit_conversion_paths() {
    let output = Command::new(env!("CARGO_BIN_EXE_fenix_to_tfdi"))
        .arg("--help")
        .output()
        .expect("run converter help");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("help is utf-8");
    for option in ["--db", "--rte-seg", "--reference", "--output"] {
        assert!(stdout.contains(option), "missing {option} in help:\n{stdout}");
    }
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
    assert!(!output_dir.exists(), "invalid inputs created candidate output");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains("unsupported arguments"), "{stderr}");
    assert!(stderr.contains("RTE_SEG") || stderr.contains("database"), "{stderr}");
    let _ = std::fs::remove_dir_all(&output_dir);
}
