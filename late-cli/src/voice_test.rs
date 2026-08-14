use super::keep_remote_audio;
use livekit::prelude::TrackSource;

const ME: &str = "11111111-1111-7111-8111-111111111111";
const STREAMER: &str = "22222222-2222-7222-8222-222222222222";

fn own_stream(user_id: &str) -> String {
    format!("stream-{user_id}")
}

#[test]
fn another_users_cli_mic_plays() {
    assert!(keep_remote_audio(
        &own_stream(ME),
        STREAMER,
        TrackSource::Microphone
    ));
}

#[test]
fn a_mic_from_another_stream_publisher_plays() {
    // Current servers mint stream-publisher grants with no microphone
    // source at all (voice is CLI-only), so this cannot occur in practice;
    // the policy stays tolerant so a CLI against an older server still
    // hears a console-mic streamer rather than silencing a human voice.
    assert!(keep_remote_audio(
        &own_stream(ME),
        &own_stream(STREAMER),
        TrackSource::Microphone
    ));
}

#[test]
fn your_own_stream_publisher_never_echoes_back() {
    // Even its microphone track: it is your own voice.
    assert!(!keep_remote_audio(
        &own_stream(ME),
        &own_stream(ME),
        TrackSource::Microphone
    ));
}

#[test]
fn program_audio_never_reaches_cli_voice() {
    // The OBS ingress mix and the console's screen-share audio live on the
    // watch page, whoever publishes them.
    for source in [
        TrackSource::ScreenshareAudio,
        TrackSource::Screenshare,
        TrackSource::Camera,
        TrackSource::Unknown,
    ] {
        assert!(
            !keep_remote_audio(&own_stream(ME), &own_stream(STREAMER), source),
            "{source:?} from a stream publisher must be dropped"
        );
        assert!(
            !keep_remote_audio(&own_stream(ME), STREAMER, source),
            "{source:?} from a plain participant must be dropped"
        );
    }
}
