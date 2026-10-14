use super::*;

#[test]
fn wrapped_buttons_and_content_publish_only_visible_targets() {
    let area = Rect::new(0, 0, 12, 4);
    let mut buffer = Buffer::empty(area);
    let mouse = MouseState::<usize, ()>::default();
    mouse.begin((12, 4));
    buttons(
        &mut Surface {
            buffer: &mut buffer,
        },
        Rect::new(1, 1, 8, 2),
        &mouse,
        &[("[Save]", 1), ("[Close]", 2)],
    );
    assert_eq!(mouse.target(1, 2, (12, 4)), Some(2));
    assert_eq!(mouse.target(9, 2, (12, 4)), None);
}
