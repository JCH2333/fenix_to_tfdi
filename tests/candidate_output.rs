use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use fenix_to_tfdi::candidate::copy_template_to_candidate;

#[test]
fn candidate_starts_as_a_complete_copy_of_the_official_template() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("fenix_to_tfdi_candidate_{unique}"));
    let reference = root.join("official").join("Nav-Primary");
    let output = root.join("candidate").join("Nav-Primary");
    fs::create_dir_all(reference.join("ProcedureLegs")).unwrap();
    fs::write(reference.join("SurfaceTypes.json"), "[]").unwrap();
    fs::write(
        reference.join("ProcedureLegs").join("TermID_1.json"),
        "[{\"ID\":1}]",
    )
    .unwrap();

    copy_template_to_candidate(&reference, &output).expect("copy official template");

    assert_eq!(fs::read_to_string(output.join("SurfaceTypes.json")).unwrap(), "[]");
    assert_eq!(
        fs::read_to_string(output.join("ProcedureLegs").join("TermID_1.json")).unwrap(),
        "[{\"ID\":1}]"
    );
    fs::remove_dir_all(root).unwrap();
}
