use super::*;

#[test]
fn viewer_guard_tracks_count() {
    let svc = WorldCupService::new();
    assert_eq!(svc.viewer_count(), 0);

    let a = svc.viewer();
    assert_eq!(svc.viewer_count(), 1);
    let b = svc.viewer();
    assert_eq!(svc.viewer_count(), 2);

    drop(a);
    assert_eq!(svc.viewer_count(), 1);
    drop(b);
    assert_eq!(svc.viewer_count(), 0);
}
