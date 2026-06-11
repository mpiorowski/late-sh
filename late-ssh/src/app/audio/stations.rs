use late_core::models::user::{AudioSource, IcecastStream, RadioStation};

pub struct StreamSelection {
    pub url: String,
    pub station: &'static str,
}

pub fn resolve_stream_selection(
    icecast_base_url: &str,
    source: AudioSource,
    icecast_stream: IcecastStream,
    radio_station: RadioStation,
) -> Option<StreamSelection> {
    match source {
        AudioSource::Icecast => Some(StreamSelection {
            url: icecast_stream_url(icecast_base_url, icecast_stream),
            station: icecast_stream.as_str(),
        }),
        AudioSource::Radio => Some(StreamSelection {
            url: radio_station_url(radio_station).to_string(),
            station: radio_station.as_str(),
        }),
        AudioSource::Youtube => None,
    }
}

fn icecast_stream_url(base_url: &str, stream: IcecastStream) -> String {
    let base = base_url.trim_end_matches('/');
    match stream {
        IcecastStream::Chill => format!("{base}/chill"),
        IcecastStream::Classical => format!("{base}/classical"),
    }
}

// The .mp3 URLs, not the .m4a ones the site advertises: .m4a is a 302 to
// .mp3 anyway, and the CLI decoder only aligns MP3 streams, so going
// direct removes the dependency on that redirect.
fn radio_station_url(station: RadioStation) -> &'static str {
    match station {
        RadioStation::Chillsynth => "https://stream.nightride.fm/chillsynth.mp3",
        RadioStation::Nightride => "https://stream.nightride.fm/nightride.mp3",
        RadioStation::Datawave => "https://stream.nightride.fm/datawave.mp3",
        RadioStation::Spacesynth => "https://stream.nightride.fm/spacesynth.mp3",
    }
}

/// Display labels for selector rows and selection banners. Settings keys
/// (`as_str`) and display labels currently coincide, but they are separate
/// concerns: renaming a label must not migrate persisted settings.
pub fn icecast_stream_display_name(stream: IcecastStream) -> &'static str {
    match stream {
        IcecastStream::Chill => "chill",
        IcecastStream::Classical => "classical",
    }
}

pub fn radio_station_display_name(station: RadioStation) -> &'static str {
    match station {
        RadioStation::Chillsynth => "chillsynth",
        RadioStation::Nightride => "nightride",
        RadioStation::Datawave => "datawave",
        RadioStation::Spacesynth => "spacesynth",
    }
}

pub fn icecast_stream_by_index(index: u8) -> Option<IcecastStream> {
    match index {
        1 => Some(IcecastStream::Chill),
        2 => Some(IcecastStream::Classical),
        _ => None,
    }
}

pub fn radio_station_by_index(index: u8) -> Option<RadioStation> {
    match index {
        1 => Some(RadioStation::Chillsynth),
        2 => Some(RadioStation::Nightride),
        3 => Some(RadioStation::Datawave),
        4 => Some(RadioStation::Spacesynth),
        _ => None,
    }
}
