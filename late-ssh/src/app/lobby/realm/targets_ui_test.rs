use ratatui::style::Style;
use ratatui::{Terminal, backend::TestBackend};

use super::{Columns, target_line};
use crate::app::common::theme;
use crate::app::lobby::realm::rulesets::{Reach, RouteMode};
use crate::app::lobby::realm::state::TargetRow;

fn row(name: &str, mine: bool) -> TargetRow {
    TargetRow {
        id: 1,
        name: name.to_string(),
        owner: None,
        owner_name: None,
        color: None,
        population: 1_000_000,
        area_km2: 50_000,
        reach: Reach {
            cost: 1.0,
            mode: RouteMode::Land { through_rivals: 0 },
        },
        fort_level: 1,
        probability: Some(0.5),
        attack: false,
        mine,
        locked: None,
    }
}

/// Render one row at a given width and hand back the style of every cell.
fn cells(selected: bool, width: u16) -> Vec<Style> {
    let mut terminal = Terminal::new(TestBackend::new(width, 1)).expect("terminal");
    terminal
        .draw(|frame| {
            let area = frame.area();
            let line = target_line(
                &row("Poland", false),
                selected,
                Columns::fit(width as usize, true),
                width as usize,
            );
            frame.render_widget(ratatui::widgets::Paragraph::new(line), area);
        })
        .expect("draw");
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.style())
        .collect()
}

/// Does this cell carry the row selection? Compared field by field rather
/// than whole: cells keep their own fg and any bold the column asked for, so
/// only the part `row_style` contributes is the part to look at. Palettes
/// with an unpaintable canvas invert instead of filling, which is the other
/// arm.
fn is_selected(style: &Style, marked: &Style) -> bool {
    use ratatui::style::Modifier;
    if marked.add_modifier.contains(Modifier::REVERSED) {
        return style.add_modifier.contains(Modifier::REVERSED);
    }
    marked.bg.is_some() && style.bg == marked.bg
}

/// The selection has to read as a *row*. A highlight that stops after the
/// last column says "these characters are selected" when what is selected is
/// the territory, and the table is six columns wide.
#[test]
fn the_selected_row_is_filled_edge_to_edge() {
    let marked = theme::row_style(true);
    let picked = cells(true, 90);
    assert_eq!(picked.len(), 90);
    assert_eq!(
        picked.iter().filter(|s| is_selected(s, &marked)).count(),
        90,
        "every cell of the row carries the selection, including the empty tail"
    );

    // And an unselected row carries none of it.
    let plain = cells(false, 90);
    assert_eq!(
        plain.iter().filter(|s| is_selected(s, &marked)).count(),
        0,
        "an unselected row should not be filled"
    );
}

/// Narrow terminals drop columns; the fill still has to cover what is left.
#[test]
fn the_fill_follows_the_width() {
    let marked = theme::row_style(true);
    for width in [40u16, 60, 120] {
        let filled = cells(true, width)
            .iter()
            .filter(|s| is_selected(s, &marked))
            .count();
        assert_eq!(filled, width as usize, "at {width} columns");
    }
}
