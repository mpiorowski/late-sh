/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 *
 * This Source Code Form is "Incompatible With Secondary Licenses", as
 * defined by the Mozilla Public License, v. 2.0.
 *
 * Derived from A Dark Room by Michael Townsend / Doublespeak Games
 * (https://github.com/doublespeakgames/adarkroom), MPL-2.0. The clocks below
 * are transcribed from `script/room.js`, `script/outside.js` and
 * `script/state_manager.js`. See LICENSING.md and NOTICE. */

//! The clock. Upstream runs on live `setTimeout`/`setInterval` handles funneled
//! through `Engine.setTimeout`; this port has no timers at all.
//!
//! Instead the game settles forward on demand: on load, on every action, and
//! on leave. Elapsed time inside a live SSH session *is* connected time, so
//! advancing the world is a subtraction, and gaps between sessions contribute
//! nothing without any bookkeeping. Nothing here runs on the render loop.
//!
//! Two clocks, deliberately different (see `pace`):
//! - **Village time** is credited, capped per day, and slowed. Fire,
//!   temperature, the stranger, income and arrivals all step on it.
//! - **Cooldowns** run on plain wall clock, uncapped and unslowed. The light,
//!   stoke, gather and trap buttons must never be frozen by a pacing rule; the
//!   opening act is a click loop and freezing it would just read as broken.

use chrono::{DateTime, Utc};
use rand::Rng;

use super::data::{self, Fire, Job, Resource, Temperature};
use super::model::{Builder, Game, View};

/// What a settle did, for the caller's log and status line.
#[derive(Clone, Debug, Default)]
pub struct Settled {
    /// Notifications produced while time passed, oldest first.
    pub messages: Vec<String>,
    /// Village seconds actually credited.
    pub credited: u32,
    /// Real seconds that went uncredited because today's allowance ran out.
    pub withheld: u32,
}

/// Advance the game to `now`.
///
/// `session_start` is when this SSH session connected: credit never reaches
/// back past it, which is what makes logged-out time worth nothing. `view` is
/// the panel the player is on, because the stranger only gets to her feet when
/// somebody is in the room to see it.
pub fn settle(
    game: &mut Game,
    view: View,
    now: DateTime<Utc>,
    session_start: DateTime<Utc>,
    rng: &mut impl Rng,
) -> Settled {
    let now_secs = now.timestamp();
    let floor = session_start.timestamp().max(game.last_settled);
    // A fresh save has never settled; it starts its clock here rather than
    // crediting every second since the epoch.
    let elapsed = if game.last_settled == 0 {
        0
    } else {
        (now_secs - floor).clamp(0, i64::from(u32::MAX)) as u32
    };
    game.last_settled = now_secs;

    // Cooldowns first, on real time: a pacing rule must never leave a button
    // stuck.
    tick_cooldowns(game, elapsed);

    // The daily floor only applies once the village stands: fast-forwarding a
    // bare room burns allowance on a world where nothing moves.
    let floor = if game.has_village() {
        super::pace::DAILY_CREDIT_FLOOR_SECS
    } else {
        0
    };
    let credited = game.pace.grant(now, elapsed, floor);
    let mut settled = Settled {
        credited,
        // The floor can credit more than elapsed, hence saturating.
        withheld: elapsed.saturating_sub(credited),
        ..Settled::default()
    };
    for _ in 0..credited {
        step_second(game, rng, &mut settled.messages);
    }

    // Upstream fires this on arrival at the room panel, not on a timer.
    if view == View::Room && game.builder == Builder::Sleeping {
        game.builder = Builder::Helping;
        settled.messages.push(
            "the stranger is standing by the fire. she says she can help. says she builds things."
                .to_string(),
        );
    }
    for building in game.refresh_build_options() {
        settled.messages.push(building.available_msg().to_string());
    }
    settled
}

fn tick_cooldowns(game: &mut Game, elapsed: u32) {
    game.stoke_cooldown = game.stoke_cooldown.saturating_sub(elapsed);
    game.gather_cooldown = game.gather_cooldown.saturating_sub(elapsed);
    game.traps_cooldown = game.traps_cooldown.saturating_sub(elapsed);
}

/// One second of village time. Every clock is an explicit countdown, so this
/// is the only place the world moves.
fn step_second(game: &mut Game, rng: &mut impl Rng, messages: &mut Vec<String>) {
    step_fire(game, messages);
    step_temperature(game, messages);
    step_builder(game, messages);
    step_need_wood(game, messages);
    step_income(game);
    step_population(game, rng, messages);
}

/// The fire burns down one level at a time. Once she is helping, the builder
/// feeds it herself: upstream stokes *before* cooling in the same call, so a
/// tended fire holds its level instead of climbing.
fn step_fire(game: &mut Game, messages: &mut Vec<String>) {
    if game.fire == Fire::Dead {
        return;
    }
    game.fire_timer = game.fire_timer.saturating_sub(1);
    if game.fire_timer > 0 {
        return;
    }
    if game.fire.value() <= Fire::Flickering.value()
        && game.builder == Builder::Helping
        && game.store(Resource::Wood) > 0
    {
        messages.push("builder stokes the fire".to_string());
        game.add_store(Resource::Wood, -1);
        game.fire = game.fire.stoked();
    }
    if game.fire != Fire::Dead {
        game.fire = game.fire.cooled();
        game.fire_timer = data::FIRE_COOL_DELAY;
        messages.push(format!("the fire is {}", game.fire.text()));
    }
}

/// The room drifts one step toward the fire, up or down.
fn step_temperature(game: &mut Game, messages: &mut Vec<String>) {
    game.temp_timer = game.temp_timer.saturating_sub(1);
    if game.temp_timer > 0 {
        return;
    }
    game.temp_timer = data::ROOM_WARM_DELAY;
    let temp = game.temperature.value();
    let fire = game.fire.value();
    let next = if temp > 0 && temp > fire {
        Some(temp - 1)
    } else if temp < 4 && temp < fire {
        Some(temp + 1)
    } else {
        None
    };
    if let Some(next) = next {
        game.temperature = Temperature::from_value(next);
        messages.push(format!("the room is {}", game.temperature.text()));
    }
}

/// The stranger's arc. She arrives on her own; warming up is what moves her
/// along after that, so the room has to be warm before she stops shivering.
fn step_builder(game: &mut Game, messages: &mut Vec<String>) {
    let waiting = matches!(
        game.builder,
        Builder::Approaching | Builder::Collapsed | Builder::Shivering
    );
    if !waiting {
        return;
    }
    game.builder_timer = game.builder_timer.saturating_sub(1);
    if game.builder_timer > 0 {
        return;
    }
    game.builder_timer = data::BUILDER_STATE_DELAY;
    match game.builder {
        Builder::Approaching => {
            messages.push(
                "a ragged stranger stumbles through the door and collapses in the corner"
                    .to_string(),
            );
            game.builder = Builder::Collapsed;
            game.need_wood_timer = Some(data::NEED_WOOD_DELAY);
        }
        Builder::Collapsed if game.temperature >= Temperature::Warm => {
            messages.push(
                "the stranger shivers, and mumbles quietly. her words are unintelligible."
                    .to_string(),
            );
            game.builder = Builder::Shivering;
        }
        Builder::Shivering if game.temperature >= Temperature::Warm => {
            messages.push(
                "the stranger in the corner stops shivering. her breathing calms.".to_string(),
            );
            game.builder = Builder::Sleeping;
        }
        // Still too cold to improve, or already past this stage.
        Builder::Collapsed | Builder::Shivering => {}
        Builder::Unseen | Builder::Sleeping | Builder::Helping => {}
    }
}

/// The moment the wood runs out and the forest becomes a place you can go.
fn step_need_wood(game: &mut Game, messages: &mut Vec<String>) {
    let Some(remaining) = game.need_wood_timer else {
        return;
    };
    let remaining = remaining.saturating_sub(1);
    if remaining > 0 {
        game.need_wood_timer = Some(remaining);
        return;
    }
    game.need_wood_timer = None;
    game.forest_unlocked = true;
    game.set_store(Resource::Wood, data::UNLOCK_FOREST_WOOD);
    messages.push("the wind howls outside".to_string());
    messages.push("the wood is running out".to_string());
}

/// Everyone's trade pays out at once. A source whose inputs would go negative
/// simply does not run this round, exactly as upstream skips the payout rather
/// than letting stores go into debt.
fn step_income(game: &mut Game) {
    game.income_timer = game.income_timer.saturating_sub(1);
    if game.income_timer > 0 {
        return;
    }
    game.income_timer = super::pace::slowed(data::INCOME_DELAY);

    if game.builder == Builder::Helping {
        let (resource, amount) = data::BUILDER_YIELD;
        pay(game, &[(resource, amount)]);
    }
    let gatherers = f64::from(game.gatherers());
    if gatherers > 0.0 {
        let (resource, amount) = data::GATHERER_YIELD;
        pay(game, &[(resource, amount * gatherers)]);
    }
    for job in Job::ALL {
        let workers = f64::from(game.worker_count(job));
        if workers == 0.0 {
            continue;
        }
        let deltas: Vec<(Resource, f64)> = job
            .yields()
            .iter()
            .map(|(resource, amount)| (*resource, amount * workers))
            .collect();
        pay(game, &deltas);
    }
}

/// Apply one source's whole payout, or none of it.
fn pay(game: &mut Game, deltas: &[(Resource, f64)]) {
    let affordable = deltas
        .iter()
        .all(|(resource, delta)| held(game, *resource) + delta >= 0.0);
    if !affordable {
        return;
    }
    for (resource, delta) in deltas {
        credit(game, *resource, *delta);
    }
}

/// What a resource is worth counting the fraction not yet banked.
fn held(game: &Game, resource: Resource) -> f64 {
    game.store(resource) as f64 + game.carry.get(&resource).copied().unwrap_or(0.0)
}

/// Move a fractional amount into a store, banking whole units and keeping the
/// remainder for next time.
fn credit(game: &mut Game, resource: Resource, delta: f64) {
    let carry = game.carry.entry(resource).or_insert(0.0);
    *carry += delta;
    let whole = carry.floor();
    *carry -= whole;
    game.add_store(resource, whole as i64);
}

/// Word gets around. Nobody arrives before there is a hut to sleep in.
fn step_population(game: &mut Game, rng: &mut impl Rng, messages: &mut Vec<String>) {
    if !game.has_village() {
        return;
    }
    if game.pop_timer == 0 {
        game.pop_timer = next_arrival_delay(rng);
        return;
    }
    game.pop_timer -= 1;
    if game.pop_timer > 0 {
        return;
    }
    game.pop_timer = next_arrival_delay(rng);

    let space = game.max_population().saturating_sub(game.population);
    if space == 0 {
        return;
    }
    let half = f64::from(space) / 2.0;
    let num = ((rng.r#gen::<f64>() * half + half).floor() as u32).max(1);
    messages.push(arrival_message(num).to_string());
    game.population += num;
}

fn next_arrival_delay(rng: &mut impl Rng) -> u32 {
    // Upstream: `floor(random * (high - low)) + low` minutes, which lands on
    // exactly 0.5, 1.5 or 2.5 (and never on the nominal 3-minute high).
    let (low, high) = data::POP_DELAY_MINUTES;
    let minutes = (rng.r#gen::<f64>() * (high - low)).floor() + low;
    super::pace::slowed((minutes * 60.0) as u32)
}

fn arrival_message(num: u32) -> &'static str {
    match num {
        1 => "a stranger arrives in the night",
        2..=4 => "a weathered family takes up in one of the huts.",
        5..=9 => "a small group arrives, all dust and bones.",
        10..=29 => "a convoy lurches in, equal parts worry and hope.",
        _ => "the town's booming. word does get around.",
    }
}

/// Roll what the traps caught. Kept here rather than in `model` because it is
/// the one player action that needs randomness.
pub fn roll_traps(game: &Game, rng: &mut impl Rng) -> Vec<(Resource, i64)> {
    let mut drops: Vec<(Resource, i64)> = Vec::new();
    for _ in 0..game.trap_rolls() {
        let roll = rng.r#gen::<f64>();
        let Some((_, resource, _)) = data::TRAP_DROPS.iter().find(|(under, _, _)| roll < *under)
        else {
            continue;
        };
        match drops.iter_mut().find(|(r, _)| r == resource) {
            Some((_, count)) => *count += 1,
            None => drops.push((*resource, 1)),
        }
    }
    drops
}

/// The line a trap haul prints: upstream's `checkTraps` join, commas between
/// finds and an "and" before the last.
pub fn haul_message(drops: &[(Resource, i64)]) -> String {
    let finds: Vec<&str> = drops
        .iter()
        .map(|(resource, _)| trap_message(*resource))
        .collect();
    let mut message = String::from("the traps contain ");
    for (i, find) in finds.iter().enumerate() {
        if i > 0 {
            message.push_str(if i == finds.len() - 1 { " and " } else { ", " });
        }
        message.push_str(find);
    }
    message
}

/// The message for one trap find, from the same table as the drops.
fn trap_message(resource: Resource) -> &'static str {
    data::TRAP_DROPS
        .iter()
        .find(|(_, r, _)| *r == resource)
        .map(|(_, _, message)| *message)
        .unwrap_or("something")
}
