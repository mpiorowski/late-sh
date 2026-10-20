use super::*;

/// The fill says whose a place is and how dug in they are, both in
/// shade. If the selection spoke in shade too they would be read for
/// each other — so it is an outline, and the same outline whatever the
/// country underneath looks like.
#[test]
fn the_selection_outline_does_not_depend_on_the_shade_under_it() {
    let mut colors = std::collections::BTreeMap::new();
    colors.insert(1u16, (0, 132, 63)); // dug in: the dark end of a ladder
    colors.insert(2u16, (156, 233, 176)); // open ground: the pale end
    let edge_a = cell_color(1, &colors, Mark::SelectionEdge);
    let edge_b = cell_color(2, &colors, Mark::SelectionEdge);
    assert_eq!(edge_a, edge_b, "one selection colour, not two");
    let (sr, sg, sb) = SELECTION_COLOR;
    assert_eq!(edge_a, Color::Rgb(sr, sg, sb));
    // And the inside of the selected country keeps its own fill, so the
    // fortification shading is still readable while it is selected.
    assert_eq!(cell_color(1, &colors, Mark::None), Color::Rgb(0, 132, 63));
    assert_ne!(
        cell_color(1, &colors, Mark::None),
        cell_color(2, &colors, Mark::None),
    );
}
