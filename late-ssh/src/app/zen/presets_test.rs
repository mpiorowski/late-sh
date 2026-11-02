use serde_json::json;

use super::{LayoutList, Layouts, LayoutsError, MAX_LAYOUTS, STOCK_HOME_NAME, STOCK_ZEN_NAME};
use crate::app::zen::state::{Node, RiceLayout, TileKind};

fn chat_alone() -> RiceLayout {
    RiceLayout {
        root: Node::leaf(TileKind::Chat),
        ..RiceLayout::home()
    }
}

#[test]
fn a_device_starts_with_stock_home_current_and_zen_second() {
    let layouts = Layouts::stock();
    assert_eq!(layouts.current(), 0);
    assert_eq!(layouts.current_name(), STOCK_HOME_NAME);
    assert_eq!(layouts.current_layout(), &RiceLayout::home());
    assert_eq!(
        layouts.get(1).map(|item| item.name.as_str()),
        Some(STOCK_ZEN_NAME)
    );
    assert_eq!(
        layouts.get(1).map(|item| &item.layout),
        Some(&RiceLayout::default())
    );
    assert_eq!(layouts.count(), 2);
}

#[test]
fn a_device_that_arranged_its_pages_before_names_keeps_them_as_home_and_zen() {
    let home = chat_alone();
    let zen = RiceLayout {
        root: Node::leaf(TileKind::Bonsai),
        ..RiceLayout::default()
    };
    let layouts = Layouts::seeded(Some(&home.to_json()), Some(&zen.to_json()));
    assert_eq!(layouts.current_layout(), &home);
    assert_eq!(layouts.get(1).map(|item| &item.layout), Some(&zen));
    // A page that no longer parses reads as its stock, not as nothing.
    let layouts = Layouts::seeded(Some(&json!({ "root": 7 })), None);
    assert_eq!(layouts.current_layout(), &RiceLayout::home());
    assert_eq!(
        layouts.get(1).map(|item| &item.layout),
        Some(&RiceLayout::default())
    );
}

#[test]
fn a_stored_set_round_trips_and_a_broken_one_falls_back() {
    let mut layouts = Layouts::stock();
    layouts
        .save_new("stream", chat_alone())
        .expect("room for a third");
    let json = layouts.to_json();
    assert_eq!(
        Layouts::from_json_or(Some(&json), Layouts::stock()),
        layouts
    );

    let fallback = Layouts::seeded(Some(&chat_alone().to_json()), None);
    assert_eq!(
        Layouts::from_json_or(None, fallback.clone()),
        fallback,
        "no blob is the fallback"
    );
    assert_eq!(
        Layouts::from_json_or(
            Some(&json!({ "current": 0, "items": [] })),
            fallback.clone()
        ),
        fallback,
        "an empty set is the fallback"
    );
    let past_the_end = json!({
        "current": 2,
        "items": [{ "name": "a", "layout": chat_alone().to_json() }]
    });
    assert_eq!(
        Layouts::from_json_or(Some(&past_the_end), fallback.clone()),
        fallback,
        "a current past its items is the fallback"
    );
}

#[test]
fn saving_names_the_page_makes_it_current_and_stops_at_nine() {
    let mut layouts = Layouts::stock();
    let index = layouts.save_new("  Stream  ", chat_alone()).expect("saved");
    assert_eq!(index, 2);
    assert_eq!(layouts.current(), 2);
    assert_eq!(
        layouts.current_name(),
        "Stream",
        "the name is trimmed, case kept"
    );
    assert_eq!(layouts.current_layout(), &chat_alone());
    assert_eq!(layouts.index_of("stream"), Some(2), "found case aside");

    assert_eq!(
        layouts.save_new("stream", chat_alone()),
        Err(LayoutsError::NameTaken)
    );
    assert_eq!(
        layouts.save_new("   ", chat_alone()),
        Err(LayoutsError::BlankName)
    );
    assert_eq!(
        layouts.save_new("seventeen chars!!", chat_alone()),
        Err(LayoutsError::LongName)
    );
    for n in layouts.count()..MAX_LAYOUTS {
        layouts
            .save_new(&format!("l{n}"), chat_alone())
            .expect("room up to nine");
    }
    assert!(layouts.is_full());
    assert_eq!(
        layouts.save_new("ten", chat_alone()),
        Err(LayoutsError::Full)
    );
}

#[test]
fn renaming_keeps_names_unique_but_a_layout_may_keep_its_own() {
    let mut layouts = Layouts::stock();
    assert_eq!(
        layouts.rename(1, "zen"),
        Ok(()),
        "its own name is not taken"
    );
    assert_eq!(layouts.rename(1, "home"), Err(LayoutsError::NameTaken));
    assert_eq!(layouts.rename(1, "focus"), Ok(()));
    assert_eq!(layouts.get(1).map(|item| item.name.as_str()), Some("focus"));
    assert_eq!(layouts.rename(5, "x"), Err(LayoutsError::NoSuchLayout));
}

#[test]
fn deleting_moves_the_current_onto_a_neighbor_and_never_empties_the_set() {
    let mut layouts = Layouts::stock();
    layouts.save_new("third", chat_alone()).expect("saved");
    assert_eq!(layouts.current(), 2);
    // Deleting a layout before the current one shifts the index with it.
    assert_eq!(layouts.delete(0), Ok(()));
    assert_eq!(layouts.current(), 1);
    assert_eq!(layouts.current_name(), "third");
    // Deleting the current, last one lands on the one before it.
    assert_eq!(layouts.delete(1), Ok(()));
    assert_eq!(layouts.current(), 0);
    assert_eq!(layouts.current_name(), STOCK_ZEN_NAME);
    assert_eq!(layouts.delete(0), Err(LayoutsError::LastOne));
    assert_eq!(layouts.delete(3), Err(LayoutsError::NoSuchLayout));
}

#[test]
fn reset_puts_back_the_stock_that_matches_the_name() {
    let mut layouts = Layouts::stock();
    layouts.store_current(chat_alone());
    assert_eq!(layouts.current_layout(), &chat_alone());
    assert!(layouts.reset(0));
    assert_eq!(layouts.current_layout(), &RiceLayout::home());
    layouts.switch(1);
    layouts.store_current(chat_alone());
    assert!(layouts.reset(1));
    assert_eq!(
        layouts.current_layout(),
        &RiceLayout::default(),
        "zen resets to the stock zen layout"
    );
    assert!(!layouts.reset(7));
    assert!(!layouts.switch(7));
    assert_eq!(layouts.current(), 1);
}

#[test]
fn the_list_selection_stays_inside_the_rows() {
    let mut list = LayoutList::open(1);
    list.move_by(5, 3);
    assert_eq!(list.selected, 2);
    list.move_by(-9, 3);
    assert_eq!(list.selected, 0);
    list.move_by(1, 0);
    assert_eq!(list.selected, 0);
}

#[test]
fn the_layout_command_reads_list_load_and_save() {
    use super::LayoutCommand;
    assert_eq!(LayoutCommand::parse("/layout"), Some(LayoutCommand::List));
    assert_eq!(
        LayoutCommand::parse("  /layout  "),
        Some(LayoutCommand::List)
    );
    assert_eq!(
        LayoutCommand::parse("/layout stream"),
        Some(LayoutCommand::Load("stream".to_string()))
    );
    assert_eq!(
        LayoutCommand::parse("/layout 2"),
        Some(LayoutCommand::Load("2".to_string()))
    );
    assert_eq!(
        LayoutCommand::parse("/layout save my stream"),
        Some(LayoutCommand::Save("my stream".to_string()))
    );
    assert_eq!(
        LayoutCommand::parse("/layout save"),
        Some(LayoutCommand::Save(String::new())),
        "a save with no name is refused by the saver, not the parser"
    );
    assert_eq!(
        LayoutCommand::parse("/layout saved"),
        Some(LayoutCommand::Load("saved".to_string())),
        "a layout may be called saved"
    );
    assert_eq!(LayoutCommand::parse("/layouts"), None);
    assert_eq!(LayoutCommand::parse("/lay"), None);
}
