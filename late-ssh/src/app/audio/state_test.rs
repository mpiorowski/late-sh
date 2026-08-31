use super::submitted_line;
use late_core::models::media_queue_item::SONG_QUEUE_REWARD_CHIPS;

/// The banner is the only place a submitter is told what they earned, so it
/// has to read from what was actually minted: a track that came in past the
/// day's cap must not advertise chips.
#[test]
fn a_submission_banner_only_promises_chips_that_were_minted() {
    assert_eq!(
        submitted_line("Submitted", 0, SONG_QUEUE_REWARD_CHIPS),
        format!("Submitted - up next (+{SONG_QUEUE_REWARD_CHIPS} chips)")
    );
    assert_eq!(
        submitted_line("Queued from history", 3, SONG_QUEUE_REWARD_CHIPS),
        format!("Queued from history - #3 in line (+{SONG_QUEUE_REWARD_CHIPS} chips)")
    );
    assert_eq!(submitted_line("Submitted", 0, 0), "Submitted - up next");
    assert_eq!(submitted_line("Submitted", 2, 0), "Submitted - #2 in line");
}
