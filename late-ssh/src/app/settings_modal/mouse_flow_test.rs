use std::time::Duration;

use late_core::models::{profile::Profile, rss_feed::RssFeed, user::InteractionMode};
use late_core::test_utils::{TestDb, create_test_user};
use ratatui::{Terminal, backend::TestBackend, buffer::Buffer};

use super::mouse::{Field, Target};
use super::state::{AccountRow, Row, StatuslinePane, Tab, TweakRow};
use crate::app::common::sidebar::SidebarOwnership;
use crate::app::state::App;
use crate::test_helpers::{make_app, new_test_db, wait_until};

async fn fixture() -> (TestDb, App) {
    let db = new_test_db().await;
    let user = create_test_user(&db.db, "mouse-user").await;
    let mut app = make_app(db.db.clone(), user.id, "settings-mouse-test");
    app.interaction_mode = InteractionMode::Hybrid;
    let client = db.db.get().await.unwrap();
    let profile = Profile::load(&client, user.id).await.unwrap();
    app.settings_modal_state
        .open_from_profile(&profile, app.rail_modes());
    app.settings_modal_state
        .set_interaction_mode_display(app.interaction_mode);
    app.show_settings = true;
    (db, app)
}

fn paint(app: &App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(app.size.0, app.size.1)).unwrap();
    terminal
        .draw(|frame| {
            let area = frame.area();
            super::ui::draw(
                frame,
                area,
                &app.settings_modal_state,
                SidebarOwnership {
                    pet: true,
                    tank: true,
                },
            );
            if app.tag_picker.is_open() {
                crate::app::tag_picker::ui::draw(frame, area, &app.tag_picker);
            }
        })
        .unwrap();
    terminal.backend().buffer().clone()
}

fn hits(app: &App) -> Vec<(ratatui::layout::Rect, Target)> {
    use crate::app::tag_picker::state::Target as TagTarget;
    if app.tag_picker.is_open() {
        // The tag picker has its own targets; name them in Settings terms so
        // one `click` helper drives both surfaces.
        app.tag_picker
            .mouse
            .hits()
            .into_iter()
            .map(|(rect, target)| match target {
                TagTarget::Done => (rect, Target::Close),
                TagTarget::Row(index) => (rect, Target::Pick(index)),
            })
            .collect()
    } else {
        app.settings_modal_state.mouse.hits()
    }
}

fn click(app: &mut App, target: Target) {
    paint(app);
    let rect = hits(app)
        .iter()
        .rev()
        .find_map(|(rect, hit)| (*hit == target).then_some(*rect))
        .unwrap_or_else(|| panic!("no visible target {target:?}: {:?}", hits(app)));
    app.handle_input(format!("\x1b[<0;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
}

fn wheel(app: &mut App, target: Target, down: bool) {
    paint(app);
    let rect = hits(app)
        .iter()
        .find_map(|(rect, hit)| (*hit == target).then_some(*rect))
        .unwrap();
    app.handle_input(
        format!(
            "\x1b[<{};{};{}M",
            if down { 65 } else { 64 },
            rect.x + 1,
            rect.y + 1
        )
        .as_bytes(),
    );
}

async fn settle(app: &mut App, condition: impl Fn(&App) -> bool) {
    for _ in 0..1500 {
        app.tick();
        if condition(app) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("condition did not settle");
}

fn text(buffer: &Buffer) -> String {
    buffer
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[tokio::test]
async fn settings_cycle_values_share_arrows_color_and_weight() {
    use ratatui::{style::Modifier, text::Span};

    use crate::app::common::theme;

    let (_db, mut app) = fixture().await;
    app.settings_modal_state.select_tab(Tab::Tweaks);
    for theme_id in ["contrast", "latte"] {
        theme::set_current_by_id(theme_id);
        for size in [(120, 40), (48, 14)] {
            app.resize(size.0, size.1).unwrap();
            for row in [
                TweakRow::TextBrightness,
                TweakRow::TerminalImages,
                TweakRow::LandingPage,
                TweakRow::ArtSplash,
                TweakRow::Screensaver,
            ] {
                app.settings_modal_state
                    .select_mouse_target(Target::Tweak(row));
                for selected in [true, false] {
                    if !selected {
                        app.settings_modal_state
                            .select_mouse_target(Target::Tweak(TweakRow::PaperAtLogin));
                    } else {
                        app.settings_modal_state.mouse.reveal_selection();
                    }
                    let buffer = paint(&app);
                    let rect = hits(&app)
                        .into_iter()
                        .find_map(|(rect, target)| (target == Target::Tweak(row)).then_some(rect))
                        .unwrap();
                    let expected = match row {
                        TweakRow::TextBrightness => "◂ neutral    ▸",
                        TweakRow::TerminalImages => "◂ Auto  ▸",
                        TweakRow::LandingPage => "◂ Clubhouse ▸",
                        TweakRow::ArtSplash => "◂ SFW    ▸",
                        TweakRow::Screensaver => "◂ Aurora fjord ▸",
                        _ => unreachable!(),
                    };
                    let x = (rect.x..rect.right())
                        .find(|x| buffer[(*x, rect.y)].symbol() == "◂")
                        .unwrap();
                    let end = x + Span::raw(expected).width() as u16;
                    assert_eq!(rect.right(), x + Span::raw(expected).width() as u16);
                    let rendered: String = (x..end).map(|x| buffer[(x, rect.y)].symbol()).collect();
                    assert_eq!(rendered, expected);
                    let expected_style = ratatui::style::Style::default()
                        .fg(theme::AMBER())
                        .add_modifier(Modifier::BOLD);
                    let expected_style = if selected {
                        expected_style.patch(theme::selection_style())
                    } else {
                        expected_style
                    };
                    for x in x..end {
                        let cell = &buffer[(x, rect.y)];
                        assert_eq!(cell.fg, expected_style.fg.unwrap());
                        assert!(cell.modifier.contains(Modifier::BOLD));
                    }
                }
                let before = hits(&app)
                    .into_iter()
                    .find_map(|(rect, target)| (target == Target::Tweak(row)).then_some(rect))
                    .unwrap();
                for forward in [true, true, true, false, false, false] {
                    let current_value = |app: &App| match row {
                        TweakRow::TextBrightness => app
                            .settings_modal_state
                            .draft()
                            .text_brightness_adjustment
                            .to_string(),
                        TweakRow::TerminalImages => app
                            .settings_modal_state
                            .draft()
                            .terminal_images
                            .as_str()
                            .to_string(),
                        TweakRow::LandingPage => app
                            .settings_modal_state
                            .draft()
                            .landing_page
                            .as_str()
                            .to_string(),
                        TweakRow::ArtSplash => app
                            .settings_modal_state
                            .draft()
                            .art_splash_mode
                            .as_str()
                            .to_string(),
                        TweakRow::Screensaver => app
                            .settings_modal_state
                            .draft()
                            .screensaver
                            .as_str()
                            .to_string(),
                        _ => unreachable!(),
                    };
                    let value_before = current_value(&app);
                    click(&mut app, Target::TweakCycle(row, forward));
                    assert_ne!(current_value(&app), value_before);
                    paint(&app);
                    let after = hits(&app)
                        .into_iter()
                        .find_map(|(rect, target)| (target == Target::Tweak(row)).then_some(rect))
                        .unwrap();
                    assert_eq!(before, after);
                }
            }
        }
    }
    theme::set_current_by_id(theme::DEFAULT_ID);
}

#[tokio::test]
async fn settings_mouse_brightness_arrows_clamp_and_wheel_preserves_value() {
    let (_db, mut app) = fixture().await;
    app.resize(48, 14).unwrap();
    app.settings_modal_state.select_tab(Tab::Tweaks);
    app.settings_modal_state
        .select_mouse_target(Target::Tweak(TweakRow::TextBrightness));
    app.settings_modal_state.mouse.reveal_selection();
    for expected in [-1, -2, -3, -4, -5, -5] {
        click(
            &mut app,
            Target::TweakCycle(TweakRow::TextBrightness, false),
        );
        assert_eq!(
            app.settings_modal_state.draft().text_brightness_adjustment,
            expected
        );
    }
    for expected in -4..=6 {
        click(&mut app, Target::TweakCycle(TweakRow::TextBrightness, true));
        assert_eq!(
            app.settings_modal_state.draft().text_brightness_adjustment,
            expected.min(5)
        );
    }
    wheel(&mut app, Target::Tweak(TweakRow::TextBrightness), true);
    assert_eq!(
        app.settings_modal_state.draft().text_brightness_adjustment,
        5
    );
    assert_eq!(
        app.settings_modal_state.selected_tweak_row(),
        TweakRow::TextBrightness
    );
    app.handle_input(b"\x1b[D");
    assert_eq!(
        app.settings_modal_state.draft().text_brightness_adjustment,
        4
    );
}

#[tokio::test]
async fn settings_mouse_rendering_covers_every_tab_and_short_scrolled_rows() {
    let (_db, mut app) = fixture().await;
    for size in [(120, 40), (48, 14)] {
        app.resize(size.0, size.1).unwrap();
        for tab in Tab::ALL {
            app.settings_modal_state.select_tab(tab);
            let buffer = paint(&app);
            let all = hits(&app);
            for (rect, _) in &all {
                assert!(!rect.is_empty());
                assert!(rect.right() <= size.0 && rect.bottom() <= size.1);
            }
            for tab in Tab::ALL {
                assert!(all.iter().any(|(_, target)| *target == Target::Tab(tab)));
            }
            assert!(all.iter().any(|(_, target)| *target == Target::Close));
            assert!(!text(&buffer).is_empty());
        }
        app.settings_modal_state.select_tab(Tab::Settings);
        let mut seen = Vec::new();
        for _ in 0..12 {
            paint(&app);
            let all = hits(&app);
            seen.extend(all.iter().filter_map(|(_, target)| {
                if let Target::Row(row) = target {
                    Some(*row)
                } else {
                    None
                }
            }));
            let rect = all
                .iter()
                .find_map(|(rect, target)| matches!(target, Target::Row(_)).then_some(*rect))
                .unwrap();
            app.handle_input(format!("\x1b[<65;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
        }
        for row in Row::ALL {
            assert!(seen.contains(&row), "{size:?}: missing {row:?}");
        }
    }
}

#[tokio::test]
async fn settings_mouse_picker_filter_languages_and_foreground_priority() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Row(Row::Country));
    let old = app.settings_modal_state.draft().country.clone();
    wheel(&mut app, Target::Pick(0), true);
    assert_eq!(app.settings_modal_state.picker().selected_index, 0);
    assert_eq!(app.settings_modal_state.draft().country, old);
    paint(&app);
    assert!(
        !hits(&app)
            .iter()
            .any(|(_, target)| matches!(target, Target::Tab(_)))
    );
    let index = hits(&app)
        .iter()
        .find_map(|(_, target)| {
            if let Target::Pick(index) = target {
                Some(*index)
            } else {
                None
            }
        })
        .unwrap();
    click(&mut app, Target::Pick(index));
    assert!(!app.settings_modal_state.picker_open());
    assert!(app.settings_modal_state.draft().country.is_some());
    click(&mut app, Target::Row(Row::Timezone));
    app.handle_input(b"nonsense-zone");
    paint(&app);
    assert!(
        !hits(&app)
            .iter()
            .any(|(_, target)| matches!(target, Target::Pick(_)))
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Row(Row::Langs));
    let index = hits_after_paint(&app)
        .iter()
        .find_map(|(_, target)| {
            if let Target::Pick(index) = target {
                Some(*index)
            } else {
                None
            }
        })
        .unwrap();
    click(&mut app, Target::Pick(index));
    assert_eq!(app.tag_picker.chosen().len(), 1);
    let cursor = app.tag_picker.cursor();
    wheel(&mut app, Target::Pick(index), true);
    assert_eq!(app.tag_picker.cursor(), cursor);
    assert_eq!(app.tag_picker.chosen().len(), 1);
    click(&mut app, Target::Close);
    assert!(!app.tag_picker.is_open());
    assert_eq!(app.settings_modal_state.draft().langs.len(), 1);
}

fn hits_after_paint(app: &App) -> Vec<(ratatui::layout::Rect, Target)> {
    paint(app);
    hits(app)
}

#[tokio::test]
async fn settings_mouse_toggles_reorders_and_scrolls_status_panes_independently() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Tweaks));
    let original = app.settings_modal_state.draft().enable_background_color;
    click(&mut app, Target::Tweak(TweakRow::BackgroundColor));
    assert_eq!(
        app.settings_modal_state.draft().enable_background_color,
        !original
    );
    let rails = app.rail_modes();
    let default_sidebar = app.settings_modal_state.draft().right_sidebar_mode;
    click(&mut app, Target::SidebarMode);
    assert_ne!(app.rail_modes().1, rails.1);
    assert_eq!(app.settings_modal_state.device_rails(), app.rail_modes());
    assert_eq!(
        app.settings_modal_state.draft().right_sidebar_mode,
        default_sidebar
    );
    click(&mut app, Target::Tweak(TweakRow::RoomListSidebar));
    assert_ne!(app.rail_modes().0, rails.0);
    click(&mut app, Target::Tweak(TweakRow::RightSidebar));
    let components = app.settings_modal_state.right_sidebar_components().to_vec();
    click(&mut app, Target::SidebarMove(0, 1));
    assert_eq!(
        app.settings_modal_state.right_sidebar_components()[1],
        components[0]
    );
    click(&mut app, Target::SidebarMove(1, -1));
    assert_eq!(
        app.settings_modal_state.right_sidebar_components(),
        components
    );
    click(&mut app, Target::SidebarMove(0, 1));
    click(&mut app, Target::Sidebar(1));
    assert_ne!(
        app.settings_modal_state.right_sidebar_components()[1].enabled,
        components[0].enabled
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Tweak(TweakRow::ChatBadges));
    click(&mut app, Target::Badge(0));
    click(&mut app, Target::Close);
    click(&mut app, Target::Tab(Tab::Statusline));
    let components = app.settings_modal_state.statusline_components().to_vec();
    click(&mut app, Target::StatusMove(0, 1));
    assert_eq!(
        app.settings_modal_state.statusline_components()[1],
        components[0]
    );
    click(&mut app, Target::StatusMove(1, -1));
    assert_eq!(app.settings_modal_state.statusline_components(), components);
    click(&mut app, Target::StatusMove(0, 1));
    click(&mut app, Target::StatusToggle(1));
    assert_ne!(
        app.settings_modal_state.statusline_components()[1].enabled,
        components[0].enabled
    );
    click(&mut app, Target::Status(1));
    assert_eq!(
        app.settings_modal_state.statusline_pane(),
        StatuslinePane::Detail
    );
    let before = app.settings_modal_state.statusline_components().to_vec();
    click(&mut app, Target::Dial(0));
    assert_ne!(app.settings_modal_state.statusline_components(), before);
    app.resize(48, 10).unwrap();
    paint(&app);
    assert!(hits(&app).iter().any(|(_, hit)| *hit == Target::Dial(0)));
    app.resize(60, 14).unwrap();
    let before = app.settings_modal_state.statusline_components().to_vec();
    wheel(&mut app, Target::Status(1), true);
    assert_eq!(app.settings_modal_state.statusline_components(), before);
    assert_eq!(app.settings_modal_state.statusline_index(), 1);
    let list_hits = hits_after_paint(&app)
        .into_iter()
        .filter(|(_, hit)| matches!(hit, Target::Status(_)))
        .collect::<Vec<_>>();
    if hits(&app).iter().any(|(_, hit)| *hit == Target::Dial(0)) {
        wheel(&mut app, Target::Dial(0), true);
    }
    let after = hits_after_paint(&app)
        .into_iter()
        .filter(|(_, hit)| matches!(hit, Target::Status(_)))
        .collect::<Vec<_>>();
    assert_eq!(list_hits, after);
    assert_eq!(app.settings_modal_state.statusline_components(), before);
}

#[tokio::test]
async fn settings_mouse_click_away_submits_the_open_editor_and_cancel_discards_it() {
    let (db, mut app) = fixture().await;
    let original = app.settings_modal_state.draft().username.clone();
    click(&mut app, Target::Row(Row::Username));
    app.handle_input(b"\x15discarded");
    click(&mut app, Target::Cancel);
    assert!(!app.settings_modal_state.editing_username());
    assert_eq!(app.settings_modal_state.draft().username, original);

    click(&mut app, Target::Row(Row::Username));
    app.handle_input(b"\x15fresh-name");
    click(&mut app, Target::Row(Row::Ide));
    assert!(!app.settings_modal_state.editing_username());
    assert!(app.settings_modal_state.editing_system_field().is_some());
    app.handle_input(b"my editor");
    click(&mut app, Target::Tab(Tab::Bio));
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Bio);
    assert!(!app.settings_modal_state.editing_text());

    let user_id = app.user_id;
    wait_until(
        || async {
            let client = db.db.get().await.unwrap();
            let stored = Profile::load(&client, user_id).await.unwrap();
            stored.username == "fresh-name" && stored.ide.as_deref() == Some("my editor")
        },
        "click-away edits saved",
    )
    .await;
}

#[tokio::test]
async fn settings_mouse_bio_click_edits_and_done_saves_without_moving_the_caret() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Bio));
    click(&mut app, Target::Bio);
    assert!(app.settings_modal_state.editing_bio());
    app.handle_input("漢字 first line\nsecond line".as_bytes());
    app.handle_input(b"\x1b[D");
    let before = app.settings_modal_state.bio_input().lines().to_vec();
    let cursor = app.settings_modal_state.bio_input().cursor();
    // Inside the open editor a click neither saves nor relocates the caret,
    // and the wheel leaves the text alone.
    click(&mut app, Target::Bio);
    wheel(&mut app, Target::Bio, false);
    assert!(app.settings_modal_state.editing_bio());
    assert_eq!(app.settings_modal_state.bio_input().lines(), before);
    assert_eq!(app.settings_modal_state.bio_input().cursor(), cursor);
    click(&mut app, Target::Submit);
    assert!(!app.settings_modal_state.editing_bio());
    assert_eq!(app.settings_modal_state.draft().bio, before.join("\n"));
}

#[tokio::test]
async fn settings_mouse_rss_click_away_adds_the_feed_and_keeps_the_clicked_one_selected() {
    let (db, mut app) = fixture().await;
    let client = db.db.get().await.unwrap();
    let original = RssFeed::create_for_user(&client, app.user_id, "http://127.0.0.1:1/original")
        .await
        .unwrap();
    app.settings_modal_state.open_from_profile(
        &Profile::load(&client, app.user_id).await.unwrap(),
        app.rail_modes(),
    );
    drop(client);
    settle(&mut app, |app| app.settings_modal_state.feeds().len() == 1).await;
    click(&mut app, Target::Tab(Tab::Feeds));

    // An empty URL is dropped quietly, the way Enter drops it.
    click(&mut app, Target::AddFeed);
    click(&mut app, Target::Tab(Tab::Account));
    assert!(!app.settings_modal_state.editing_feed_url());
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Account);

    click(&mut app, Target::Tab(Tab::Feeds));
    click(&mut app, Target::AddFeed);
    app.handle_input(b"http://127.0.0.1:1/new");
    click(&mut app, Target::Feed(original.id));
    assert!(!app.settings_modal_state.editing_feed_url());
    let user_id = app.user_id;
    wait_until(
        || async {
            let client = db.db.get().await.unwrap();
            RssFeed::list_for_user(&client, user_id)
                .await
                .unwrap()
                .len()
                == 2
        },
        "subscription stored",
    )
    .await;
    click(&mut app, Target::RefreshFeeds);
    settle(&mut app, |app| app.settings_modal_state.feeds().len() == 2).await;
    assert_eq!(
        app.settings_modal_state.feeds()[app.settings_modal_state.feed_index()].id,
        original.id
    );
    wheel(&mut app, Target::Feed(original.id), true);
    assert_eq!(
        app.settings_modal_state.feeds()[app.settings_modal_state.feed_index()].id,
        original.id
    );
    click(&mut app, Target::RemoveFeed);
    settle(&mut app, |app| app.settings_modal_state.feeds().len() == 1).await;
}

#[tokio::test]
async fn settings_mouse_account_dialogs_require_typed_confirmation_and_block_pending_clicks() {
    let (db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Account));
    let buffer = paint(&app);
    assert!(text(&buffer).contains("IRC access token"));
    click(&mut app, Target::Account(AccountRow::LinkAccounts));
    click(&mut app, Target::GenerateCode);
    assert!(app.settings_modal_state.link_account_dialog().pending());
    click(&mut app, Target::Close);
    assert!(app.settings_modal_state.link_account_dialog().open());
    settle(&mut app, |app| {
        !app.settings_modal_state.link_account_dialog().pending()
    })
    .await;
    click(&mut app, Target::Caret(Field::LinkCode, 0));
    app.handle_input(b"invalid-code");
    click(&mut app, Target::LookupCode);
    settle(&mut app, |app| {
        !app.settings_modal_state.link_account_dialog().pending()
    })
    .await;
    assert!(
        app.settings_modal_state
            .link_account_dialog()
            .status()
            .is_some()
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Account(AccountRow::DeleteAccount));
    app.handle_input(b"wrong-name");
    click(&mut app, Target::ConfirmDelete);
    assert!(!app.settings_modal_state.delete_account_dialog().pending());
    assert!(
        app.settings_modal_state
            .delete_account_dialog()
            .status()
            .unwrap()
            .contains("does not match")
    );
    click(&mut app, Target::Close);
    click(&mut app, Target::Account(AccountRow::IrcToken));
    settle(&mut app, |app| {
        app.settings_modal_state
            .irc_token_dialog()
            .status()
            .is_some()
    })
    .await;
    click(&mut app, Target::Irc(super::state::IrcTokenFocus::Primary));
    assert!(app.settings_modal_state.irc_token_dialog().pending());
    click(&mut app, Target::Close);
    assert!(app.settings_modal_state.irc_token_dialog().open());
    settle(&mut app, |app| {
        app.settings_modal_state
            .irc_token_dialog()
            .revealed_token()
            .is_some()
    })
    .await;
    click(&mut app, Target::DismissToken);
    settle(&mut app, |app| {
        app.settings_modal_state.irc_token_dialog().has_token()
    })
    .await;
    click(&mut app, Target::Irc(super::state::IrcTokenFocus::Revoke));
    assert!(
        app.settings_modal_state
            .irc_token_dialog()
            .confirming_revoke()
    );
    click(&mut app, Target::Irc(super::state::IrcTokenFocus::Revoke));
    assert!(app.settings_modal_state.irc_token_dialog().pending());
    settle(&mut app, |app| {
        !app.settings_modal_state.irc_token_dialog().pending()
    })
    .await;
    assert!(!app.settings_modal_state.irc_token_dialog().has_token());
    click(&mut app, Target::Close);
    click(&mut app, Target::Account(AccountRow::DeleteAccount));
    app.handle_input(b"mouse-user");
    click(&mut app, Target::ConfirmDelete);
    assert!(app.settings_modal_state.delete_account_dialog().pending());
    let client = db.db.get().await.unwrap();
    crate::test_helpers::wait_until(
        || async {
            late_core::models::user::User::get(&client, app.user_id)
                .await
                .unwrap()
                .is_none()
        },
        "disposable account to be deleted",
    )
    .await;
}

#[tokio::test]
async fn settings_mouse_keyboard_only_mode_and_non_left_events_do_not_activate() {
    let (_db, mut app) = fixture().await;
    app.interaction_mode = InteractionMode::Keyboard;
    click(&mut app, Target::Tab(Tab::Themes));
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    app.interaction_mode = InteractionMode::Hybrid;
    paint(&app);
    let rect = hits(&app)
        .iter()
        .find_map(|(rect, hit)| (*hit == Target::Tab(Tab::Themes)).then_some(*rect))
        .unwrap();
    for (button, suffix) in [(2, "M"), (0, "m"), (32, "M")] {
        app.handle_input(
            format!("\x1b[<{button};{};{}{suffix}", rect.x + 1, rect.y + 1).as_bytes(),
        );
        assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    }
    app.resize(48, 14).unwrap();
    app.handle_input(format!("\x1b[<0;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Settings);
    app.handle_input(b"\t");
    assert_eq!(app.settings_modal_state.selected_tab(), Tab::Bio);
}

#[tokio::test]
async fn settings_mouse_theme_search_stars_groups_and_wheel_do_not_apply_accidentally() {
    use super::state::ThemeTreeRow;
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Themes));
    let group = app
        .settings_modal_state
        .theme_tree_rows()
        .iter()
        .position(|row| matches!(row, ThemeTreeRow::Group { .. }))
        .unwrap();
    let before = app.settings_modal_state.theme_tree_rows().len();
    click(&mut app, Target::Theme(group));
    assert_ne!(app.settings_modal_state.theme_tree_rows().len(), before);
    click(&mut app, Target::Search);
    app.handle_input(b"paper");
    let row = app
        .settings_modal_state
        .theme_tree_rows()
        .iter()
        .position(|row| matches!(row, ThemeTreeRow::Theme { .. }))
        .unwrap();
    let theme = app.settings_modal_state.draft().theme_id.clone();
    let selected = app.settings_modal_state.theme_selected_row();
    wheel(&mut app, Target::Theme(row), true);
    assert_eq!(app.settings_modal_state.theme_selected_row(), selected);
    assert_eq!(app.settings_modal_state.draft().theme_id, theme);
    let favorites = app.settings_modal_state.draft().favorite_theme_ids.clone();
    click(&mut app, Target::Star(row));
    assert_ne!(
        app.settings_modal_state.draft().favorite_theme_ids,
        favorites
    );
    click(&mut app, Target::Theme(row));
    assert_eq!(app.settings_modal_state.draft().theme_id, theme);
    app.handle_input(b"\x15no-theme-matches-this");
    paint(&app);
    assert!(
        !hits(&app)
            .iter()
            .any(|(_, hit)| matches!(hit, Target::Theme(_) | Target::Star(_)))
    );
}

#[tokio::test]
async fn settings_mouse_invites_code_field_takes_the_caret_and_its_button_submits() {
    let (_db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Account));
    click(&mut app, Target::Account(AccountRow::Invites));
    settle(&mut app, |app| {
        app.settings_modal_state.invites_dialog().accepts_code()
    })
    .await;
    app.handle_input(b"??");
    click(&mut app, Target::Caret(Field::InviteCode, 0));
    assert_eq!(
        app.settings_modal_state
            .invites_dialog()
            .code_input()
            .cursor(),
        (0, 0)
    );
    click(&mut app, Target::AddInviteCode);
    assert_eq!(
        app.settings_modal_state.invites_dialog().message(),
        Some(("That does not look like an invite code.", true))
    );
}

#[tokio::test]
async fn settings_mouse_close_during_theme_search_saves_the_previewed_theme() {
    use crate::app::common::theme;
    let (db, mut app) = fixture().await;
    click(&mut app, Target::Tab(Tab::Themes));
    let original = app.settings_modal_state.draft().theme_id.clone();
    click(&mut app, Target::Search);
    // Typing previews the first match; nothing is clicked or confirmed.
    app.handle_input(theme::OPTIONS[theme::OPTIONS.len() - 1].label.as_bytes());
    let previewed = app.settings_modal_state.draft().theme_id.clone();
    assert_ne!(previewed, original);
    click(&mut app, Target::Close);
    assert!(!app.show_settings);

    let client = db.db.get().await.unwrap();
    let user_id = app.user_id;
    wait_until(
        || async { Profile::load(&client, user_id).await.unwrap().theme_id == previewed },
        "previewed theme saved",
    )
    .await;
}

#[tokio::test]
async fn settings_mouse_link_confirmation_choices_fields_and_submit_on_short_terminal() {
    use late_core::models::account_link;
    use late_core::models::user::User;
    let (db, mut app) = fixture().await;
    let other = create_test_user(&db.db, "peer-user").await;
    let client = db.db.get().await.unwrap();
    let (code, _) = account_link::create_code(&client, other.id).await.unwrap();
    click(&mut app, Target::Tab(Tab::Account));
    click(&mut app, Target::Account(AccountRow::LinkAccounts));
    click(&mut app, Target::Caret(Field::LinkCode, 0));
    app.handle_input(code.as_bytes());
    click(&mut app, Target::LookupCode);
    settle(&mut app, |app| {
        !app.settings_modal_state.link_account_dialog().pending()
    })
    .await;
    assert_eq!(
        app.settings_modal_state.link_account_dialog().step(),
        super::state::LinkAccountStep::Confirm
    );
    click(&mut app, Target::KeepAccount(false));
    assert!(
        !app.settings_modal_state
            .link_account_dialog()
            .keep_current()
    );
    click(&mut app, Target::KeepAccount(true));
    assert!(
        app.settings_modal_state
            .link_account_dialog()
            .keep_current()
    );
    click(&mut app, Target::ConfirmLink);
    assert!(!app.settings_modal_state.link_account_dialog().pending());
    assert!(
        app.settings_modal_state
            .link_account_dialog()
            .status()
            .unwrap()
            .contains("does not match")
    );
    click(&mut app, Target::Caret(Field::LinkConfirm, 0));
    app.handle_input(b"mouse-user");
    app.resize(48, 14).unwrap();
    paint(&app);
    for _ in 0..8 {
        if hits(&app)
            .iter()
            .any(|(_, hit)| *hit == Target::ConfirmLink)
        {
            break;
        }
        let rect = hits(&app)
            .iter()
            .find_map(|(rect, hit)| {
                matches!(hit, Target::Caret(_, _) | Target::KeepAccount(_)).then_some(*rect)
            })
            .unwrap();
        app.handle_input(format!("\x1b[<65;{};{}M", rect.x + 1, rect.y + 1).as_bytes());
        paint(&app);
    }
    click(&mut app, Target::ConfirmLink);
    assert!(app.settings_modal_state.link_account_dialog().pending());
    click(&mut app, Target::Close);
    assert!(app.settings_modal_state.link_account_dialog().open());
    crate::test_helpers::wait_until(
        || async { User::get(&client, other.id).await.unwrap().is_none() },
        "disposable peer to be linked",
    )
    .await;
}

fn rect_for(app: &App, target: Target) -> ratatui::layout::Rect {
    hits(app)
        .into_iter()
        .rev()
        .find_map(|(rect, hit)| (hit == target).then_some(rect))
        .unwrap()
}

fn rendered_row(buffer: &Buffer, rect: ratatui::layout::Rect) -> String {
    let mut text = String::new();
    let mut x = rect.x;
    while x < rect.right() {
        let symbol = buffer[(x, rect.y)].symbol();
        text.push_str(symbol);
        x += ratatui::text::Span::raw(symbol).width().max(1) as u16;
    }
    text
}

#[tokio::test]
async fn notification_choices_render_every_value_with_stable_directional_targets() {
    let (_db, mut app) = fixture().await;
    let full_width = text(&paint(&app));
    assert!(full_width.contains("Auto-translate new messages"));
    assert!(full_width.contains("Translate my messages to English"));
    assert!(full_width.contains("Streams (friends live, your viewers)"));
    for size in [(120, 40), (48, 14)] {
        app.resize(size.0, size.1).unwrap();
        for (row, count) in [(Row::Cooldown, 10), (Row::NotifyFormat, 3)] {
            app.settings_modal_state
                .select_mouse_target(Target::Row(row));
            app.settings_modal_state.mouse.reveal_selection();
            paint(&app);
            let left = rect_for(&app, Target::RowCycle(row, false));
            let right = rect_for(&app, Target::RowCycle(row, true));
            let original = (
                app.settings_modal_state.draft().notify_cooldown_mins,
                app.settings_modal_state.draft().notify_format.clone(),
            );
            for forward in [true, false] {
                for _ in 0..count {
                    let buffer = paint(&app);
                    assert_eq!(rect_for(&app, Target::RowCycle(row, false)), left);
                    assert_eq!(rect_for(&app, Target::RowCycle(row, true)), right);
                    assert_eq!(buffer[(left.x, left.y)].symbol(), "◂");
                    assert_eq!(buffer[(right.x + 1, right.y)].symbol(), "▸");
                    let label = if row == Row::Cooldown {
                        let mins = app.settings_modal_state.draft().notify_cooldown_mins;
                        if mins == 0 {
                            "off".to_string()
                        } else {
                            format!("{mins} min")
                        }
                    } else {
                        match app.settings_modal_state.draft().notify_format.as_deref() {
                            Some("osc777") => "OSC 777",
                            Some("osc9") => "OSC 9",
                            _ => "Both (777 + 9)",
                        }
                        .to_string()
                    };
                    assert!(
                        rendered_row(&buffer, rect_for(&app, Target::Row(row))).contains(&label)
                    );
                    let before = (
                        app.settings_modal_state.draft().notify_cooldown_mins,
                        app.settings_modal_state.draft().notify_format.clone(),
                    );
                    wheel(&mut app, Target::Row(row), false);
                    assert_eq!(
                        (
                            app.settings_modal_state.draft().notify_cooldown_mins,
                            app.settings_modal_state.draft().notify_format.clone()
                        ),
                        before
                    );
                    app.settings_modal_state.mouse.reveal_selection();
                    click(&mut app, Target::RowCycle(row, forward));
                    assert_eq!(app.settings_modal_state.selected_row(), row);
                }
            }
            assert_eq!(
                app.settings_modal_state.draft().notify_cooldown_mins,
                original.0
            );
            if original.1.is_some() {
                assert_eq!(app.settings_modal_state.draft().notify_format, original.1);
            }
        }
    }
}

#[tokio::test]
async fn language_chooser_selects_current_filters_native_names_and_codes_and_cancels() {
    use late_core::models::message_translation::TranslateLang;
    let (_db, mut app) = fixture().await;
    for size in [(120, 40), (48, 14)] {
        app.resize(size.0, size.1).unwrap();
        for (index, lang) in TranslateLang::ALL.into_iter().enumerate() {
            app.settings_modal_state
                .select_mouse_target(Target::Row(Row::TranslateTo));
            app.settings_modal_state.mouse.reveal_selection();
            let buffer = paint(&app);
            assert!(
                rendered_row(&buffer, rect_for(&app, Target::Row(Row::TranslateTo))).contains(
                    &format!(
                        "{}  …",
                        app.settings_modal_state.draft().translate_to.label()
                    )
                )
            );
            let current = app.settings_modal_state.draft().translate_to;
            click(&mut app, Target::Row(Row::TranslateTo));
            assert_eq!(
                app.settings_modal_state.picker().selected_index,
                TranslateLang::ALL
                    .iter()
                    .position(|lang| *lang == current)
                    .unwrap()
            );
            app.handle_input(lang.as_str().as_bytes());
            let chosen = app
                .settings_modal_state
                .filtered_languages()
                .iter()
                .position(|value| *value == lang)
                .unwrap();
            click(&mut app, Target::Pick(chosen));
            assert_eq!(app.settings_modal_state.draft().translate_to, lang);
            assert!(!app.settings_modal_state.picker_open());
            app.handle_input(b" ");
            assert_eq!(app.settings_modal_state.picker().selected_index, index);
            paint(&app);
            assert!(
                hits(&app)
                    .iter()
                    .all(|(_, target)| matches!(target, Target::Close | Target::Pick(_)))
            );
            click(&mut app, Target::Close);
            assert_eq!(app.settings_modal_state.draft().translate_to, lang);
        }
    }
    app.settings_modal_state
        .open_picker(super::state::PickerKind::Language);
    for ch in "日本語".chars() {
        app.settings_modal_state.picker_push(ch);
    }
    assert_eq!(
        app.settings_modal_state.filtered_languages(),
        vec![TranslateLang::Ja]
    );
    app.handle_input(b"\r");
    assert_eq!(
        app.settings_modal_state.draft().translate_to,
        TranslateLang::Ja
    );
    app.handle_input(b" ");
    app.handle_input(b"no-such-language");
    assert_eq!(app.settings_modal_state.picker_len(), 0);
    assert!(text(&paint(&app)).contains("no results"));
    app.handle_input(b"\r");
    assert_eq!(
        app.settings_modal_state.draft().translate_to,
        TranslateLang::Ja
    );
    app.handle_input(b"\x1b[D");
    assert_eq!(
        app.settings_modal_state.draft().translate_to,
        TranslateLang::Ko
    );
    app.handle_input(b"\x1b[C");
    app.handle_input(b" ");
    app.handle_input(b"en");
    super::input::handle_escape(&mut app);
    assert!(!app.settings_modal_state.picker_open());
    assert_eq!(
        app.settings_modal_state.draft().translate_to,
        TranslateLang::Ja
    );
}

#[tokio::test]
async fn interaction_chooser_applies_explicit_modes_and_terminal_reporting_once() {
    let (db, mut app) = fixture().await;
    app.settings_modal_state.select_tab(Tab::Tweaks);
    app.settings_modal_state
        .select_mouse_target(Target::Tweak(TweakRow::InteractionMode));
    app.resize(48, 14).unwrap();
    app.settings_modal_state.mouse.reveal_selection();
    let buffer = paint(&app);
    assert!(
        rendered_row(
            &buffer,
            rect_for(&app, Target::Tweak(TweakRow::InteractionMode))
        )
        .contains("Hybrid  …")
    );
    click(&mut app, Target::Tweak(TweakRow::InteractionMode));
    assert_eq!(app.settings_modal_state.picker().selected_index, 2);
    assert_eq!(app.interaction_mode, InteractionMode::Hybrid);
    let buffer = paint(&app);
    assert!(text(&buffer).contains("Keyboard disables clicks and wheel"));
    click(&mut app, Target::Close);
    assert!(app.pending_terminal_commands.is_empty());
    app.handle_input(b" ");
    click(&mut app, Target::Pick(0));
    assert_eq!(app.interaction_mode, InteractionMode::Keyboard);
    assert!(!app.settings_modal_state.picker_open());
    assert_eq!(
        app.pending_terminal_commands,
        vec![b"\x1b[?1006l\x1b[?1003l\x1b[?1000l".to_vec()]
    );
    app.pending_terminal_commands.clear();
    app.handle_input(b"\r");
    app.handle_input(b"mouse");
    app.handle_input(b"\r");
    assert_eq!(app.interaction_mode, InteractionMode::Mouse);
    assert_eq!(
        app.settings_modal_state.interaction_mode(),
        InteractionMode::Mouse
    );
    assert_eq!(
        app.pending_terminal_commands,
        vec![b"\x1b[?1000h\x1b[?1003h\x1b[?1006h".to_vec()]
    );
    app.pending_terminal_commands.clear();
    app.handle_input(b" ");
    click(&mut app, Target::Pick(2));
    assert_eq!(app.interaction_mode, InteractionMode::Hybrid);
    assert!(app.pending_terminal_commands.is_empty());
    app.handle_input(b" ");
    app.handle_input(b"xyz");
    app.handle_input(b"\r");
    assert_eq!(app.interaction_mode, InteractionMode::Hybrid);
    app.handle_input(b"\x1b[D");
    assert_eq!(app.interaction_mode, InteractionMode::Mouse);
    app.handle_input(b"\x1b[C");
    assert_eq!(app.interaction_mode, InteractionMode::Hybrid);
    let user_id = app.user_id;
    crate::test_helpers::wait_until(
        || {
            let db = db.db.clone();
            async move {
                let client = db.get().await.unwrap();
                let user = late_core::models::user::User::get(&client, user_id)
                    .await
                    .unwrap()
                    .unwrap();
                late_core::models::user::extract_interaction_mode(&user.settings)
                    == Some(InteractionMode::Hybrid)
            }
        },
        "interaction mode persistence",
    )
    .await;
}

#[tokio::test]
async fn sidebar_arrows_preserve_device_only_persistence_and_always_offer_panels() {
    use late_core::models::user_ssh_key::{UserSshKey, extract_key_layout};
    let (db, mut app) = fixture().await;
    let client = db.db.get().await.unwrap();
    UserSshKey::ensure(&client, app.user_id, "settings-device")
        .await
        .unwrap();
    UserSshKey::ensure(&client, app.user_id, "other-device")
        .await
        .unwrap();
    app.key_fingerprint = Some("settings-device".to_string());
    let original = app.rail_modes();
    app.settings_modal_state.select_tab(Tab::Tweaks);
    for size in [(120, 40), (48, 14)] {
        app.resize(size.0, size.1).unwrap();
        for row in [TweakRow::RightSidebar, TweakRow::RoomListSidebar] {
            app.settings_modal_state
                .select_mouse_target(Target::Tweak(row));
            app.settings_modal_state.mouse.reveal_selection();
            paint(&app);
            let left = rect_for(&app, Target::TweakCycle(row, false));
            let right = rect_for(&app, Target::TweakCycle(row, true));
            for forward in [true, true, true, false, false, false] {
                click(&mut app, Target::TweakCycle(row, forward));
                let expected = app.device_rails.unwrap();
                let db = db.db.clone();
                let user_id = app.user_id;
                crate::test_helpers::wait_until(
                    || {
                        let db = db.clone();
                        async move {
                            let client = db.get().await.unwrap();
                            let key = UserSshKey::find_by_fingerprint(
                                &client,
                                user_id,
                                "settings-device",
                            )
                            .await
                            .unwrap()
                            .unwrap();
                            extract_key_layout(&key.settings) == Some(expected)
                        }
                    },
                    "Settings sidebar device persistence",
                )
                .await;
                let buffer = paint(&app);
                assert_eq!(rect_for(&app, Target::TweakCycle(row, false)), left);
                assert_eq!(rect_for(&app, Target::TweakCycle(row, true)), right);
                assert_eq!(buffer[(left.x, left.y)].symbol(), "◂");
                assert_eq!(buffer[(right.x + 1, right.y)].symbol(), "▸");
                assert_eq!(app.rail_modes(), app.settings_modal_state.device_rails());
                if row == TweakRow::RightSidebar {
                    assert_eq!(
                        rendered_row(&buffer, rect_for(&app, Target::SidebarPanels)),
                        "[Panels]"
                    );
                    click(&mut app, Target::SidebarPanels);
                    assert!(app.settings_modal_state.right_sidebar_components_open());
                    click(&mut app, Target::Close);
                }
            }
        }
        assert_eq!(app.rail_modes(), original);
    }
    assert!(
        extract_key_layout(
            &UserSshKey::find_by_fingerprint(&client, app.user_id, "other-device")
                .await
                .unwrap()
                .unwrap()
                .settings
        )
        .is_none()
    );
    let stored = Profile::load(&client, app.user_id).await.unwrap();
    assert_eq!((stored.room_list_mode, stored.right_sidebar_mode), original);
    click(&mut app, Target::Tweak(TweakRow::RightSidebar));
    assert!(app.settings_modal_state.right_sidebar_components_open());
}

#[tokio::test]
async fn statusline_compaction_keeps_names_and_distinct_checkbox_reorder_targets() {
    let (_db, mut app) = fixture().await;
    app.settings_modal_state.select_tab(Tab::Statusline);
    for size in [(120, 40), (48, 14), (60, 14)] {
        app.resize(size.0, size.1).unwrap();
        for index in 0..app.settings_modal_state.statusline_components().len() {
            app.settings_modal_state
                .select_mouse_target(Target::Status(index));
            app.settings_modal_state.mouse.reveal_selection();
            let buffer = paint(&app);
            let row = rect_for(&app, Target::Status(index));
            let label = app.settings_modal_state.statusline_components()[index]
                .component
                .label();
            assert!(rendered_row(&buffer, row).contains(label));
            let toggle = rect_for(&app, Target::StatusToggle(index));
            let up = rect_for(&app, Target::StatusMove(index, -1));
            let down = rect_for(&app, Target::StatusMove(index, 1));
            assert_eq!(toggle.x, row.x + 1);
            assert_eq!(up.right(), down.x);
            assert_eq!(up.x, row.right() - 4);
            assert_eq!(down.right(), row.right());
            assert_eq!((up.width, down.width), (2, 2));
            assert_eq!(rendered_row(&buffer, up), "[↑");
            assert_eq!(rendered_row(&buffer, down), "↓]");
            assert!(!toggle.intersects(up));
            assert!(!toggle.intersects(down));
        }
    }
    app.resize(120, 40).unwrap();
    for index in 0..app.settings_modal_state.statusline_components().len() {
        app.settings_modal_state
            .select_mouse_target(Target::Status(index));
        app.settings_modal_state
            .focus_statusline_pane(StatuslinePane::Detail);
        for dial_index in 0..app.settings_modal_state.statusline_dials().len() {
            app.settings_modal_state
                .select_mouse_target(Target::Dial(dial_index));
            let dial = app.settings_modal_state.statusline_dials()[dial_index];
            if !matches!(
                dial,
                super::state::StatuslineDial::Label | super::state::StatuslineDial::Variant
            ) {
                continue;
            }
            let original = app.settings_modal_state.statusline_components()[index];
            paint(&app);
            let left = rect_for(&app, Target::DialCycle(dial_index, false));
            let right = rect_for(&app, Target::DialCycle(dial_index, true));
            let count = if dial == super::state::StatuslineDial::Label {
                if original.component.text_label().is_empty() {
                    2
                } else {
                    3
                }
            } else {
                original.component.variants().len()
            };
            for forward in [true, false] {
                for _ in 0..count {
                    let buffer = paint(&app);
                    assert_eq!(rect_for(&app, Target::DialCycle(dial_index, false)), left);
                    assert_eq!(rect_for(&app, Target::DialCycle(dial_index, true)), right);
                    assert_eq!(buffer[(left.x, left.y)].symbol(), "◂");
                    assert_eq!(buffer[(right.x + 1, right.y)].symbol(), "▸");
                    let before = app.settings_modal_state.statusline_components()[index];
                    click(&mut app, Target::DialCycle(dial_index, forward));
                    assert_ne!(
                        app.settings_modal_state.statusline_components()[index],
                        before
                    );
                }
            }
            assert_eq!(
                app.settings_modal_state.statusline_components()[index],
                original
            );
        }
    }
}
