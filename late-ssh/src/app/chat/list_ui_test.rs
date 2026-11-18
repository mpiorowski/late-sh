use super::scroll_offset;

/// Stories of 5 rows with a rule between: story `i` is rows `6i..6i+5`.
fn story(i: usize) -> std::ops::Range<usize> {
    6 * i..6 * i + 5
}

/// The story above through the story below, as `lay_out` reports it.
fn context(i: usize, count: usize) -> std::ops::Range<usize> {
    let start = if i == 0 {
        story(0).start
    } else {
        story(i - 1).start
    };
    let end = if i + 1 == count {
        story(i).end
    } else {
        story(i + 1).end
    };
    start..end
}

/// Walking up from the newest story at the bottom, the window holds still
/// until the story above the selection would leave it, and only then
/// scrolls, a story at a time.
#[test]
fn the_list_scrolls_only_when_the_next_story_would_leave_the_window() {
    let count = 10;
    let total = story(count - 1).end;
    let height = 24;
    let mut offset = 0;
    let mut offsets = Vec::new();
    for i in (0..count).rev() {
        offset = scroll_offset(offset, total, &story(i), &context(i, count), height);
        offsets.push(offset);
    }
    // The first draw lands at the bottom; moving up keeps it there while
    // stories 8, 7 and 6 (with 5, 6 above them in view) move inside it.
    assert_eq!(offsets[0], total - height);
    assert_eq!(offsets[1], total - height);
    assert_eq!(offsets[2], total - height);
    // Story 6 is selected: story 5 above it must be in view, row 30.
    assert_eq!(offsets[3], story(5).start.min(total - height));
    // At the top it stops at row 0.
    assert_eq!(*offsets.last().unwrap(), 0);
    // Every step keeps the selected story and the one above it in view.
    for (step, i) in (0..count).rev().enumerate() {
        let top = context(i, count).start;
        assert!(
            offsets[step] <= top,
            "story {i} at offset {}",
            offsets[step]
        );
        assert!(story(i).end <= offsets[step] + height);
    }

    // And back down: still until the story below would leave the window.
    let held = scroll_offset(0, total, &story(1), &context(1, count), height);
    assert_eq!(held, 0);
    let held = scroll_offset(held, total, &story(2), &context(2, count), height);
    assert_eq!(held, 0);
    let moved = scroll_offset(held, total, &story(3), &context(3, count), height);
    assert_eq!(moved, story(4).end - height);
}

/// Too short for a story and both neighbours, the neighbours share what the
/// selected story leaves; shorter than the story, its first rows show.
#[test]
fn a_short_window_keeps_the_selected_story_first() {
    let total = story(9).end;
    let offset = scroll_offset(0, total, &story(5), &context(5, 10), 9);
    assert!(offset <= story(5).start && story(5).end <= offset + 9);
    assert_eq!(
        scroll_offset(0, total, &story(5), &context(5, 10), 3),
        story(5).start
    );
    assert_eq!(
        scroll_offset(0, total, &story(5), &context(5, 10), 0),
        story(5).start.min(total)
    );
}
