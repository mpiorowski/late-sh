use super::*;

/// A rectangle inside the cap keeps its own pixels (made even); a larger
/// one shrinks with its shape kept; one too small to draw in has no canvas.
#[test]
fn canvas_takes_the_rectangles_shape_within_the_cap() {
    assert_eq!(
        Canvas::for_pixels(1281, 721),
        Some(Canvas {
            width: 1280,
            height: 720
        })
    );
    assert_eq!(
        Canvas::for_pixels(2560, 1600),
        Some(Canvas {
            width: 1728,
            height: 1080
        })
    );
    assert_eq!(
        Canvas::for_pixels(3840, 1000),
        Some(Canvas {
            width: 1920,
            height: 500
        })
    );
    assert_eq!(Canvas::for_pixels(15, 900), None);
    assert_eq!(Canvas::for_pixels(0, 0), None);
}

/// A frame is scaled as large as fits with its aspect kept and centered,
/// whether it is wider, taller, smaller or larger than the canvas.
#[test]
fn frames_fit_the_canvas_centered_with_their_aspect() {
    let canvas = Canvas {
        width: 1280,
        height: 720,
    };
    // 16:10 in 16:9: bars left and right.
    assert_eq!(
        canvas.fit(1728, 1080),
        Placement {
            x: 64,
            y: 0,
            width: 1152,
            height: 720
        }
    );
    // Ultrawide: bars above and below.
    assert_eq!(
        canvas.fit(3440, 1440),
        Placement {
            x: 0,
            y: 92,
            width: 1280,
            height: 534
        }
    );
    // A small frame is scaled up to fill.
    assert_eq!(
        canvas.fit(320, 180),
        Placement {
            x: 0,
            y: 0,
            width: 1280,
            height: 720
        }
    );
}

/// The whole composed frame: black everywhere (limited-range luma 16,
/// neutral chroma), the picture's three planes at the placement, source
/// stride padding left behind.
#[test]
fn blit_lands_each_plane_at_the_placement() {
    let canvas = Canvas {
        width: 8,
        height: 4,
    };
    let place = Placement {
        x: 2,
        y: 0,
        width: 4,
        height: 4,
    };
    // Luma rows are 4 wide in a stride of 6; chroma rows 2 wide in 3.
    let y: Vec<u8> = (0..4).flat_map(|_| [200, 201, 202, 203, 9, 9]).collect();
    let u: Vec<u8> = (0..2).flat_map(|_| [50, 51, 9]).collect();
    let v: Vec<u8> = (0..2).flat_map(|_| [60, 61, 9]).collect();

    let mut frame = canvas.black();
    canvas.blit(&mut frame, place, (&y, &u, &v), (6, 3, 3));

    let luma_row = [16, 16, 200, 201, 202, 203, 16, 16];
    let mut expected: Vec<u8> = (0..4).flat_map(|_| luma_row).collect();
    expected.extend((0..2).flat_map(|_| [128, 50, 51, 128]));
    expected.extend((0..2).flat_map(|_| [128, 60, 61, 128]));
    assert_eq!(frame, expected);
}

/// The stream's own audio always plays; the room's voices only when the
/// ticket asks, so a viewer in the voice channel never hears them twice.
#[test]
fn voices_play_only_when_the_ticket_asks() {
    assert!(keep_audio(false, "stream-01a1bb93"));
    assert!(keep_audio(true, "stream-01a1bb93"));
    assert!(keep_audio(true, "01a1bb93-5b50-7f09"));
    assert!(!keep_audio(false, "01a1bb93-5b50-7f09"));
}

/// The ticket's token is a credential and stays out of every log line.
#[test]
fn ticket_debug_hides_the_token() {
    let ticket = StreamTicket {
        livekit_url: "wss://rtc.late.sh".to_string(),
        token: "secret-jwt".to_string(),
        voices: true,
    };
    let shown = format!("{ticket:?}");
    assert!(shown.contains("wss://rtc.late.sh"));
    assert!(!shown.contains("secret-jwt"));
}
