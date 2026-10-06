use super::*;

#[test]
fn a_smaller_screen_is_centered() {
    assert_eq!(
        fit_axis(80, 100, 40),
        AxisFit {
            src: 0,
            dst: 10,
            len: 80
        }
    );
}

#[test]
fn a_larger_screen_follows_the_cursor() {
    assert_eq!(
        fit_axis(120, 80, 60),
        AxisFit {
            src: 20,
            dst: 0,
            len: 80
        }
    );
}

#[test]
fn the_crop_window_stops_at_the_screens_edges() {
    assert_eq!(fit_axis(120, 80, 5).src, 0);
    assert_eq!(fit_axis(120, 80, 119).src, 40);
}

#[test]
fn duration_label_reads_minutes_then_hours() {
    assert_eq!(duration_label(0), "0m");
    assert_eq!(duration_label(59), "59m");
    assert_eq!(duration_label(185), "3h 05m");
}
