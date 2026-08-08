use std::path::PathBuf;

use fenix_to_tfdi::cli::parse_conversion_args;

#[test]
fn explicit_paths_form_one_isolated_conversion_request() {
    let request = parse_conversion_args([
        "--db",
        "inputs/nd.db3",
        "--rte-seg",
        "inputs/2607/RTE_SEG.csv",
        "--reference",
        "official/Nav-Primary",
        "--output",
        "output/Nav-Primary",
    ])
    .expect("valid conversion arguments");

    assert_eq!(request.db_path, PathBuf::from("inputs/nd.db3"));
    assert_eq!(
        request.rte_seg_path,
        PathBuf::from("inputs/2607/RTE_SEG.csv")
    );
    assert_eq!(request.reference_dir, PathBuf::from("official/Nav-Primary"));
    assert_eq!(request.output_dir, PathBuf::from("output/Nav-Primary"));
}

#[test]
fn optional_ils_reference_database_is_preserved_in_conversion_request() {
    let request = parse_conversion_args([
        "--db",
        "inputs/nd-2608.db3",
        "--ils-reference-db",
        "inputs/nd-2607-ils.db3",
        "--rte-seg",
        "inputs/RTE_SEG.csv",
        "--reference",
        "official/Nav-Primary",
        "--output",
        "output/Nav-Primary",
    ])
    .expect("valid conversion arguments with ILS reference");

    assert_eq!(
        request.ils_reference_db_path,
        Some(PathBuf::from("inputs/nd-2607-ils.db3"))
    );
}

#[test]
fn active_reference_directory_cannot_be_used_as_candidate_output() {
    let error = parse_conversion_args([
        "--db",
        "inputs/nd.db3",
        "--rte-seg",
        "inputs/2607/RTE_SEG.csv",
        "--reference",
        "official/Nav-Primary",
        "--output",
        "official/Nav-Primary",
    ])
    .expect_err("in-place conversion must be rejected");

    assert!(error.to_string().contains("must be different"));
}
