use super::*;

#[test]
fn fresh_character_matches_seed_defaults() {
    let c = Character::new("hero", 100);
    assert_eq!(c.level, 1);
    assert_eq!(c.experience, 0);
    assert_eq!(c.hitpoints, 10);
    assert_eq!(c.max_hitpoints(), 10);
    assert_eq!(c.attack(), 1); // level 1 + fists 0
    assert_eq!(c.defense(), 1);
    assert_eq!(c.gold, 50);
    assert_eq!(c.turns, 10);
    assert!(c.alive);
}

#[test]
fn stats_track_level_and_gear() {
    let mut c = Character::new("hero", 0);
    c.level = 8;
    c.weapon_tier = 10;
    c.armor_tier = 7;
    assert_eq!(c.max_hitpoints(), 80);
    assert_eq!(c.attack(), 18); // 8 + 10
    assert_eq!(c.defense(), 15); // 8 + 7
}

#[test]
fn specialty_skill_grants_a_use_every_three() {
    let mut c = Character::new("hero", 0);
    c.choose_specialty(Specialty::Thief);
    // Choosing seeds the +1 bonus use.
    assert_eq!(c.specialty_uses, 1);
    // Two increments: still floor(2/3)=0 from skill, the seeded use remains.
    c.increment_specialty();
    c.increment_specialty();
    assert_eq!(c.specialty_skill, 2);
    assert_eq!(c.specialty_uses, 1);
    // The third increment crosses a multiple of 3 and grants a use.
    c.increment_specialty();
    assert_eq!(c.specialty_skill, 3);
    assert_eq!(c.specialty_uses, 2);
}

#[test]
fn specialty_uses_refresh_on_new_day() {
    let mut c = Character::new("hero", 0);
    c.choose_specialty(Specialty::Mystical);
    c.specialty_skill = 9; // floor(9/3) = 3, plus the +1 chosen bonus
    c.specialty_uses = 0; // spent down during the day
    c.roll_new_day(1, 0, 0, &mut rand::thread_rng());
    assert_eq!(c.specialty_uses, 4);
}

// --- PvP (pvp.php + lib/pvpsupport.php + lib/pvpwarning.php) ---------

#[test]
fn pvp_immunity_needs_every_condition() {
    // Immune while young, dragonless, unforfeited, and under the exp bar
    // (`pvpwarning`: age <= 5 AND dk == 0 AND pk == 0 AND exp <= 1500).
    let mut c = Character::new("hero", 0);
    c.age = 5;
    c.experience = 1500;
    assert!(c.pvp_immune());
    // Each condition alone breaks it.
    assert!(!{
        let mut c = c.clone();
        c.age = 6;
        c.pvp_immune()
    });
    assert!(!{
        let mut c = c.clone();
        c.experience = 1501;
        c.pvp_immune()
    });
    assert!(!{
        let mut c = c.clone();
        c.dragon_kills = 1;
        c.pvp_immune()
    });
    c.pk = true; // attacked while immune once: forfeited forever
    assert!(!c.pvp_immune());
}

#[test]
fn pvp_win_gold_follows_the_log_formula() {
    // round(10 * level * ln(max(1, gold))): ln(1000) = 6.9078 -> 345.
    assert_eq!(pvp_win_gold(5, 1000), 345);
    // A pauper's ln(1) = 0: nothing to take.
    assert_eq!(pvp_win_gold(5, 0), 0);
    assert_eq!(pvp_win_gold(5, 1), 0);
}

#[test]
fn pvp_attacker_exp_pays_the_level_difference() {
    // Base round(10% of 1000) = 100; +2 levels: bonus +20; -1: -10.
    assert_eq!(pvp_attacker_exp(1000, 7, 5), (120, 20));
    assert_eq!(pvp_attacker_exp(1000, 4, 5), (90, -10));
    assert_eq!(pvp_attacker_exp(1000, 5, 5), (100, 0));
}

#[test]
fn pvp_death_costs_the_purse_and_fifteen_percent() {
    let mut c = Character::new("hero", 0);
    c.gold = 500;
    c.experience = 1000;
    c.companions.push(Companion {
        name: "Shadow".into(),
        hitpoints: 5,
        max_hitpoints: 5,
        attack: 1.0,
        defense: 1.0,
        attack_per_level: 0,
        defense_per_level: 0,
        hp_per_level: 0,
        dying_text: String::new(),
        ability: Default::default(),
        ignore_limit: true,
    });
    c.pvp_die();
    assert_eq!(c.gold, 0);
    assert_eq!(c.experience, 850); // 15% lost (pvpattlose)
    assert!(!c.alive);
    assert!(c.companions.is_empty());
}

#[test]
fn pvp_slain_takes_from_the_bank_on_a_shortfall() {
    // The victim spent gold between engage and settlement: the bank
    // absorbs the difference (pvpvictory's IF guard).
    let mut c = Character::new("hero", 0);
    c.gold = 50;
    c.gold_in_bank = 100;
    c.experience = 1000;
    c.pvp_slain(80, 50);
    assert_eq!(c.gold, 0);
    assert_eq!(c.gold_in_bank, 70);
    assert_eq!(c.experience, 950); // the engage-time 5% passed in
    assert!(!c.alive);
}

#[test]
fn pvp_fights_refill_at_dawn_but_not_on_resurrection() {
    let mut c = Character::new("hero", 0);
    c.player_fights = 0;
    c.favor = 200;
    c.alive = false;
    // The paid resurrection skips the PvP pool (newday.php's
    // `resurrection != true` guard), like soulpoints and grave fights.
    assert!(c.resurrect(0, &mut rand::thread_rng()).is_some());
    assert_eq!(c.player_fights, 0);
    // A real dawn refills it.
    c.roll_new_day(1, 0, 0, &mut rand::thread_rng());
    assert_eq!(c.player_fights, PVP_FIGHTS_PER_DAY);
}

#[test]
fn bounty_immunity_is_one_notch_more_lenient_than_pvp() {
    // dag.php tests strict `<` on age and experience where the PvP
    // list/warning use `<=`: exactly-at-the-bar warriors are still safe
    // from attack yet already bountyable. Kept 1=1.
    let mut c = Character::new("hero", 0);
    c.age = PVP_IMMUNITY_DAYS;
    c.experience = PVP_IMMUNITY_MAX_EXP;
    assert!(c.pvp_immune());
    assert!(!c.bounty_immune());

    c.age = PVP_IMMUNITY_DAYS - 1;
    c.experience = PVP_IMMUNITY_MAX_EXP - 1;
    assert!(c.bounty_immune());
    // Any of the escape hatches ends it: a kill, a pk, the thresholds.
    c.pk = true;
    assert!(!c.bounty_immune());
}

#[test]
fn bounty_cost_adds_the_ten_percent_fee_rounded() {
    assert_eq!(bounty_cost(100), 110);
    assert_eq!(bounty_cost(155), 171); // 170.5 rounds half-away
    assert_eq!(bounty_cost(0), 0);
}

#[test]
fn clan_rank_ladder_pops_the_founder_rung() {
    // clan_nextrank/clan_previousrank drop the founder before walking:
    // nothing promotes to 31, and a stepped-down founder is a leader.
    assert_eq!(clan_next_rank(CLAN_APPLICANT), CLAN_MEMBER);
    assert_eq!(clan_next_rank(CLAN_MEMBER), CLAN_OFFICER);
    assert_eq!(clan_next_rank(CLAN_OFFICER), CLAN_LEADER);
    assert_eq!(clan_next_rank(CLAN_LEADER), CLAN_LEADER);
    assert_eq!(clan_prev_rank(CLAN_FOUNDER), CLAN_LEADER);
    assert_eq!(clan_prev_rank(CLAN_LEADER), CLAN_OFFICER);
    assert_eq!(clan_prev_rank(CLAN_MEMBER), CLAN_APPLICANT);
    assert_eq!(clan_prev_rank(CLAN_APPLICANT), CLAN_APPLICANT);
}

#[test]
fn clan_promote_clamps_at_the_actors_own_rank() {
    // GREATEST(0, LEAST(yours, next)): an officer lifts a member no
    // higher than officer; a leader lifts an officer to leader.
    assert_eq!(clan_promote_rank(CLAN_OFFICER, CLAN_APPLICANT), CLAN_MEMBER);
    assert_eq!(clan_promote_rank(CLAN_OFFICER, CLAN_MEMBER), CLAN_OFFICER);
    assert_eq!(clan_promote_rank(CLAN_LEADER, CLAN_OFFICER), CLAN_LEADER);
    assert_eq!(clan_promote_rank(CLAN_FOUNDER, CLAN_OFFICER), CLAN_LEADER);
}

#[test]
fn clan_management_gates_follow_the_membership_page() {
    // Only officers+ see the ops at all.
    assert!(!clan_can_promote(CLAN_MEMBER, CLAN_APPLICANT));
    // Promote: strictly below you, never onto the founder rung.
    assert!(clan_can_promote(CLAN_OFFICER, CLAN_MEMBER));
    assert!(!clan_can_promote(CLAN_OFFICER, CLAN_OFFICER));
    assert!(!clan_can_promote(CLAN_FOUNDER, CLAN_FOUNDER));
    // Demote: equals-or-below, never yourself, and hidden when the rung
    // below is applicant — a member can only be removed.
    assert!(clan_can_demote(CLAN_LEADER, CLAN_OFFICER, false));
    assert!(clan_can_demote(CLAN_OFFICER, CLAN_OFFICER, false));
    assert!(!clan_can_demote(CLAN_OFFICER, CLAN_MEMBER, false));
    assert!(!clan_can_demote(CLAN_LEADER, CLAN_LEADER, true));
    // The founder's one self-demotion is the step-down.
    assert!(clan_can_step_down(CLAN_FOUNDER, CLAN_FOUNDER, true));
    assert!(!clan_can_step_down(CLAN_LEADER, CLAN_LEADER, true));
    // Remove: at-or-below, never yourself (that's the withdraw).
    assert!(clan_can_remove(CLAN_OFFICER, CLAN_OFFICER, false));
    assert!(clan_can_remove(CLAN_OFFICER, CLAN_APPLICANT, false));
    assert!(!clan_can_remove(CLAN_OFFICER, CLAN_LEADER, false));
    assert!(!clan_can_remove(CLAN_OFFICER, CLAN_OFFICER, true));
}

#[test]
fn clan_name_and_tag_validation_follow_the_registrar() {
    // applicant_new.php: 5–50 chars of letters/spaces/apostrophes/dashes;
    // the tag 2–5 letters only.
    assert!(clan_name_valid("The Dragon's-Bane"));
    assert!(!clan_name_valid("Four"));
    assert!(!clan_name_valid(&"a".repeat(51)));
    assert!(!clan_name_valid("Bad Name 7"));
    assert!(clan_tag_valid("DB"));
    assert!(clan_tag_valid("BANES"));
    assert!(!clan_tag_valid("A"));
    assert!(!clan_tag_valid("TOOBIG"));
    assert!(!clan_tag_valid("D7"));
}

#[test]
fn commentary_name_tags_real_members_only() {
    // The <TAG> prefix renders for rank > 0 only — applicants stay bare
    // (upstream's `if ($row['clanrank'])`).
    let mut c = Character::new("hero", 0);
    assert_eq!(c.commentary_name(), "hero");
    c.join_clan(uuid::Uuid::from_u128(7), "DB", CLAN_APPLICANT, 100);
    assert_eq!(c.commentary_name(), "hero");
    c.clan_rank = CLAN_MEMBER;
    assert_eq!(c.commentary_name(), "<DB> hero");
    c.leave_clan();
    assert_eq!(c.commentary_name(), "hero");
    assert_eq!(c.clan_id, None);
    assert_eq!(c.clan_joined_at, 0);
}

#[test]
fn clan_membership_survives_a_dragon_kill() {
    // dragon.php's preserve list carries clanid/clanrank/clanjoindate
    // through the reset.
    let mut c = Character::new("hero", 0);
    c.join_clan(uuid::Uuid::from_u128(7), "DB", CLAN_FOUNDER, 100);
    c.level = 12;
    c.slay_dragon(false);
    assert_eq!(c.clan_id, Some(uuid::Uuid::from_u128(7)));
    assert_eq!(c.clan_rank, CLAN_FOUNDER);
    assert_eq!(c.clan_joined_at, 100);
    assert_eq!(c.clan_tag, "DB");
}

#[test]
fn transfer_draw_taps_the_hand_first_and_the_bank_for_the_rest() {
    let mut c = Character::new("hero", 0);
    c.gold = 30;
    c.gold_in_bank = 100;
    // Fully covered by the purse: the bank untouched.
    assert_eq!(c.draw_for_transfer(20), 0);
    assert_eq!((c.gold, c.gold_in_bank), (10, 100));
    // The shortfall comes out of the bank (`bank.php`'s negative-gold
    // overflow), and the split comes back for a refund.
    assert_eq!(c.draw_for_transfer(50), 40);
    assert_eq!((c.gold, c.gold_in_bank), (0, 60));
}

#[test]
fn the_new_post_watermark_trails_one_dawn_behind() {
    // `newday.php`: `recentcomments = lasthit` then `lasthit = now` —
    // "new" means posted since your PREVIOUS dawn, whenever that was.
    let mut c = Character::new("hero", 10);
    assert_eq!(c.comments_seen_before_day, 0);
    c.roll_new_day(12, 0, 0, &mut rand::thread_rng()).unwrap();
    assert_eq!(c.comments_seen_before_day, 10);
    c.roll_new_day(15, 0, 0, &mut rand::thread_rng()).unwrap();
    assert_eq!(c.comments_seen_before_day, 12);
}

#[test]
fn new_day_resets_the_transfer_counters() {
    // newday.php zeroes `amountouttoday`/`transferredtoday`
    // unconditionally, resurrection days included.
    let mut c = Character::new("hero", 0);
    c.amount_out_today = 75;
    c.transfers_received_today = 3;
    c.roll_new_day(1, 0, 0, &mut rand::thread_rng()).unwrap();
    assert_eq!(c.amount_out_today, 0);
    assert_eq!(c.transfers_received_today, 0);
}

#[test]
fn new_day_collects_a_haunt_once_and_resets_the_bounty_count() {
    let mut c = Character::new("hero", 0);
    c.bounties_set_today = 4;
    c.haunted_by = "Grimald the Grey".into();
    let fx = c.roll_new_day(1, 0, 0, &mut rand::thread_rng()).unwrap();
    // One turn gone against the freshly-assembled day, the mark cleared,
    // and the haunter's name surfaced for the log/report.
    assert_eq!(fx.haunted_by.as_deref(), Some("Grimald the Grey"));
    assert_eq!(c.turns, TURNS_PER_DAY - 1);
    assert!(c.haunted_by.is_empty());
    assert_eq!(c.bounties_set_today, 0);
    // The next dawn has nothing to collect.
    let fx = c.roll_new_day(2, 0, 0, &mut rand::thread_rng()).unwrap();
    assert_eq!(fx.haunted_by, None);
    assert_eq!(c.turns, TURNS_PER_DAY);
}

#[test]
fn a_haunt_collects_on_the_paid_resurrection_too() {
    // newday.php's hauntedby block is unconditional — a bought dawn
    // pays the turn as surely as a real one.
    let mut c = Character::new("hero", 0);
    c.alive = false;
    c.favor = RESURRECTION_FAVOR_COST;
    c.haunted_by = "Grimald the Grey".into();
    let fx = c.resurrect(0, &mut rand::thread_rng()).unwrap();
    assert_eq!(fx.haunted_by.as_deref(), Some("Grimald the Grey"));
    assert!(c.haunted_by.is_empty());
    // base 10 + ff 0 - 6 = 4, then the haunt's -1.
    assert_eq!(c.turns, 3);
}

#[test]
fn increment_without_specialty_is_a_noop() {
    let mut c = Character::new("hero", 0);
    assert_eq!(c.increment_specialty(), None);
    assert_eq!(c.specialty_skill, 0);
    assert_eq!(c.specialty_uses, 0);
}

#[test]
fn advancing_levels_adds_hp_and_full_heals() {
    let mut c = Character::new("hero", 0);
    c.hitpoints = 3;
    c.advance_level();
    assert_eq!(c.level, 2);
    assert_eq!(c.max_hitpoints(), 20);
    assert_eq!(c.hitpoints, 20);
}

#[test]
fn weapon_trade_in_is_credited() {
    let mut c = Character::new("hero", 0);
    // First weapon, no trade-in: tier 1 costs 48.
    assert_eq!(c.weapon_upgrade_cost(1), Some(48));
    assert!(c.buy_weapon(1));
    assert_eq!(c.weapon_tier, 1);
    assert_eq!(c.gold, 2); // 50 - 48
    // Can't "upgrade" to a lower/equal tier.
    assert_eq!(c.weapon_upgrade_cost(1), None);
    // Tier 2 costs 225 minus 75% of tier-1's 48 = 225 - 36 = 189.
    assert_eq!(c.weapon_upgrade_cost(2), Some(189));
}

#[test]
fn healing_is_free_at_level_one_and_scales_after() {
    let mut c = Character::new("hero", 0);
    c.hitpoints = 1;
    assert_eq!(c.full_heal_cost(), 0); // ln(1) = 0
    assert!(c.buy_full_heal());
    assert_eq!(c.hitpoints, 10);

    c.level = 5;
    c.hitpoints = c.max_hitpoints() - 20; // 20 missing
    // round(ln(5) * (20 + 10)) = round(1.609 * 30) = 48
    assert_eq!(c.full_heal_cost(), 48);
}

#[test]
fn death_zeroes_gold_and_clips_exp() {
    let mut c = Character::new("hero", 0);
    c.gold = 500;
    c.experience = 1000;
    c.die();
    assert_eq!(c.gold, 0);
    assert_eq!(c.experience, 900);
    assert!(!c.alive);
    assert_eq!(c.hitpoints, 0);
}

#[test]
fn banked_gold_survives_death() {
    let mut c = Character::new("hero", 0);
    c.gold = 500;
    c.deposit(400);
    assert_eq!(c.gold, 100);
    assert_eq!(c.gold_in_bank, 400);
    c.die();
    assert_eq!(c.gold, 0);
    assert_eq!(c.gold_in_bank, 400);
}

#[test]
fn new_day_refills_and_revives() {
    let mut c = Character::new("hero", 10);
    c.turns = 0;
    c.level = 3;
    c.grave_fights = 0;
    c.seen_dragon = true;
    c.seen_master_today = true;
    c.die();
    // The free path: wait for the dawn and rise with a *full* day — the
    // -6 dock belongs to the paid resurrection only (newday.php applies
    // resurrectionturns only when resurrection=true).
    assert!(c.roll_new_day(11, 0, 0, &mut rand::thread_rng()).is_some());
    assert_eq!(c.turns, TURNS_PER_DAY);
    assert!(c.alive);
    assert_eq!(c.hitpoints, c.max_hitpoints());
    // Soulpoints refill to 50 + 5*level; grave fights to the daily pool;
    // the dragon may be sought again.
    assert_eq!(c.soulpoints, 50 + 5 * 3);
    assert_eq!(c.grave_fights, GRAVE_FIGHTS_PER_DAY);
    assert!(!c.seen_dragon);
    // The master will see you again (`seenmaster` clears every dawn).
    assert!(!c.seen_master_today);
    // Same day again: no reset.
    c.turns = 3;
    assert!(c.roll_new_day(11, 0, 0, &mut rand::thread_rng()).is_none());
    assert_eq!(c.turns, 3);
}

#[test]
fn dead_stats_ignore_gear_and_track_level() {
    let mut c = Character::new("ghost", 0);
    c.weapon_tier = 15;
    c.armor_tier = 15;
    c.dragon_attack_bonus = 9;
    // Level 1: 10 + round(0) on both sides, gear irrelevant.
    assert_eq!(c.dead_combatant().attack, 10);
    assert_eq!(c.dead_combatant().defense, 10);
    assert_eq!(c.max_soulpoints(), 55);
    // Level 4: 10 + round(4.5) = 15 (PHP half-away rounding).
    c.level = 4;
    assert_eq!(c.dead_combatant().attack, 15);
    assert_eq!(c.max_soulpoints(), 70);
}

#[test]
fn soul_restoration_prices_by_depletion() {
    let mut c = Character::new("ghost", 0); // level 1, max soul 55
    c.soulpoints = 0;
    assert_eq!(c.soul_restore_cost(), 10); // fully drained: the cap
    c.soulpoints = 27; // missing 28: round(280/55) = 5
    assert_eq!(c.soul_restore_cost(), 5);
    c.favor = 4;
    assert_eq!(c.restore_soul(), None); // can't afford
    c.favor = 5;
    assert_eq!(c.restore_soul(), Some(5));
    assert_eq!(c.soulpoints, 55);
    assert_eq!(c.favor, 0);
    assert_eq!(c.restore_soul(), None); // already whole
}

#[test]
fn paid_resurrection_is_a_docked_extra_day() {
    let mut c = Character::new("hero", 10);
    c.level = 3;
    c.favor = 120;
    c.die();
    c.soulpoints = 12;
    c.grave_fights = 2;
    // Alive or broke: no sale.
    let mut alive = Character::new("alive", 10);
    alive.favor = 500;
    assert!(alive.resurrect(0, &mut rand::thread_rng()).is_none());
    let mut broke = Character::new("broke", 10);
    broke.die();
    broke.favor = 99;
    assert!(broke.resurrect(0, &mut rand::thread_rng()).is_none());

    assert!(c.resurrect(0, &mut rand::thread_rng()).is_some());
    assert!(c.alive);
    assert_eq!(c.favor, 20);
    // Turns for the rest of today: base 10 - 6 = 4 (plus any ff points).
    assert_eq!(c.turns, (TURNS_PER_DAY as i32 + RESURRECTION_TURNS) as u32);
    assert_eq!(c.hitpoints, c.max_hitpoints());
    // Soulpoints and grave fights are NOT refreshed by the paid path.
    assert_eq!(c.soulpoints, 12);
    assert_eq!(c.grave_fights, 2);
    // last_day untouched: the real next dawn still rolls a full day.
    assert_eq!(c.last_day, 10);
}

#[test]
fn new_day_spirits_jitter_turns() {
    // A live player (no resurrection penalty): base 10 + spirits.
    let mut high = Character::new("high", 10);
    high.roll_new_day(11, 0, 2, &mut rand::thread_rng()); // very high spirits
    assert_eq!(high.turns, 12);
    let mut low = Character::new("low", 10);
    low.roll_new_day(11, 0, -2, &mut rand::thread_rng()); // very low spirits
    assert_eq!(low.turns, 8);
    // ff dragon points feed the daily pool.
    let mut invested = Character::new("ff", 10);
    invested.dragon_ff_bonus = 4;
    invested.roll_new_day(11, 0, 0, &mut rand::thread_rng());
    assert_eq!(invested.turns, 14);
}

#[test]
fn bank_interest_is_gated_on_using_your_turns() {
    // Worked for it: 0 turns left at day's end → interest is paid.
    let mut worker = Character::new("worker", 10);
    worker.gold_in_bank = 1000;
    worker.turns = 0;
    worker.roll_new_day(11, 10, 0, &mut rand::thread_rng()); // 10% rolled
    assert_eq!(worker.gold_in_bank, 1100);

    // Slacked off: left more than the threshold unused → no interest.
    let mut slacker = Character::new("slacker", 10);
    slacker.gold_in_bank = 1000;
    slacker.turns = FIGHTS_FOR_INTEREST + 1;
    slacker.roll_new_day(11, 10, 0, &mut rand::thread_rng());
    assert_eq!(slacker.gold_in_bank, 1000);

    // Over the ceiling → no interest no matter how hard you worked.
    let mut rich = Character::new("rich", 10);
    rich.gold_in_bank = MAX_GOLD_FOR_INTEREST;
    rich.turns = 0;
    rich.roll_new_day(11, 10, 0, &mut rand::thread_rng());
    assert_eq!(rich.gold_in_bank, MAX_GOLD_FOR_INTEREST);

    // Debt compounds even when turns went unused (no "work for it" gate
    // on negative balances).
    let mut debtor = Character::new("debtor", 10);
    debtor.gold_in_bank = -100;
    debtor.turns = FIGHTS_FOR_INTEREST + 5;
    debtor.roll_new_day(11, 10, 0, &mut rand::thread_rng());
    assert_eq!(debtor.gold_in_bank, -110);
}

#[test]
fn borrowing_drives_the_balance_negative() {
    let mut c = Character::new("hero", 0);
    c.level = 5; // lending ceiling 5 * 20 = 100
    assert_eq!(c.max_borrow(), 100);
    assert_eq!(c.borrow_available(), 100);
    assert_eq!(c.borrow(60), 60);
    assert_eq!(c.gold_in_bank, -60);
    assert_eq!(c.gold, 50 + 60);
    // Only 40 left before the floor; requests clamp.
    assert_eq!(c.borrow_available(), 40);
    assert_eq!(c.borrow(500), 40);
    assert_eq!(c.gold_in_bank, -100);
    // A positive balance raises the headroom.
    c.gold_in_bank = 30;
    assert_eq!(c.borrow_available(), 130);
    // Plain withdrawals never dip below zero.
    c.withdraw(500);
    assert_eq!(c.gold_in_bank, 0);
    // Deposits pay debt down.
    c.gold_in_bank = -50;
    c.gold = 80;
    c.deposit(80);
    assert_eq!(c.gold_in_bank, 30);
}

#[test]
fn partial_heals_price_and_heal_by_percent() {
    let mut c = Character::new("hero", 0);
    c.level = 5;
    c.hitpoints = c.max_hitpoints() - 20; // 20 missing
    // Full price: round(ln(5) * 30) = 48; 50% = round(48*0.5) = 24.
    assert_eq!(c.heal_cost(100), 48);
    assert_eq!(c.heal_cost(50), 24);
    assert_eq!(c.heal_cost(10), 5);
    c.gold = 24;
    // 50% heals round(20 * 0.5) = 10 HP.
    assert_eq!(c.buy_heal(50), Some(10));
    assert_eq!(c.hitpoints, c.max_hitpoints() - 10);
    assert_eq!(c.gold, 0);
    // Can't afford the rest.
    assert_eq!(c.buy_heal(100), None);
}

#[test]
fn overheal_normalizes_free() {
    let mut c = Character::new("hero", 0);
    c.hitpoints = c.max_hitpoints() + 7;
    assert!(c.normalize_overheal());
    assert_eq!(c.hitpoints, c.max_hitpoints());
    assert!(!c.normalize_overheal());
}

#[test]
fn dragon_kill_banks_a_point_and_resets_run() {
    let mut c = Character::new("hero", 0);
    c.level = 15;
    c.weapon_tier = 15;
    c.armor_tier = 12;
    c.experience = 99999;
    c.gold = 4000; // wiped by the reset, not retained
    c.gold_in_bank = 90_000;
    c.favor = 40;
    c.haunted_by = "wraith".into();
    c.race = Race::Plainsborn;
    c.specialty = Specialty::Mystical;
    c.specialty_skill = 12;
    c.slay_dragon(false);

    assert_eq!(c.dragon_kills, 1);
    // One chooseable dragon point banked; no boons auto-applied.
    assert_eq!(c.dragon_points_unspent, 1);
    assert_eq!(c.dragon_attack_bonus, 0);
    assert_eq!(c.dragon_defense_bonus, 0);
    assert_eq!(c.dragon_hp_bonus, 0);
    assert_eq!(c.charm, CHARM_PER_DRAGON_KILL);
    // Run reset.
    assert_eq!(c.level, 1);
    assert_eq!(c.weapon_tier, 0);
    assert_eq!(c.armor_tier, 0);
    assert_eq!(c.experience, 0);
    // Restart gold: 50 + 50*1 = 100 (on-hand gold not retained).
    assert_eq!(c.gold, 100);
    // First kill is below the gem threshold (kills-7).
    assert_eq!(c.gems, 0);
    // dragon.php's field loop reverts everything outside its preserve
    // list: bank, favor, haunt mark, race, specialty all reset, and a
    // full new day is owed once the gates close.
    assert_eq!(c.gold_in_bank, 0);
    assert_eq!(c.favor, 0);
    assert!(c.haunted_by.is_empty());
    assert_eq!(c.race, Race::None);
    assert_eq!(c.specialty, Specialty::None);
    assert_eq!(c.specialty_skill, 0);
    assert!(c.dawn_owed);
    assert!(!c.seen_dragon);
}

#[test]
fn the_owed_dawn_rolls_a_full_day_without_a_calendar_change() {
    // dragon.php wipes `lasthit`, so the very next page load runs a full
    // newday.php once the gates pass — same game day or not.
    let mut c = Character::new("hero", 10);
    c.level = 15;
    c.turns = 2;
    c.soulpoints = 1;
    c.grave_fights = 0;
    c.player_fights = 0;
    c.slay_dragon(false);
    c.spend_dragon_point(DragonPointKind::ForestFights);
    c.race = Race::Plainsborn;
    let fx = c.dawn(0, &mut rand::thread_rng());
    assert!(!c.dawn_owed);
    // A fresh run's first day: base 10 + 1 ff point + 2 race bonus.
    assert_eq!(c.turns, TURNS_PER_DAY + 1 + 2);
    assert_eq!(c.age, 1);
    assert_eq!(c.soulpoints, c.max_soulpoints());
    assert_eq!(c.grave_fights, GRAVE_FIGHTS_PER_DAY);
    assert_eq!(c.player_fights, PVP_FIGHTS_PER_DAY);
    assert_eq!(c.hitpoints, c.max_hitpoints());
    // `last_day` untouched: the real next dawn still rolls normally.
    assert_eq!(c.last_day, 10);
    assert!(!fx.divorced);
}

#[test]
fn dragon_kill_gold_caps_then_flawless_adds_on_top() {
    let mut c = Character::new("hero", 0);
    c.level = 15;
    c.dragon_kills = 9; // 10th kill after increment
    c.gold = 100;
    c.slay_dragon(true);
    assert_eq!(c.dragon_kills, 10);
    // 50 + 50*10 = 550, capped to 300, then +150 flawless = 450.
    assert_eq!(c.gold, DRAGON_RUN_GOLD_CAP + FLAWLESS_GOLD_BONUS);
    // Gems: max(0, 10-7) = 3, plus 1 flawless = 4.
    assert_eq!(c.gems, 4);
}

#[test]
fn dragon_points_spend_into_permanent_boons() {
    let mut c = Character::new("hero", 0);
    c.dragon_points_unspent = 4;
    assert!(c.spend_dragon_point(DragonPointKind::Hp));
    assert_eq!(c.dragon_hp_bonus, HP_PER_DRAGON_POINT);
    assert_eq!(c.hitpoints, HP_PER_LEVEL + HP_PER_DRAGON_POINT);
    assert!(c.spend_dragon_point(DragonPointKind::Attack));
    assert!(c.spend_dragon_point(DragonPointKind::Defense));
    assert_eq!(c.attack(), 2);
    assert_eq!(c.defense(), 2);
    let before = c.turns;
    assert!(c.spend_dragon_point(DragonPointKind::ForestFights));
    assert_eq!(c.dragon_ff_bonus, 1);
    assert_eq!(c.turns, before + 1); // today's pool grows immediately
    // Pool exhausted.
    assert_eq!(c.dragon_points_unspent, 0);
    assert!(!c.spend_dragon_point(DragonPointKind::Attack));
}

#[test]
fn forest_victory_pays_rolls_and_refunds_flawless_turns() {
    use rand::{SeedableRng, rngs::StdRng};
    let mut c = Character::new("hero", 0);
    c.level = 3;
    let turns_before = c.turns;
    let foe = SlainFoe {
        level: 3,
        gold: 148,
        exp: 34,
    };
    let mut rng = StdRng::seed_from_u64(7);
    let v = c.forest_victory(&[foe], true, &mut rng);
    // Single foe at your level: no level-diff bonus, exp = the foe's exp.
    assert_eq!(v.exp, 34);
    // Gold: e_rand(0,148) then e_rand(roll, 2*roll) — bounded by 2x base.
    assert!(v.gold <= 296);
    // Flawless at-level fight refunds the turn.
    assert!(v.turn_refunded);
    assert_eq!(c.turns, turns_before + 1);
    assert_eq!(c.experience, 34);

    // Over-leveled flawless fights refund nothing.
    let mut over = Character::new("over", 0);
    over.level = 10;
    let weak = SlainFoe {
        level: 3,
        gold: 10,
        exp: 34,
    };
    let v = over.forest_victory(&[weak], true, &mut rng);
    assert!(!v.turn_refunded);
    // Level-diff penalty: bonus round(34*(1+.25*(3-10)) - 34) = -60 drives
    // the total negative, so the -exp+1 floor pays exactly 1 exp.
    assert_eq!(v.exp, 1);
}

#[test]
fn forest_victory_multi_fight_bonuses() {
    use rand::{SeedableRng, rngs::StdRng};
    let mut c = Character::new("hero", 0);
    c.level = 5;
    c.dragon_kills = 12;
    let foe = SlainFoe {
        level: 5,
        gold: 198,
        exp: 55,
    };
    let mut rng = StdRng::seed_from_u64(3);
    let v = c.forest_victory(&[foe, foe, foe], false, &mut rng);
    // Per-foe exp average is 55; the multi bonus adds
    // round(dragonkills*level / n) = round(60/3) = 20, scaled by
    // 1.05^2 → round(20 * 1.1025) = 22. Total 77.
    assert_eq!(v.exp, 77);
    assert!(!v.turn_refunded);
}

#[test]
fn mushroom_save_clamps_victory_at_one_hp() {
    use rand::{SeedableRng, rngs::StdRng};
    let mut c = Character::new("hero", 0);
    c.hitpoints = 0;
    let foe = SlainFoe {
        level: 1,
        gold: 0,
        exp: 0,
    };
    c.forest_victory(&[foe], false, &mut StdRng::seed_from_u64(1));
    assert_eq!(c.hitpoints, 1);
}

#[test]
fn buff_foe_scales_with_investment() {
    use rand::{SeedableRng, rngs::StdRng};
    let base = data::creature_tier(5);
    // No investment: the stat pool is 0, only the exp flux moves.
    let fresh = Character::new("fresh", 0);
    let foe = fresh.buff_foe(base, &mut StdRng::seed_from_u64(2));
    assert_eq!(foe.attack, base.attack);
    assert_eq!(foe.defense, base.defense);
    assert_eq!(foe.hp, base.hp);
    let expflux = (base.exp as f64 / 10.0).round() as u32;
    assert!(foe.exp >= base.exp - expflux && foe.exp <= base.exp + expflux);

    // Invested: dk = round(20 * (0.25 + 0.05*100/100)) = 6 points spread
    // over attack/defense/+5hp, with gold/exp compensation.
    let mut vet = Character::new("vet", 0);
    vet.dragon_kills = 100;
    vet.dragon_attack_bonus = 8;
    vet.dragon_defense_bonus = 7;
    vet.dragon_hp_bonus = 25; // 5 points
    let foe = vet.buff_foe(base, &mut StdRng::seed_from_u64(2));
    let spent =
        (foe.attack - base.attack) + (foe.defense - base.defense) + (foe.hp - base.hp) / 5;
    assert_eq!(spent, 6);
    assert!(foe.gold >= base.gold);
}

#[test]
fn dragon_scaling_tracks_investment() {
    use rand::{SeedableRng, rngs::StdRng};
    let mut c = Character::new("hero", 0);
    c.level = 15;
    // No boons → no scaling, the dragon is exactly its base (deterministic).
    let base = c.scaled_dragon(&mut StdRng::seed_from_u64(1));
    assert_eq!(base, c.scaled_dragon(&mut StdRng::seed_from_u64(99)));

    // Invest +4 attack, +2 defense, +30 HP (=6 HP-points). investment = 12,
    // scaling points = round(12 * 0.75) = 9.
    c.dragon_attack_bonus = 4;
    c.dragon_defense_bonus = 2;
    c.dragon_hp_bonus = 30;
    assert_eq!(c.investment_points(), 12);
    let (a, d, h) = c.scaled_dragon(&mut StdRng::seed_from_u64(3));
    // The flux always spends exactly the 9 points (as +1 atk/def or +5 HP).
    let stat_points = (a - base.0) + (d - base.1) + (h - base.2) / 5;
    assert_eq!(stat_points, 9);
    assert!(a >= base.0 && d >= base.1 && h >= base.2);
}

#[test]
fn race_stat_bonuses_scale_with_level() {
    // The elf/troll formula: 1 + floor(level/5) — +1 at 1..=4, +2 at
    // 5..=9, +3 at 10..=14, +4 at 15.
    let mut c = Character::new("weald", 0);
    c.race = Race::Wealdkin;
    assert_eq!(c.defense(), 1 + 1); // level 1 + armor 0 + bonus 1
    assert_eq!(c.attack(), 1); // no attack bonus for the Wealdkin
    c.level = 5;
    assert_eq!(c.defense(), 5 + 2);
    c.level = 15;
    assert_eq!(c.defense(), 15 + 4);

    let mut t = Character::new("crag", 0);
    t.race = Race::Cragborn;
    t.level = 10;
    assert_eq!(t.attack(), 10 + 3);
    assert_eq!(t.defense(), 10);

    // The dead fight on level alone: no race bonus beyond the grave.
    let dead = t.dead_combatant();
    t.race = Race::None;
    assert_eq!(t.dead_combatant().attack, dead.attack);
    assert_eq!(t.dead_combatant().defense, dead.defense);
}

#[test]
fn plainsborn_gain_bonus_fights_each_day() {
    let mut c = Character::new("plains", 10);
    c.race = Race::Plainsborn;
    c.roll_new_day(11, 0, 0, &mut rand::thread_rng());
    assert_eq!(c.turns, TURNS_PER_DAY + PLAINSBORN_FOREST_BONUS);

    // The race's newday hook fires on the paid resurrection too:
    // 10 + 2 - 6 = 6 turns.
    c.die();
    c.favor = 100;
    assert!(c.resurrect(0, &mut rand::thread_rng()).is_some());
    assert_eq!(
        c.turns,
        (TURNS_PER_DAY as i32 + PLAINSBORN_FOREST_BONUS as i32 + RESURRECTION_TURNS) as u32
    );
}

#[test]
fn deepfolk_scale_gold_and_shrug_off_cave_ins() {
    assert_eq!(Race::Deepfolk.creature_gold(100), 120);
    assert_eq!(Race::Deepfolk.creature_gold(97), 116); // round(116.4)
    assert_eq!(Race::Plainsborn.creature_gold(100), 100);
    assert_eq!(Race::Deepfolk.mine_death_percent(), 5);
    assert_eq!(Race::Wealdkin.mine_death_percent(), 90);
}

#[test]
fn forest_hunt_shifts_creature_level() {
    assert_eq!(ForestHunt::Slumming.creature_level(5), 4);
    assert_eq!(ForestHunt::Hunt.creature_level(5), 5);
    assert_eq!(ForestHunt::Thrillseeking.creature_level(5), 6);
    assert_eq!(ForestHunt::Slumming.creature_level(1), 1); // clamps
    assert_eq!(ForestHunt::Thrillseeking.creature_level(15), 16); // clamps
}

#[test]
fn the_run_ages_a_day_at_every_dawn() {
    // A fresh character starts on day 1 (upstream rolls the first new day
    // at first login); each dawn adds one, an already-rolled day doesn't.
    let mut c = Character::new("hero", 10);
    assert_eq!(c.age, 1);
    c.roll_new_day(11, 0, 0, &mut rand::thread_rng());
    assert_eq!(c.age, 2);
    c.roll_new_day(11, 0, 0, &mut rand::thread_rng());
    assert_eq!(c.age, 2);
}

#[test]
fn a_dead_dawn_counts_a_resurrection() {
    let mut c = Character::new("hero", 10);
    c.alive = false;
    c.roll_new_day(11, 0, 0, &mut rand::thread_rng());
    assert!(c.alive);
    assert_eq!(c.resurrections, 1);
    // A living dawn doesn't.
    c.roll_new_day(12, 0, 0, &mut rand::thread_rng());
    assert_eq!(c.resurrections, 1);
}

#[test]
fn the_paid_resurrection_ages_the_run_and_counts_itself() {
    let mut c = Character::new("hero", 10);
    c.alive = false;
    c.favor = RESURRECTION_FAVOR_COST;
    assert!(c.resurrect(0, &mut rand::thread_rng()).is_some());
    assert_eq!(c.age, 2);
    assert_eq!(c.resurrections, 1);
}

#[test]
fn a_dragon_kill_stamps_the_run_age_and_resets_the_counters() {
    let mut c = Character::new("hero", 0);
    c.level = 15;
    c.age = 9;
    c.resurrections = 3;
    c.slay_dragon(false);
    assert_eq!(c.dragon_age, 9);
    assert_eq!(c.best_dragon_age, 9);
    assert_eq!(c.age, 0);
    assert_eq!(c.resurrections, 0);

    // A slower next run doesn't beat the record; a faster one does.
    c.age = 14;
    c.slay_dragon(false);
    assert_eq!(c.dragon_age, 14);
    assert_eq!(c.best_dragon_age, 9);
    c.age = 4;
    c.slay_dragon(false);
    assert_eq!(c.dragon_age, 4);
    assert_eq!(c.best_dragon_age, 4);
}
