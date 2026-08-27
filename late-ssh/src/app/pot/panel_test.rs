use super::{POT_PANEL_HEIGHT, pot_panel_lines};
use crate::app::pot::state::PotView;

/// The rail is 24 columns wide and the panel is drawn into 21 of them (a
/// separator column, a padding column, and a right inset). Everything below
/// asserts against that width, since it is the only one that ships.
const RAIL_WIDTH: u16 = 21;

fn plain(view: &PotView) -> Vec<String> {
    pot_panel_lines(RAIL_WIDTH, view)
        .into_iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect::<String>()
        })
        .collect()
}

fn view(size: i64, ticket_count: i64, my_tickets: i64, draws_in: &str) -> PotView {
    PotView {
        size,
        ticket_count,
        my_tickets,
        draws_in: draws_in.to_string(),
        open: true,
    }
}

/// What a player actually reads: the prize and the clock on one row, the
/// field and their place in it on the other, both rows exactly the rail's
/// width so the panel never shifts what sits under it.
#[test]
fn the_panel_shows_the_pot_the_clock_and_your_tickets() {
    let lines = plain(&view(84_200, 842, 5, "3h12m"));
    assert_eq!(
        lines,
        vec!["84,200       in 3h12m", "842 tickets     you 5"]
    );
    assert_eq!(lines.len(), POT_PANEL_HEIGHT as usize);
    for line in &lines {
        assert_eq!(line.chars().count(), RAIL_WIDTH as usize);
    }
}

/// An empty pot still renders every row, so the panel's shape is the same
/// before the first ticket as after the last.
#[test]
fn an_empty_pot_keeps_the_panels_shape() {
    assert_eq!(
        plain(&view(0, 0, 0, "23h59m")),
        vec!["0           in 23h59m", "0 tickets       you 0"]
    );
}

/// Before the first refresh there is no pot to describe. Dashes, not a
/// zero-chip pot nobody can buy into.
#[test]
fn a_pot_that_has_not_loaded_renders_dashes() {
    assert_eq!(plain(&PotView::default()), vec!["  ─", "  ─"]);
}

/// A pot too big for the row gives up columns on the left; the countdown and
/// the holding, which are what a player acts on, never truncate.
#[test]
fn a_wide_row_truncates_the_left_value() {
    let lines = plain(&view(123_456_789_012, 123_456, 50, "23h59m"));
    assert_eq!(lines[0], "123,456,78… in 23h59m");
    assert_eq!(lines[1], "123,456 ticke… you 50");
    for line in &lines {
        assert!(line.chars().count() <= RAIL_WIDTH as usize);
    }
}
