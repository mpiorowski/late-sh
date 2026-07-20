use crate::app::icon_picker::picker::*;

fn make_entry(name: &str) -> IconEntry {
    IconEntry {
        icon: "x".to_string(),
        name: name.to_string(),
        name_lower: name.to_lowercase(),
    }
}

fn two_section_view() -> (Vec<IconEntry>, Vec<IconEntry>) {
    let a = vec![make_entry("a0"), make_entry("a1")];
    let b = vec![make_entry("b0"), make_entry("b1"), make_entry("b2")];
    (a, b)
}

fn views<'a>(a: &'a [IconEntry], b: &'a [IconEntry]) -> Vec<SectionView<'a>> {
    vec![
        SectionView {
            title: "A",
            entries: a.iter().collect(),
        },
        SectionView {
            title: "B",
            entries: b.iter().collect(),
        },
    ]
}

#[test]
fn flat_len_counts_headers_plus_entries() {
    let (a, b) = two_section_view();
    let sections = views(&a, &b);
    assert_eq!(flat_len(&sections), 7);
    assert_eq!(selectable_count(&sections), 5);
}

#[test]
fn selectable_to_flat_skips_headers() {
    let (a, b) = two_section_view();
    let sections = views(&a, &b);
    assert_eq!(selectable_to_flat(&sections, 0), Some(1));
    assert_eq!(selectable_to_flat(&sections, 1), Some(2));
    assert_eq!(selectable_to_flat(&sections, 2), Some(4));
    assert_eq!(selectable_to_flat(&sections, 3), Some(5));
    assert_eq!(selectable_to_flat(&sections, 4), Some(6));
    assert_eq!(selectable_to_flat(&sections, 5), None);
}

#[test]
fn flat_to_selectable_returns_none_for_headers() {
    let (a, b) = two_section_view();
    let sections = views(&a, &b);
    assert_eq!(flat_to_selectable(&sections, 0), None);
    assert_eq!(flat_to_selectable(&sections, 1), Some(0));
    assert_eq!(flat_to_selectable(&sections, 2), Some(1));
    assert_eq!(flat_to_selectable(&sections, 3), None);
    assert_eq!(flat_to_selectable(&sections, 4), Some(2));
    assert_eq!(flat_to_selectable(&sections, 6), Some(4));
    assert_eq!(flat_to_selectable(&sections, 7), None);
}

#[test]
fn flat_selectable_round_trip() {
    let (a, b) = two_section_view();
    let sections = views(&a, &b);
    for selectable in 0..selectable_count(&sections) {
        let flat = selectable_to_flat(&sections, selectable).unwrap();
        assert_eq!(flat_to_selectable(&sections, flat), Some(selectable));
    }
}

#[test]
fn entry_at_selectable_crosses_section_boundary() {
    let (a, b) = two_section_view();
    let sections = views(&a, &b);
    assert_eq!(entry_at_selectable(&sections, 0).unwrap().name, "a0");
    assert_eq!(entry_at_selectable(&sections, 2).unwrap().name, "b0");
    assert_eq!(entry_at_selectable(&sections, 4).unwrap().name, "b2");
    assert!(entry_at_selectable(&sections, 5).is_none());
}

#[test]
fn selected_chat_icon_wraps_kaomoji_as_inline_code() {
    let catalog = IconCatalogData::load();
    let mut state = IconPickerState::default();
    state.set_tab(IconPickerTab::Kaomoji);
    for ch in "happy smile".chars() {
        state.search_insert_char(ch);
    }

    assert_eq!(
        selected_icon(&state, &catalog).as_deref(),
        Some("(* ^ ω ^)")
    );
    assert_eq!(
        selected_chat_icon(&state, &catalog).as_deref(),
        Some("`(* ^ ω ^)`")
    );
}

#[test]
fn selected_chat_icon_uses_longer_code_fence_for_backtick_kaomoji() {
    let catalog = IconCatalogData::load();
    let mut state = IconPickerState::default();
    state.set_tab(IconPickerTab::Kaomoji);
    for ch in "table flip".chars() {
        state.search_insert_char(ch);
    }

    assert_eq!(
        selected_icon(&state, &catalog).as_deref(),
        Some("(╯`Д´)╯︵ ┻━┻")
    );
    assert_eq!(
        selected_chat_icon(&state, &catalog).as_deref(),
        Some("``(╯`Д´)╯︵ ┻━┻``")
    );
}

#[test]
fn selected_chat_icon_leaves_non_kaomoji_raw() {
    let catalog = IconCatalogData::load();
    let state = IconPickerState::default();

    assert_eq!(selected_icon(&state, &catalog).as_deref(), Some("👍"));
    assert_eq!(selected_chat_icon(&state, &catalog).as_deref(), Some("👍"));
}

#[test]
fn tab_navigation_cycles_forward_and_back() {
    let mut state = IconPickerState::default();
    state.next_tab();
    assert_eq!(state.tab, IconPickerTab::Kaomoji);
    state.next_tab();
    assert_eq!(state.tab, IconPickerTab::Unicode);
    state.next_tab();
    assert_eq!(state.tab, IconPickerTab::NerdFont);
    state.next_tab();
    assert_eq!(state.tab, IconPickerTab::Emoji);
    state.prev_tab();
    assert_eq!(state.tab, IconPickerTab::NerdFont);
}
