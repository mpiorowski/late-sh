#[path = "../helpers/mod.rs"]
mod helpers;

mod svc;

use std::{
    thread,
    time::{Duration, Instant},
};

use late_ssh::app::artboard::provenance::ArtboardProvenance;
use late_ssh::app::artboard::svc::DartboardService;

pub(crate) fn wait_for<T>(mut check: impl FnMut() -> Option<T>) -> T {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(value) = check() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "condition not met before timeout"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

pub(crate) fn test_color() -> dartboard_core::RgbColor {
    dartboard_core::RgbColor::new(255, 110, 64)
}

pub(crate) fn shared_provenance() -> late_ssh::app::artboard::provenance::SharedArtboardProvenance {
    ArtboardProvenance::default().shared()
}

pub(crate) fn connected_service(
    server: dartboard_local::ServerHandle,
    username: &str,
    shared: late_ssh::app::artboard::provenance::SharedArtboardProvenance,
) -> DartboardService {
    DartboardService::new(server, uuid::Uuid::now_v7(), username, shared)
}
