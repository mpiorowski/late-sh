use super::*;

#[test]
fn arrows_home_and_end_become_ss3() {
    assert_eq!(to_application_cursor(b"\x1b[A"), b"\x1bOA");
    assert_eq!(to_application_cursor(b"\x1b[B"), b"\x1bOB");
    assert_eq!(to_application_cursor(b"\x1b[C"), b"\x1bOC");
    assert_eq!(to_application_cursor(b"\x1b[D"), b"\x1bOD");
    assert_eq!(to_application_cursor(b"\x1b[H"), b"\x1bOH");
    assert_eq!(to_application_cursor(b"\x1b[F"), b"\x1bOF");
}

#[test]
fn ordinary_keys_and_a_burst_of_arrows_survive() {
    // Plain keys are untouched, including the letters the broken arrows used
    // to decay into.
    assert_eq!(to_application_cursor(b"hjklABCD?"), b"hjklABCD?");
    // A held arrow arrives as one chunk of repeats, and arrows glue to
    // ordinary keys; every sequence in the chunk is rewritten in place.
    assert_eq!(
        to_application_cursor(b"\x1b[A\x1b[A\x1b[A"),
        b"\x1bOA\x1bOA\x1bOA"
    );
    assert_eq!(to_application_cursor(b"z\x1b[Ds"), b"z\x1bODs");
}

#[test]
fn sequences_that_do_not_change_form_pass_through() {
    // Page Up/Down are the same in both modes: brogue's save picker binds them
    // beside the arrows, and they work today precisely because of that.
    assert_eq!(to_application_cursor(b"\x1b[5~\x1b[6~"), b"\x1b[5~\x1b[6~");
    // Modified arrows stay CSI in application mode too.
    assert_eq!(to_application_cursor(b"\x1b[1;2A"), b"\x1b[1;2A");
    // F1's CSI encoding, and an SS3 arrow from a client that already sends
    // one, must not be mangled.
    assert_eq!(to_application_cursor(b"\x1b[11~"), b"\x1b[11~");
    assert_eq!(to_application_cursor(b"\x1bOA"), b"\x1bOA");
}

#[test]
fn a_sequence_cut_by_the_chunk_boundary_is_left_alone() {
    // The tail is passed on as-is rather than rewritten on a guess or held
    // back; the `A` that completes it arrives in the next chunk and is
    // forwarded there, exactly as before this translation existed.
    assert_eq!(to_application_cursor(b"\x1b["), b"\x1b[");
    assert_eq!(to_application_cursor(b"\x1b"), b"\x1b");
    assert_eq!(to_application_cursor(b"x\x1b["), b"x\x1b[");
}
