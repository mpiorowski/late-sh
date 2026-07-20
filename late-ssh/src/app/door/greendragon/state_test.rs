use super::*;

fn lvl(level: u8) -> Character {
    let mut c = Character::new("t", 0);
    c.level = level;
    c.hitpoints = c.max_hitpoints();
    c
}

#[test]
fn village_menu_gates_on_state() {
    let mut c = lvl(1);
    c.turns = 0;
    let rows = village_menu(&c, false, false);
    // Forest row disabled with no turns.
    assert!(!rows[0].1);
    // Healer disabled at full health.
    let healer = rows
        .iter()
        .find(|(l, _)| l.starts_with("The Mendery"))
        .unwrap();
    assert!(!healer.1);
    // Dragon not offered below level 15 (a forest nav, `lib/forest.php`).
    assert!(
        !forest_menu(&c)
            .iter()
            .any(|(l, _)| l.starts_with("Seek Out"))
    );
    // Slumming's nav hides at level 1 (`lib/forest.php:15`).
    assert!(
        !forest_menu(&c)
            .iter()
            .any(|(l, _)| l.starts_with("Go Slumming"))
    );
}

#[test]
fn dragon_offered_at_max_level() {
    let c = lvl(15);
    let rows = forest_menu(&c);
    assert!(rows.iter().any(|(l, _)| l.starts_with("Seek Out")));
    // The dragon fight offers no Flee row; skills stay
    // (`dragon.php` `fightnav(true, false)`).
    let rows = fight_menu(&c, FoeKind::Dragon);
    assert!(!rows.iter().any(|(l, _)| l == "Flee"));
    assert!(rows.len() > 1 || c.specialty == Specialty::None);
}

#[test]
fn shop_lists_affordable_upgrades() {
    let mut c = lvl(2); // level 2 stocks tiers 1 and 2
    c.gold = 100; // affords tier 1 (48) but not tier 2 (189 after trade-in)
    let tiers = available_tiers(&c, true);
    assert_eq!(tiers[0], (1, 48));
    let menu = shop_menu(&c, true);
    assert!(menu[0].1); // tier 1 affordable
    assert!(!menu[1].1); // tier 2 not
}

#[test]
fn shop_is_level_gated() {
    // Even with limitless gold, a shop only stocks gear up to your level.
    let mut c = lvl(3);
    c.gold = 1_000_000;
    let tiers = available_tiers(&c, true);
    assert!(tiers.iter().all(|(t, _)| *t <= 3));
    assert_eq!(tiers.last().unwrap().0, 3);
    // Out of upgrades for your rank shows the level-gated nudge, not "finest".
    c.weapon_tier = 3;
    let menu = shop_menu(&c, true);
    assert!(menu[0].0.contains("Advance a level"));
}

#[test]
fn bank_menu_reflects_balances() {
    let mut c = lvl(3);
    c.gold = 200;
    c.gold_in_bank = 0;
    let rows = bank_menu(&c, true);
    assert!(rows[0].1); // can deposit
    assert!(!rows[1].1); // nothing to withdraw
    // The loan row offers the full level-scaled credit line (3 * 20).
    assert!(rows[2].0.contains("60 gold available"));
    assert!(rows[2].1);
    // At level 3 the transfer window is open (`mintransferlev`).
    assert!(rows[3].1);

    // In debt: the deposit row becomes a pay-down and the credit shrinks.
    c.gold_in_bank = -40;
    let rows = bank_menu(&c, true);
    assert!(rows[0].0.starts_with("Pay down debt (40 owed)"));
    assert!(!rows[1].1); // nothing (positive) to withdraw
    assert!(rows[2].0.contains("20 gold available"));
}

#[test]
fn bank_transfer_row_gates_on_level_or_dragon_kills() {
    // Under `mintransferlev` (3) with no kills the window is shut...
    let mut c = lvl(2);
    let rows = bank_menu(&c, true);
    assert!(!rows[3].1);
    // ...a dragon kill opens it regardless of level...
    c.dragon_kills = 1;
    assert!(bank_menu(&c, true)[3].1);
    // ...and a settling transfer holds the row until the runner returns.
    assert!(!bank_menu(&c, false)[3].1);
}

#[test]
fn healer_menu_stocks_the_full_percent_shelf() {
    let mut c = lvl(5);
    c.hitpoints = c.max_hitpoints() - 20; // full cost 48
    c.gold = 24;
    let rows = healer_menu(&c);
    // 100% plus 90..10 by tens.
    assert_eq!(rows.len(), 10);
    assert!(rows[0].0.starts_with("Complete healing (48 gold)"));
    assert!(!rows[0].1); // can't afford 48
    assert!(rows[1].0.starts_with("Heal 90%"));
    // 50% costs 24 — exactly affordable (row index 5: 100,90,80,70,60,50).
    assert!(rows[5].0.starts_with("Heal 50% (24 gold)"));
    assert!(rows[5].1);
    assert!(rows[9].0.starts_with("Heal 10% (5 gold)"));
}

#[test]
fn graveyard_menu_gates_on_favor_and_fights() {
    let mut c = lvl(5); // max soulpoints 75
    c.die();
    c.grave_fights = 0;
    c.favor = 0;
    c.soulpoints = 55; // missing 20: restore costs round(200/75) = 3
    let rows = graveyard_menu(&c);
    assert!(rows[0].0.contains("0 left today"));
    assert!(!rows[0].1); // no torments left
    assert!(rows[1].0.contains("(3 favor)"));
    assert!(!rows[1].1); // can't afford restoration
    assert!(!rows[2].1); // resurrection needs 100 favor
    assert!(!rows[3].1); // haunting needs 25 favor
    assert!(rows[4].1); // the lost souls always listen
    assert!(rows[5].1); // waiting always works

    c.grave_fights = 4;
    c.favor = 100;
    let rows = graveyard_menu(&c);
    assert!(rows[0].1);
    assert!(rows[1].1);
    assert!(rows[2].1);
    assert!(rows[3].1); // 100 favor covers the haunt too

    // A whole soul has nothing to restore, whatever the favor.
    c.soulpoints = c.max_soulpoints();
    assert!(!graveyard_menu(&c)[1].1);
}

#[test]
fn fight_menu_hides_skills_from_the_dead() {
    let mut c = lvl(5);
    c.choose_specialty(Specialty::Thief);
    // Alive: Attack + 4 skills + Flee.
    assert_eq!(fight_menu(&c, FoeKind::Creature).len(), 6);
    // PvP strips skills AND the way out ("honor" and "pride").
    let rows = fight_menu(&c, FoeKind::Pvp);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "Attack");
    // Dead (a torment fight): bare essence only.
    c.die();
    let rows = fight_menu(&c, FoeKind::Torment);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "Attack");
    assert_eq!(rows[1].0, "Flee");
}

#[test]
fn stables_menu_counts_the_trade_in() {
    let mut c = lvl(5);
    c.gems = 6;
    let rows = stables_menu(&c);
    assert_eq!(rows.len(), 3);
    assert!(rows[0].1); // pony affordable at 6 gems
    assert!(!rows[1].1); // courser (10 gems) not

    // Owning the pony adds its 4-gem refund to buying power and a sell row.
    c.mount = 1;
    let rows = stables_menu(&c);
    assert_eq!(rows.len(), 4);
    assert!(!rows[0].1); // your own stall is not for sale
    assert!(rows[1].1); // 6 gems + 4 refund covers the courser
    assert!(rows[3].0.contains("4 gem refund"));
}

#[test]
fn merc_camp_lists_hires_and_mending() {
    let mut c = lvl(5);
    c.gold = 10_000;
    c.gems = 10;
    assert_eq!(merc_camp_menu(&c).len(), 2);

    // The crag bear is a Deepfolk-only listing.
    c.race = Race::Deepfolk;
    let rows = merc_camp_menu(&c);
    assert_eq!(rows.len(), 3);
    assert!(rows[2].0.contains("Crag Bear"));

    // One hire fills the cap: every hire row disables. Wounding the hire
    // adds a mend row priced by the sawbones formula.
    assert!(c.hire_mercenary(&data::MERCENARIES[0]));
    assert!(merc_camp_menu(&c).iter().all(|(_, enabled)| !enabled));
    c.companions[0].hitpoints = 1;
    let rows = merc_camp_menu(&c);
    assert!(rows.last().unwrap().0.starts_with("Mend Skarn"));
    assert!(rows.last().unwrap().1);
}

#[test]
fn race_menu_offers_the_four_ancestries() {
    let rows = race_menu();
    assert_eq!(rows.len(), model::RACES.len());
    assert!(rows.iter().all(|(_, enabled)| *enabled));
    assert!(rows[0].0.contains("Plainsborn"));
    assert!(rows[2].0.contains("+20% creature gold"));
}

#[test]
fn dragon_point_menu_offers_the_four_boons() {
    let rows = dragon_point_menu();
    assert_eq!(rows.len(), 4);
    assert!(rows.iter().all(|(_, enabled)| *enabled));
    assert!(rows[0].0.contains("max hitpoints"));
    assert!(rows[1].0.contains("forest fight"));
}

#[test]
fn training_gate_blocks_a_second_daily_challenge() {
    let mut c = lvl(1);
    c.experience = c.exp_for_next_level();
    assert!(c.can_challenge_master());
    assert!(training_menu(&c)[0].0.starts_with("Challenge"));

    // The challenge spends the day's audience; only a win reopens it.
    c.seen_master_today = true;
    assert!(!c.can_challenge_master());
    let rows = training_menu(&c);
    assert!(rows[0].0.contains("seen enough of you"));
    assert!(!rows[0].1);
    c.advance_level();
    assert!(!c.seen_master_today);
}

#[test]
fn commentary_menu_gates_the_speak_row_on_the_allowance() {
    // Loading: nothing to count against, so speaking waits.
    let rows = commentary_menu(CommentRoom::Village, None, false, 0, 0);
    assert_eq!(rows.len(), 6);
    assert!(!rows[0].1);
    assert!(rows[4].1); // refresh
    assert!(rows[5].1); // leave

    // Plenty left: a plain prompt.
    let rows = commentary_menu(CommentRoom::Village, Some(13), false, 0, 0);
    assert!(rows[0].1);
    assert!(!rows[0].0.contains("left today"));

    // Running low surfaces the count (upstream shows it under 3).
    let rows = commentary_menu(CommentRoom::Village, Some(2), false, 0, 0);
    assert!(rows[0].0.contains("2 left today"));
    assert!(rows[0].1);

    // Exhausted: the row closes.
    let rows = commentary_menu(CommentRoom::DarkHorse, Some(0), false, 0, 0);
    assert!(!rows[0].1);
}

#[test]
fn commentary_menu_pages_like_upstreams_nav_row() {
    // A full newest window: only "older" opens (upstream shows Previous
    // when the window fills; Next and First Unseen stay dark).
    let rows = commentary_menu(CommentRoom::Village, Some(13), true, 0, 0);
    assert!(rows[1].1); // older
    assert!(!rows[2].1); // newer
    assert!(!rows[3].1); // first unseen

    // Scrolled back: "newer" opens; the unseen jump lights up when its
    // target is a different page.
    let rows = commentary_menu(CommentRoom::Village, Some(13), true, 2, 1);
    assert!(rows[1].1);
    assert!(rows[2].1);
    assert!(rows[3].1);
    let rows = commentary_menu(CommentRoom::Village, Some(13), true, 1, 1);
    assert!(!rows[3].1); // already on the unseen page
}

#[test]
fn village_menu_lists_the_talk_rooms() {
    let mut c = lvl(3);
    c.gold = 0;
    let rows = village_menu(&c, false, false);
    assert!(rows.iter().any(|(l, _)| l.starts_with("The Town Square")));
    assert!(rows.iter().any(|(l, _)| l.starts_with("The Gardens")));
    assert!(
        rows.iter()
            .any(|(l, _)| l.starts_with("A weathered standing stone"))
    );
    // The seance is pay-per-visit: level 3 wants 60 gold.
    let gypsy = rows
        .iter()
        .find(|(l, _)| l.starts_with("The Gypsy's Tent"))
        .unwrap();
    assert!(gypsy.0.contains("60 gold"));
    assert!(!gypsy.1);
    c.gold = 60;
    let rows = village_menu(&c, false, false);
    assert!(
        rows.iter()
            .find(|(l, _)| l.starts_with("The Gypsy's Tent"))
            .unwrap()
            .1
    );
}

// --- the warrior list + Hall of Fame ---------------------------------

fn entry(handle: &str, level: u8) -> RosterEntry {
    RosterEntry {
        user_id: Uuid::from_u128(handle.bytes().fold(0u128, |a, b| a * 31 + b as u128)),
        name: format!("Seedling {handle}"),
        handle: handle.to_string(),
        level,
        alive: true,
        race: "Plainsborn",
        dragon_kills: 0,
        dragon_age: 0,
        best_dragon_age: 0,
        resurrections: 0,
        gems: 0,
        charm: 0,
        max_hp: level as u32 * 10,
        experience: 0,
        wealth: 0,
        online: false,
        idle_secs: 0,
        lodged: false,
        pvp_immune: false,
        bounty_immune: false,
        pvp_engaged_at: 0,
        clan_id: None,
    }
}

// --- the bounty board (modules/dag.php) --------------------------------

#[test]
fn bounty_page_orders_by_level_then_gold_and_flips() {
    let low = entry("low", 3);
    let high = entry("high", 9);
    let rich_low = entry("richlow", 3);
    let wanted = vec![
        (low.user_id, 200u64),
        (high.user_id, 150u64),
        (rich_low.user_id, 500u64),
    ];
    let roster = vec![low, high, rich_low];
    // Default: level desc, gold desc within a level (dag's default sort).
    let page = build_bounty_page(&wanted, &roster, false, 0);
    assert!(page.rows[0].contains("high"));
    assert!(page.rows[1].contains("richlow"));
    assert!(page.rows[2].contains("low"));
    // The gold toggle re-orders by the price alone.
    let page = build_bounty_page(&wanted, &roster, true, 0);
    assert!(page.rows[0].contains("richlow"));
    assert!(page.rows[1].contains("low"));
    assert!(page.rows[2].contains("high"));
}

#[test]
fn bounty_page_drops_targets_without_a_roster_row() {
    // A vanished character's contracts were closed by the board read;
    // whatever aggregate still arrives has no row to hang on.
    let known = entry("known", 5);
    let wanted = vec![(known.user_id, 100u64), (Uuid::from_u128(424242), 999u64)];
    let roster = vec![known];
    let page = build_bounty_page(&wanted, &roster, false, 0);
    assert_eq!(page.rows.len(), 1);
    assert!(page.heading.contains("1 head"));
}

// --- PvP target lists (pvp.php + lib/pvplist.php) ---------------------

#[test]
fn pvp_rows_filter_the_ineligible() {
    let me = Uuid::from_u128(999);
    let mut sleeper = entry("prey", 5);
    let awake = {
        let mut e = entry("awake", 5);
        e.online = true;
        e
    };
    let shielded = {
        let mut e = entry("green", 5);
        e.pvp_immune = true;
        e
    };
    let dead = {
        let mut e = entry("ghost", 5);
        e.alive = false;
        e
    };
    let low = entry("low", 3); // below my-1
    let high = entry("high", 8); // above my+2
    let roster = vec![sleeper.clone(), awake, shielded, dead, low, high];
    // My level 5: the band is [4, 7]; only the plain sleeper qualifies.
    let (rows, elsewhere) = build_pvp_rows(&roster, me, 5, PvpVenue::Fields, 100_000);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].1.contains("prey"));
    assert!(rows[0].2); // attackable
    assert_eq!(elsewhere, 0);

    // A fresh engage flags them off for ten minutes, but they still show.
    sleeper.pvp_engaged_at = 100_000 - 60;
    let (rows, _) = build_pvp_rows(&[sleeper.clone()], me, 5, PvpVenue::Fields, 100_000);
    assert!(!rows[0].2);
    assert!(rows[0].1.contains("hunted too recently"));
    sleeper.pvp_engaged_at = 100_000 - model::PVP_TIMEOUT_SECS;
    let (rows, _) = build_pvp_rows(&[sleeper.clone()], me, 5, PvpVenue::Fields, 100_000);
    assert!(rows[0].2);
}

#[test]
fn pvp_venues_split_on_the_inn_room() {
    let me = Uuid::from_u128(999);
    let fields = entry("fields", 5);
    let mut lodged = entry("lodged", 5);
    lodged.lodged = true;
    let roster = vec![fields, lodged];
    // The fields list holds the unlodged and rumors the other.
    let (rows, elsewhere) = build_pvp_rows(&roster, me, 5, PvpVenue::Fields, 0);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].1.contains("fields"));
    assert_eq!(elsewhere, 1);
    // The inn's keys open only the lodged rooms.
    let (rows, elsewhere) = build_pvp_rows(&roster, me, 5, PvpVenue::Inn, 0);
    assert_eq!(rows.len(), 1);
    assert!(rows[0].1.contains("lodged"));
    assert_eq!(elsewhere, 1);
}

#[test]
fn pvp_rows_never_list_yourself() {
    let me = Uuid::from_u128(999);
    let mut myself = entry("me", 5);
    myself.user_id = me;
    let (rows, elsewhere) = build_pvp_rows(&[myself], me, 5, PvpVenue::Fields, 0);
    assert!(rows.is_empty());
    assert_eq!(elsewhere, 0);
}

#[test]
fn village_menu_offers_the_hunt() {
    let c = lvl(3);
    let row = village_menu(&c, false, false)
        .into_iter()
        .find(|(l, _)| l.starts_with("Slay Other Warriors"))
        .unwrap();
    assert!(row.0.contains("3 left today"));
    assert!(row.1);
}

#[test]
fn warrior_list_orders_by_level_kills_then_name() {
    // list.php: level DESC, dragonkills DESC, login ASC — a total order.
    let mut a = entry("zed", 5);
    let mut b = entry("abe", 5);
    b.dragon_kills = 2;
    let c = entry("moe", 9);
    let page = build_warrior_page(
        &[a.clone(), b.clone(), c.clone()],
        RosterView::All,
        "",
        None,
        0,
    );
    assert!(page.rows[0].contains("moe"));
    assert!(page.rows[1].contains("abe")); // kills break the level tie
    assert!(page.rows[2].contains("zed"));
    // Same level and kills: the bare name decides.
    a.dragon_kills = 2;
    b.name = "Zzz abe".into(); // the *display* name must not re-order
    let page = build_warrior_page(&[a, b, c], RosterView::All, "", None, 0);
    assert!(page.rows[1].contains("abe"));
}

#[test]
fn warrior_search_is_a_subsequence_match() {
    // Upstream interleaves % between typed characters: `%j%o%e%`.
    assert!(name_matches("Farmboy Joe", "joe"));
    assert!(name_matches("Journeyman Orc Expert", "joe")); // subsequence
    assert!(!name_matches("Joe", "joex"));
    assert!(name_matches("Anything", ""));
}

#[test]
fn warrior_online_view_filters_and_pages_clamp() {
    let mut on = entry("here", 3);
    on.online = true;
    let off = entry("gone", 7);
    let entries = [on, off];
    let page = build_warrior_page(&entries, RosterView::Online, "", None, 0);
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].contains("here"));
    // A page past the end clamps to the last page instead of blanking.
    let page = build_warrior_page(&entries, RosterView::All, "", None, 99);
    assert_eq!(page.page, 0);
    assert_eq!(page.rows.len(), 2);
}

#[test]
fn hof_kills_lists_slayers_only_and_gates_your_rank() {
    let mut vet = entry("vet", 10);
    vet.dragon_kills = 3;
    vet.dragon_age = 9;
    vet.best_dragon_age = 7;
    let fresh = entry("fresh", 2);
    let me = lvl(5); // no kills
    let page = build_hof_page(
        &[vet, fresh],
        &me,
        Uuid::nil(),
        HofRanking::Kills,
        false,
        0,
        &mut rand::thread_rng(),
    );
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].contains("vet"));
    // No kills: no "your rank" line (upstream only sets $me when
    // dragonkills > 0 on this ranking).
    assert!(!page.foot.iter().any(|f| f.contains("top")));
}

#[test]
fn hof_gems_ranking_shows_names_only() {
    let mut rich = entry("rich", 5);
    rich.gems = 40;
    let page = build_hof_page(
        &[rich],
        &lvl(1),
        Uuid::nil(),
        HofRanking::Gems,
        false,
        0,
        &mut rand::thread_rng(),
    );
    // Exact gem counts never render (upstream lists rank + name only).
    assert!(!page.rows[0].contains("40"));
}

#[test]
fn hof_wealth_is_fuzzed_within_five_percent() {
    let mut rich = entry("rich", 5);
    rich.wealth = 10_000;
    let mut rng = rand::thread_rng();
    for _ in 0..200 {
        let key = hof_key(&rich, HofRanking::Wealth, &mut rng);
        assert!((9_500..=10_500).contains(&key), "fuzz out of range: {key}");
    }
    // Debt fuzzes too (the total is signed).
    rich.wealth = -1_000;
    let key = hof_key(&rich, HofRanking::Wealth, &mut rng);
    assert!((-1_050..=-950).contains(&key));
}

#[test]
fn hof_speed_ranks_ascending_and_least_flips_it() {
    let mut quick = entry("quick", 5);
    quick.dragon_kills = 1;
    quick.best_dragon_age = 3;
    let mut slow = entry("slow", 5);
    slow.dragon_kills = 1;
    slow.best_dragon_age = 20;
    let mut unranked = entry("never", 15); // no kill: filtered out
    unranked.best_dragon_age = 0;
    let entries = [quick, slow, unranked];
    let page = build_hof_page(
        &entries,
        &lvl(1),
        Uuid::nil(),
        HofRanking::Speed,
        false,
        0,
        &mut rand::thread_rng(),
    );
    assert_eq!(page.rows.len(), 2);
    assert!(page.rows[0].contains("quick")); // fastest first
    let page = build_hof_page(
        &entries,
        &lvl(1),
        Uuid::nil(),
        HofRanking::Speed,
        true,
        0,
        &mut rand::thread_rng(),
    );
    assert!(page.rows[0].contains("slow")); // "worst" = slowest first
}

#[test]
fn hof_percentile_counts_at_or_better_and_floors_at_one() {
    let mut me = lvl(5);
    me.charm = 10;
    let mut best = entry("best", 5);
    best.charm = 50;
    let mut mid = entry("mid", 5);
    mid.charm = 10;
    let mut worst = entry("worst", 5);
    worst.charm = 1;
    let page = build_hof_page(
        &[best, mid, worst],
        &me,
        Uuid::nil(),
        HofRanking::Charm,
        false,
        0,
        &mut rand::thread_rng(),
    );
    // Two of three have charm >= mine: round(200/3) = 67.
    assert!(
        page.foot.iter().any(|f| f.contains("top 67%")),
        "{:?}",
        page.foot
    );
}

#[test]
fn hof_marks_your_own_row() {
    let mut mine = entry("me", 5);
    mine.charm = 9;
    let my_id = mine.user_id;
    let page = build_hof_page(
        &[mine, entry("other", 5)],
        &lvl(5),
        my_id,
        HofRanking::Charm,
        false,
        0,
        &mut rand::thread_rng(),
    );
    assert!(page.rows[0].starts_with('*'));
    assert!(!page.rows[1].starts_with('*'));
}

#[test]
fn warrior_list_menu_gates_the_pager() {
    // Loading: only the presence row and the way back are live.
    let rows = warrior_list_menu(None, false);
    assert!(!rows[0].1);
    assert!(rows[1].1);
    assert!(!rows[3].1);
    assert!(rows[5].1);
    // One page of results: no pager either way.
    let page = ListPage {
        pages: 3,
        page: 1,
        ..ListPage::default()
    };
    let rows = warrior_list_menu(Some(&page), false);
    assert!(rows[3].1); // next
    assert!(rows[4].1); // previous
    // Enrolled with a clan: the clan slice slots in before the pager.
    let rows = warrior_list_menu(Some(&page), true);
    assert!(rows[3].0.contains("clan"));
    assert!(rows[4].1); // next, shifted
}

#[test]
fn hall_of_fame_menu_marks_the_shown_ranking() {
    let page = ListPage::default();
    let rows = hall_of_fame_menu(HofRanking::Wealth, false, Some(&page));
    assert!(rows[1].0.contains("(shown)"));
    assert!(rows[7].0.contains("worst"));
    let rows = hall_of_fame_menu(HofRanking::Wealth, true, Some(&page));
    assert!(rows[7].0.contains("best"));
}

// --- clans (clan.php + lib/clan/*) --------------------------------------

fn member(name: &str, rank: u8, dks: u32, level: u8, joined: i64) -> ClanMemberRow {
    ClanMemberRow {
        user_id: Uuid::from_u128(name.bytes().fold(0u128, |a, b| a * 31 + b as u128)),
        name: name.to_string(),
        level,
        dragon_kills: dks,
        rank,
        joined_at: joined,
        alive: true,
        online: false,
        idle_secs: 0,
    }
}

fn clan_row() -> ClanRow {
    ClanRow {
        id: Uuid::from_u128(99),
        created: chrono::Utc::now(),
        updated: chrono::Utc::now(),
        name: "Dragon's Bane".into(),
        tag: "DB".into(),
        motd: String::new(),
        motd_author: String::new(),
        description: String::new(),
        desc_author: String::new(),
        custom_verb: String::new(),
    }
}

#[test]
fn clan_membership_sorts_rank_kills_level_then_join_date() {
    // clan_membership.php: rank DESC, dragonkills DESC, level DESC,
    // clanjoindate ASC.
    let rows = sort_clan_members(&[
        member("old-member", model::CLAN_MEMBER, 5, 9, 10),
        member("founder", model::CLAN_FOUNDER, 0, 1, 50),
        member("new-officer", model::CLAN_OFFICER, 0, 3, 90),
        member("young-member", model::CLAN_MEMBER, 5, 9, 40),
        member("applicant", model::CLAN_APPLICANT, 9, 15, 1),
    ]);
    let names: Vec<&str> = rows.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "founder",
            "new-officer",
            "old-member", // the join date breaks the full tie
            "young-member",
            "applicant"
        ]
    );
}

#[test]
fn clan_detail_page_orders_by_rank_then_join_date_and_totals_kills() {
    // detail.php: rank DESC, clanjoindate ASC — kills don't reorder the
    // public roll, they only sum in the footer.
    let clan = clan_row();
    let members = [
        member("late-officer", model::CLAN_OFFICER, 9, 9, 80),
        member("early-officer", model::CLAN_OFFICER, 0, 2, 20),
        member("founder", model::CLAN_FOUNDER, 3, 12, 5),
    ];
    let page = build_clan_detail_page(&clan, &members, 0);
    assert!(page.heading.contains("Dragon's Bane <DB>"));
    assert!(page.rows[0].contains("founder"));
    assert!(page.rows[1].contains("early-officer"));
    assert!(page.rows[2].contains("late-officer"));
    assert!(page.foot[0].contains("12 dragon kills"));
}

#[test]
fn warrior_clan_slice_filters_by_presence_and_clan() {
    let my_clan = Some(Uuid::from_u128(9));
    let mut mate = entry("mate", 5);
    mate.online = true;
    mate.clan_id = my_clan;
    let mut offline_mate = entry("sleeper", 5);
    offline_mate.clan_id = my_clan;
    let mut stranger = entry("stranger", 5);
    stranger.online = true;
    let entries = [mate, offline_mate, stranger];
    let page = build_warrior_page(&entries, RosterView::Clan, "", my_clan, 0);
    assert_eq!(page.rows.len(), 1);
    assert!(page.rows[0].contains("mate"));
}

#[test]
fn village_menu_lists_the_rosters() {
    let rows = village_menu(&lvl(1), false, false);
    assert!(rows.iter().any(|(l, _)| l == "List Warriors"));
    assert!(rows.iter().any(|(l, _)| l == "The Hall of Fame"));
}

#[test]
fn tavern_hub_offers_the_barman() {
    let rows = tavern_menu(&lvl(1), TavernView::Hub, None, false);
    // The barman sits between the gambler's games and the etchings; the
    // hub select arm indexes these rows, so the order is load-bearing.
    assert!(rows[3].0.starts_with("A word with the barman"));
    assert!(rows[3].1);
    assert!(rows[4].0.contains("etchings"));
}

#[test]
fn intel_sheet_reads_the_charm_bands() {
    // The verdict line follows `darkhorse.php`'s exact comparisons:
    // equality first, then the wide tests strict at ten either side.
    let verdict = |mine: u32, theirs: u32| {
        let mut t = lvl(3);
        t.charm = theirs;
        build_intel_sheet(&t, mine).pop().unwrap()
    };
    assert!(verdict(5, 5).contains("every bit as homely"));
    assert!(verdict(20, 9).contains("far homelier"));
    // Exactly ten apart fails the strict wide test on both sides.
    assert!(verdict(20, 10).contains("a shade homelier"));
    assert!(verdict(10, 20).ends_with("fairer of face than you."));
    assert!(!verdict(10, 20).contains("far fairer"));
    assert!(verdict(9, 20).contains("far fairer"));
}

#[test]
fn intel_sheet_lays_out_the_stat_rows() {
    let mut t = lvl(4);
    t.gold = 321;
    t.weapon_tier = 2;
    t.armor_tier = 1;
    let sheet = build_intel_sheet(&t, 0);
    assert!(sheet.iter().any(|l| l.contains("Level:   4")));
    assert!(sheet.iter().any(|l| l.contains("Gold:    321")));
    assert!(
        sheet
            .iter()
            .any(|l| l.contains(data::weapon_name(2)) && l.starts_with("Weapon:"))
    );
    // The mock sheet shares the shape but answers nothing.
    let mock = intel_mock_sheet();
    assert!(mock.iter().any(|l| l.starts_with("Level:   Skint")));
}
