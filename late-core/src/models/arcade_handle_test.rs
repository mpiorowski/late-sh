use crate::models::arcade_handle::*;

#[test]
fn shape_accepts_plain_handles() {
    assert!(handle_shape_valid("mat"));
    assert!(handle_shape_valid("srcrip"));
    assert!(handle_shape_valid("Gnoll_Fan_99"));
    assert!(handle_shape_valid("a2345678901234567890")); // 20 chars
}

#[test]
fn shape_rejects_bad_lengths() {
    assert!(!handle_shape_valid(""));
    assert!(!handle_shape_valid("ab"));
    assert!(!handle_shape_valid("a23456789012345678901")); // 21 chars
}

#[test]
fn shape_rejects_bad_charset_and_leading_char() {
    assert!(!handle_shape_valid("1mat")); // must start with a letter
    assert!(!handle_shape_valid("_mat"));
    assert!(!handle_shape_valid("mat p")); // crawl allows spaces; we don't
    assert!(!handle_shape_valid("mat-p")); // host sanitizers strip hyphens
    assert!(!handle_shape_valid("mat.p"));
    assert!(!handle_shape_valid("måt")); // ascii only
}

#[test]
fn reserved_blocks_fallback_and_hash_shapes() {
    assert!(handle_reserved("late"));
    assert!(handle_reserved("LATE"));
    assert!(handle_reserved("late_0dd47727a40681b9"));
    assert!(handle_reserved("Late_anything"));
    // Names merely containing or resembling it stay claimable.
    assert!(!handle_reserved("latecomer"));
    assert!(!handle_reserved("chocolate"));
}
