use super::keep_remote_audio;
use livekit::prelude::TrackSource;

const ME: &str = "11111111-1111-7111-8111-111111111111";
const STREAMER: &str = "22222222-2222-7222-8222-222222222222";

fn stream(user_id: &str) -> String {
    format!("stream-{user_id}")
}

#[test]
fn another_users_cli_mic_plays() {
    assert!(keep_remote_audio(STREAMER, TrackSource::Microphone));
}

#[test]
fn stream_publishers_never_play_whatever_the_source_label() {
    // `stream-*` identities are program publishers (OBS ingress, go-live
    // console), never human mics. The ingress source label is not
    // guaranteed to survive the transcoding-off passthrough, so a
    // Microphone-labeled track from a stream publisher is still the OBS
    // mix, not a voice, and it must not leak into CLI voice. This also
    // keeps your own stream publisher from echoing back at you.
    for source in [
        TrackSource::Microphone,
        TrackSource::ScreenshareAudio,
        TrackSource::Screenshare,
        TrackSource::Camera,
        TrackSource::Unknown,
    ] {
        assert!(
            !keep_remote_audio(&stream(ME), source),
            "{source:?} from your own stream publisher must be dropped"
        );
        assert!(
            !keep_remote_audio(&stream(STREAMER), source),
            "{source:?} from another stream publisher must be dropped"
        );
    }
}

#[test]
fn program_audio_from_a_plain_participant_never_plays() {
    for source in [
        TrackSource::ScreenshareAudio,
        TrackSource::Screenshare,
        TrackSource::Camera,
        TrackSource::Unknown,
    ] {
        assert!(
            !keep_remote_audio(STREAMER, source),
            "{source:?} from a plain participant must be dropped"
        );
    }
}
