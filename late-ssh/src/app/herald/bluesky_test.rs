use std::io::Cursor;

use image::{ImageFormat, Rgb, RgbImage};
use uuid::Uuid;

use super::bluesky::{BLOB_MAX_BYTES, facets, fit_under, tid_for};

/// A PNG of `width` by `height`, flat (tiny) or noisy (incompressible).
pub(crate) fn png(width: u32, height: u32, noisy: bool) -> Vec<u8> {
    let mut seed: u32 = 7;
    let image = RgbImage::from_fn(width, height, |_, _| {
        if noisy {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let bytes = seed.to_be_bytes();
            Rgb([bytes[0], bytes[1], bytes[2]])
        } else {
            Rgb([30, 30, 60])
        }
    });
    let mut out = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("encode");
    out
}

/// A key's record key is a 13-character TID, the same every time, and
/// later keys sort later.
#[test]
fn the_record_key_is_a_stable_tid_in_key_order() {
    let first = Uuid::now_v7();
    std::thread::sleep(std::time::Duration::from_millis(2));
    let second = Uuid::now_v7();
    let tid = tid_for(first);
    assert_eq!(tid.len(), 13);
    assert!(
        tid.bytes()
            .all(|b| b"234567abcdefghijklmnopqrstuvwxyz".contains(&b)),
        "{tid}"
    );
    assert!(b"234567abcdefghij".contains(&tid.as_bytes()[0]), "{tid}");
    assert_eq!(tid_for(first), tid, "deterministic");
    assert!(tid_for(second) > tid, "later keys sort later");
}

/// A small picture goes up as it is; a big one is shrunk until it fits
/// and reports its new size.
#[test]
fn a_big_picture_is_shrunk_under_the_cap() {
    let small = png(640, 200, false);
    let fitted = fit_under(&small, BLOB_MAX_BYTES).expect("fits");
    assert_eq!(fitted.png, small);
    assert_eq!((fitted.width, fitted.height), (640, 200));

    let big = png(1536, 480, true);
    assert!(big.len() > BLOB_MAX_BYTES, "{} bytes", big.len());
    let fitted = fit_under(&big, BLOB_MAX_BYTES).expect("shrinks");
    assert!(
        fitted.png.len() <= BLOB_MAX_BYTES,
        "{} bytes",
        fitted.png.len()
    );
    assert!(fitted.width < 1536 && fitted.width >= 320);
    assert_eq!(
        (f64::from(fitted.width) / f64::from(fitted.height) * 10.0).round(),
        32.0,
        "the aspect ratio holds"
    );

    let cannot = fit_under(&png(400, 400, true), 100).expect_err("never fits");
    assert!(cannot.to_string().contains("does not fit"));
}

/// The link and the tags are facets by byte range; plain words are not.
#[test]
fn facets_mark_the_link_and_the_tags_by_bytes() {
    let text =
        "Ünïcode before.\n\nhttps://late.sh/gallery?key=daily:2026-10-22\n\n#ASCIIart #ssh end";
    let found = facets(text);
    assert_eq!(found.len(), 3, "{found:?}");
    let link_start = text.find("https://").unwrap();
    let link_end = link_start + "https://late.sh/gallery?key=daily:2026-10-22".len();
    assert_eq!(found[0]["index"]["byteStart"], link_start);
    assert_eq!(found[0]["index"]["byteEnd"], link_end);
    assert_eq!(
        found[0]["features"][0]["uri"],
        "https://late.sh/gallery?key=daily:2026-10-22"
    );
    assert_eq!(found[1]["features"][0]["tag"], "ASCIIart");
    assert_eq!(
        found[1]["features"][0]["$type"],
        "app.bsky.richtext.facet#tag"
    );
    assert_eq!(found[2]["features"][0]["tag"], "ssh");
    let tag_start = text.find("#ssh").unwrap();
    assert_eq!(found[2]["index"]["byteStart"], tag_start);
    assert_eq!(found[2]["index"]["byteEnd"], tag_start + 4);
    assert!(facets(" a lone # and words ").is_empty());
}
