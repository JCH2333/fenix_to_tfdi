use std::process::Command;

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
