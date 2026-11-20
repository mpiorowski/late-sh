use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Duration, TimeZone, Utc};
use late_core::models::deadchannel_bout::{Bout, Side};
use late_core::models::presence::{ClubhouseStand, PresenceRecord, Spot};
use rand::SeedableRng;
use rand::rngs::StdRng;
use uuid::Uuid;

use super::{Phase, phase, roster, seen_by};
use crate::app::deadchannel::fight::cards::DECK;
use crate::app::deadchannel::ring::data::{AFTERMATH_SECS, BELL_SECS, ROUND_SECS};
use crate::app::deadchannel::ring::state::Duel;
use crate::app::deadchannel::ring::svc::{Board, LiveBout};
use crate::app::deadchannel::runner::state::Look;
use crate::app::deadchannel::runner::svc::RunnerEntry;

fn at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2101, 3, 9, 22, 0, 0).unwrap()
}

/// A bout mira called and tom took at [`at`], the bell set, round 1 on
/// the clock.
pub(crate) fn bout(mira: Uuid, tom: Uuid) -> Bout {
    let bell = at() + Duration::seconds(BELL_SECS);
    Bout {
        id: Uuid::now_v7(),
        day: at().date_naive(),
        caller_user_id: mira,
        called_user_id: tom,
        caller_username: "mira".to_string(),
        called_username: "tom".to_string(),
        caller_deck: Some(serde_json::json!(DECK)),
        called_deck: Some(serde_json::json!(DECK)),
        called_at: at(),
        expires_at: at(),
        accepted_at: Some(at()),
        bell_at: Some(bell),
        round: 1,
        deadline_at: Some(bell + Duration::seconds(ROUND_SECS)),
        state: None,
        pool_caller: 0,
        pool_called: 0,
        ended_at: None,
        outcome: None,
        caller_settled_at: None,
        called_settled_at: None,
    }
}

fn live(mira: Uuid, tom: Uuid) -> LiveBout {
    LiveBout {
        bout: bout(mira, tom),
        duel: Duel::deal(DECK.to_vec(), DECK.to_vec(), &mut StdRng::seed_from_u64(7)),
        bets: Vec::new(),
    }
}

fn record(user_id: Uuid, username: &str) -> PresenceRecord {
    PresenceRecord {
        session_id: Uuid::now_v7(),
        user_id,
        username: username.to_string(),
        clubhouse: ClubhouseStand {
            spot: Spot::Door,
            since_ms: 0,
            emote: None,
            petted_dog_at_ms: None,
        },
        nightcap: None,
        street: None,
        lounge: false,
    }
}

fn entry(marks: i32) -> RunnerEntry {
    RunnerEntry {
        look: Look::random(1, &mut StdRng::seed_from_u64(1)),
        level: 5,
        peak_level: 5,
        marks,
    }
}

#[test]
fn the_roster_is_every_other_marked_runner_on_the_wire_once_by_name() {
    let [me, tom, ash, wren, gone] = [(); 5].map(|()| Uuid::now_v7());
    let looks = Arc::new(HashMap::from([
        (me, entry(2)),
        (tom, entry(1)),
        (ash, entry(3)),
        // No mark: on the wire, not in the ring.
        (wren, entry(0)),
        // Marked, but off the wire.
        (gone, entry(1)),
    ]));
    let records = [
        record(tom, "tom"),
        record(me, "mira"),
        record(wren, "wren"),
        record(ash, "ash"),
        // A second session of the same runner.
        record(tom, "tom"),
    ];
    let names: Vec<(String, i32)> = roster(&records, &looks, me)
        .into_iter()
        .map(|contender| (contender.username, contender.marks))
        .collect();
    assert_eq!(names, [("ash".to_string(), 3), ("tom".to_string(), 1)]);
}

#[test]
fn the_phase_follows_the_bell_the_round_clock_and_the_aftermath() {
    let mut live = live(Uuid::now_v7(), Uuid::now_v7());
    let bell = at() + Duration::seconds(BELL_SECS);
    let secs = |phase: Phase<'_>| match phase {
        Phase::Empty => ("empty", None),
        Phase::Set { bell_in, .. } => ("set", Some(bell_in)),
        Phase::On { clock, .. } => ("on", clock),
        Phase::Over { clears_in, .. } => ("over", Some(clears_in)),
    };
    assert_eq!(secs(phase(None, at())), ("empty", None));
    assert_eq!(
        secs(phase(Some(&live), at() + Duration::seconds(10))),
        ("set", Some(BELL_SECS - 10))
    );
    assert_eq!(
        secs(phase(Some(&live), bell)),
        ("on", Some(ROUND_SECS)),
        "the bell opens round 1's clock"
    );
    assert_eq!(
        secs(phase(Some(&live), bell + Duration::seconds(ROUND_SECS))),
        ("on", None),
        "a round past its clock waits on the sweeper"
    );
    let ended = bell + Duration::seconds(40);
    live.bout.ended_at = Some(ended);
    assert_eq!(
        secs(phase(Some(&live), ended + Duration::seconds(15))),
        ("over", Some(AFTERMATH_SECS - 15))
    );
    assert_eq!(
        secs(phase(
            Some(&live),
            ended + Duration::seconds(AFTERMATH_SECS)
        )),
        ("empty", None),
        "the ring clears"
    );
}

#[test]
fn a_sessions_copy_holds_its_own_slots_and_nobody_elses() {
    let [mira, tom, wren] = [(); 3].map(|()| Uuid::now_v7());
    let mut live = live(mira, tom);
    live.duel.place(Side::Caller, 0, 0).expect("mira lays");
    live.duel.place(Side::Called, 2, 1).expect("tom lays");
    let board = Board {
        calls: Vec::new(),
        live: Some(live),
        records: HashMap::new(),
    };
    let laid = |viewer: Uuid| {
        let seen = seen_by(&board, viewer);
        let duel = &seen.live.as_ref().expect("the bout").duel;
        [Side::Caller, Side::Called].map(|side| {
            duel.corner(side)
                .slots
                .iter()
                .filter(|slot| slot.is_some())
                .count()
        })
    };
    assert_eq!(laid(mira), [1, 0]);
    assert_eq!(laid(tom), [0, 1]);
    assert_eq!(laid(wren), [0, 0], "a watcher sees neither");
    // The hands stay open to everybody.
    let seen = seen_by(&board, wren);
    let full = board.live.as_ref().expect("the bout");
    for side in [Side::Caller, Side::Called] {
        assert_eq!(
            seen.live
                .as_ref()
                .expect("the bout")
                .duel
                .corner(side)
                .piles,
            full.duel.corner(side).piles
        );
    }
}
