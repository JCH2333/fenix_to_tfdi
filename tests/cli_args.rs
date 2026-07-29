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
