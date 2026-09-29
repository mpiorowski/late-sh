use crate::models::daily_match::DailyResult;

#[test]
fn every_result_reads_back_from_its_stored_spelling() {
    for result in DailyResult::ALL {
        assert_eq!(DailyResult::parse(result.as_str()).unwrap(), result);
    }
}

#[test]
fn an_unfinished_rows_empty_result_is_not_a_result() {
    let error = DailyResult::parse("").unwrap_err();
    assert_eq!(error.to_string(), "unknown daily match result: \"\"");
}
