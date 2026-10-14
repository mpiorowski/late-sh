use super::date_entry::parse;
use chrono::NaiveDate;

fn d(s: &str) -> NaiveDate {
    s.parse().unwrap()
}

#[test]
fn calendar_date_entry_flexible_absolute_forms() {
    let today = d("2026-10-02");
    for input in [
        "2028-2-29",
        "2028/02/29",
        "2028.2.29",
        "29/2/2028",
        "2/29/2028",
        "February 29, 2028",
        "29th Feb 2028",
        "FEB. 29TH, 2028",
        "Feb-29-2028",
        "  Feb\t29  2028  ",
    ] {
        assert_eq!(
            parse(input, d("2030-01-01"), today).unwrap(),
            d("2028-02-29"),
            "{input}"
        );
    }
    for input in ["Oct 2", "2 October", "Oct. 2nd"] {
        assert_eq!(
            parse(input, d("2030-01-01"), today).unwrap(),
            today,
            "{input}"
        );
    }
    assert_eq!(
        parse("Sept 21st 2026", today, today).unwrap(),
        d("2026-09-21")
    );
    assert_eq!(parse("12/12/2026", today, today).unwrap(), d("2026-12-12"));
}

#[test]
fn calendar_date_entry_relative_offsets_use_calendar_months_and_years() {
    let today = d("2026-10-02");
    let base = d("2028-01-31");
    for (input, expected) in [
        ("1 month", "2028-02-29"),
        ("+1mo", "2028-02-29"),
        ("1M", "2028-02-29"),
        ("in 2 months", "2028-03-31"),
        ("2 months from now", "2028-03-31"),
        ("2 MONTHS AGO", "2027-11-30"),
        ("-2 months", "2027-11-30"),
        ("−2 months", "2027-11-30"),
        ("next month", "2028-02-29"),
        ("last year", "2027-01-31"),
        ("+2w", "2028-02-14"),
        ("in 3 weeks", "2028-02-21"),
        ("2 weeks 3 days ago", "2028-01-14"),
        ("+1month2d", "2028-03-02"),
        ("1 year 2 months 3 days", "2029-04-03"),
        ("0 days", "2028-01-31"),
    ] {
        assert_eq!(parse(input, base, today).unwrap(), d(expected), "{input}");
    }
    assert_eq!(
        parse("+1 year", d("2028-02-29"), today).unwrap(),
        d("2029-02-28")
    );
    assert_eq!(
        parse("1 month ago", d("2026-03-31"), today).unwrap(),
        d("2026-02-28")
    );
    // A day is a civil date step, independent of the viewer's DST transition.
    assert_eq!(
        parse("+2 days", d("2026-03-07"), today).unwrap(),
        d("2026-03-09")
    );
}

#[test]
fn calendar_date_entry_today_and_weekdays_use_account_local_today() {
    let today = d("2026-10-02"); // Friday; base is deliberately another year.
    for (input, expected) in [
        ("today", "2026-10-02"),
        ("yesterday", "2026-10-01"),
        ("Tomorrow", "2026-10-03"),
        ("friday", "2026-10-02"),
        ("next Fri", "2026-10-09"),
        ("last Friday", "2026-09-25"),
        ("Monday", "2026-10-05"),
        ("last Monday", "2026-09-28"),
    ] {
        assert_eq!(
            parse(input, d("2030-01-01"), today).unwrap(),
            d(expected),
            "{input}"
        );
    }
}

#[test]
fn calendar_date_entry_rejects_ambiguous_invalid_and_unbounded_input() {
    let today = d("2026-10-02");
    for input in [
        "",
        "02/03/2026",
        "02-03-2026",
        "2026-02-29",
        "Feb 30",
        "Feb 29",
        "Oct 11st",
        "2 Oct 26",
        "2026/99/99",
        "next someday",
        "two months ago",
        "in -2 days",
        "-2 days ago",
        "1.5 months",
        "12 hours ago",
        "2m",
        "forever",
        "999999999999999999 months",
        "4294967295 years",
        "4294967295M1M",
        "4294967295 weeks",
        "10000 years",
        "0000-01-01",
        "tomorrow noon",
    ] {
        assert!(parse(input, today, today).is_err(), "{input}");
    }
    assert!(parse("+1 day", d("9999-12-31"), today).is_err());
    assert!(parse("-1 day", d("0001-01-01"), today).is_err());
    assert!(
        parse("02/03/2026", today, today)
            .unwrap_err()
            .to_string()
            .contains("Ambiguous")
    );
}
