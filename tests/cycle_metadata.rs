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
