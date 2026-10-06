use qrcodegen::QrCodeEcc;
use ratatui::widgets::Widget;
use rstest::{fixture, rstest};

use super::super::barcode::{Braille, FullBlock};
use super::super::polarity::LightOnDark;
use super::*;

type HB<'a> = QrWidget<'a, HalfBlock, DarkOnLight>;
type FB<'a> = QrWidget<'a, FullBlock, DarkOnLight>;
type BR<'a> = QrWidget<'a, Braille, DarkOnLight>;

/// Empty string QR → version 1 → 21×21 modules.
#[fixture]
fn empty_qr() -> QrCode {
    QrCode::encode_text("", QrCodeEcc::Low).expect("failed to create QR code")
}

#[rstest]
#[case::exact_1x1((40, 40), Scaling::Exact(1, 1), (21, 11))]
#[case::exact_2x2((40, 40), Scaling::Exact(2, 2), (42, 21))]
#[case::max_fitting((21, 11), Scaling::Max, (21, 11))]
#[case::max_larger((42, 22), Scaling::Max, (42, 21))]
#[case::min_fitting((21, 11), Scaling::Min, (21, 21))]
#[case::min_larger((42, 22), Scaling::Min, (42, 32))]
fn size_halfblock_no_qz(
    empty_qr: QrCode,
    #[case] area: (u16, u16),
    #[case] scaling: Scaling,
    #[case] expected: (u16, u16),
) {
    let w = HB::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_scaling(scaling)
        .with_aspect_ratio(AspectRatio::Computed);
    let rect = Rect::new(0, 0, area.0, area.1);
    assert_eq!(w.size(rect), Size::from(expected));
}

#[rstest]
#[case::exact_1x1(Scaling::Exact(1, 1), (29, 15))]
#[case::max_71x71(Scaling::Max, (58, 58))]
#[case::min_71x71(Scaling::Min, (87, 73))]
fn size_halfblock_with_qz(
    empty_qr: QrCode,
    #[case] scaling: Scaling,
    #[case] expected: (u16, u16),
) {
    let w = HB::new(&empty_qr)
        .with_scaling(scaling)
        .with_aspect_ratio(AspectRatio::Computed);
    let rect = Rect::new(0, 0, 71, 71);
    assert_eq!(w.size(rect), Size::from(expected));
}

#[rstest]
#[case::square(AspectRatio::Square, (23, 11))]
#[case::computed(AspectRatio::Computed, (21, 11))]
fn size_aspect_halfblock(
    empty_qr: QrCode,
    #[case] aspect: AspectRatio,
    #[case] expected: (u16, u16),
) {
    let w = HB::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_aspect_ratio(aspect);
    assert_eq!(w.size(Rect::new(0, 0, 40, 40)), Size::from(expected));
}

#[rstest]
#[case::computed(AspectRatio::Computed, (11, 6))]
#[case::square(AspectRatio::Square, (13, 6))]
fn size_aspect_braille(
    empty_qr: QrCode,
    #[case] aspect: AspectRatio,
    #[case] expected: (u16, u16),
) {
    let w = BR::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_aspect_ratio(aspect);
    assert_eq!(w.size(Rect::new(0, 0, 40, 40)), Size::from(expected));
}

#[rstest]
fn render_halfblock_exact(empty_qr: QrCode) {
    let mut buf = Buffer::empty(Rect::new(0, 0, 21, 11));
    unstyled::<HB>(&empty_qr).render(buf.area, &mut buf);
    assert_eq!(
        buf,
        Buffer::with_lines([
            "█▀▀▀▀▀█  ▀▄▄  █▀▀▀▀▀█",
            "█ ███ █ ███▀█ █ ███ █",
            "█ ▀▀▀ █  █▀█▄ █ ▀▀▀ █",
            "▀▀▀▀▀▀▀ ▀ ▀ ▀ ▀▀▀▀▀▀▀",
            "▄▄ ▀▀▄▀█▄  ▄█  ▄▄█▀▄▄",
            "▀█ ▄▄▀▀▀█▄▄▄▀▀▀█▄▀ ▄▀",
            "▀ ▀▀  ▀ █▄   ▄▀▀▀▀███",
            "█▀▀▀▀▀█ ▀██▄ ██  ▀█ ▀",
            "█ ███ █ ██ ▀▄ ▀▄█ ▀▀ ",
            "█ ▀▀▀ █  ▀ ▀▀▄ █▀█ ██",
            "▀▀▀▀▀▀▀   ▀▀▀ ▀▀▀ ▀▀ ",
        ])
    );
}

#[rstest]
fn render_halfblock_with_quiet_zone(empty_qr: QrCode) {
    let mut buf = Buffer::empty(Rect::new(0, 0, 29, 15));
    HB::new(&empty_qr)
        .with_aspect_ratio(AspectRatio::Computed)
        .with_style(Style::default())
        .render(buf.area, &mut buf);
    assert_eq!(
        buf,
        Buffer::with_lines([
            "                             ",
            "                             ",
            "    █▀▀▀▀▀█  ▀▄▄  █▀▀▀▀▀█    ",
            "    █ ███ █ ███▀█ █ ███ █    ",
            "    █ ▀▀▀ █  █▀█▄ █ ▀▀▀ █    ",
            "    ▀▀▀▀▀▀▀ ▀ ▀ ▀ ▀▀▀▀▀▀▀    ",
            "    ▄▄ ▀▀▄▀█▄  ▄█  ▄▄█▀▄▄    ",
            "    ▀█ ▄▄▀▀▀█▄▄▄▀▀▀█▄▀ ▄▀    ",
            "    ▀ ▀▀  ▀ █▄   ▄▀▀▀▀███    ",
            "    █▀▀▀▀▀█ ▀██▄ ██  ▀█ ▀    ",
            "    █ ███ █ ██ ▀▄ ▀▄█ ▀▀     ",
            "    █ ▀▀▀ █  ▀ ▀▀▄ █▀█ ██    ",
            "    ▀▀▀▀▀▀▀   ▀▀▀ ▀▀▀ ▀▀     ",
            "                             ",
            "                             ",
        ])
    );
}

#[rstest]
fn render_fullblock_exact(empty_qr: QrCode) {
    let mut buf = Buffer::empty(Rect::new(0, 0, 21, 21));
    unstyled::<FB>(&empty_qr).render(buf.area, &mut buf);
    assert_eq!(
        buf,
        Buffer::with_lines([
            "███████  █    ███████",
            "█     █   ██  █     █",
            "█ ███ █ █████ █ ███ █",
            "█ ███ █ ███ █ █ ███ █",
            "█ ███ █  ███  █ ███ █",
            "█     █  █ ██ █     █",
            "███████ █ █ █ ███████",
            "                     ",
            "   ██ ██    █    ██  ",
            "██   █ ██  ██  ███ ██",
            "██   ████   ████ █  █",
            " █ ██   ████   ██  █ ",
            "█ ██  █ █     ███████",
            "        ██   █    ███",
            "███████ ███  ██  ██ █",
            "█     █  ███ ██   █  ",
            "█ ███ █ ██ █  █ █ ██ ",
            "█ ███ █ ██  █  ██    ",
            "█ ███ █  █ ██  ███ ██",
            "█     █      █ █ █ ██",
            "███████   ███ ███ ██ ",
        ])
    );
}

#[rstest]
fn render_fullblock_scaled_2x1(empty_qr: QrCode) {
    let w = FB::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_scaling(Scaling::Exact(2, 1))
        .with_aspect_ratio(AspectRatio::Computed);
    assert_eq!(w.size(Rect::ZERO), Size::from((42, 21)));
}

#[rstest]
fn render_halfblock_inverted(empty_qr: QrCode) {
    type LoD<'a> = QrWidget<'a, HalfBlock, LightOnDark>;
    let mut buf = Buffer::empty(Rect::new(0, 0, 21, 11));
    LoD::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_aspect_ratio(AspectRatio::Computed)
        .with_style(Style::default())
        .render(buf.area, &mut buf);
    // Inverted: █↔space, ▀↔▄
    assert_eq!(
        buf,
        Buffer::with_lines([
            " ▄▄▄▄▄ ██▄▀▀██ ▄▄▄▄▄ ",
            " █   █ █   ▄ █ █   █ ",
            " █▄▄▄█ ██ ▄ ▀█ █▄▄▄█ ",
            "▄▄▄▄▄▄▄█▄█▄█▄█▄▄▄▄▄▄▄",
            "▀▀█▄▄▀▄ ▀██▀ ██▀▀ ▄▀▀",
            "▄ █▀▀▄▄▄ ▀▀▀▄▄▄ ▀▄█▀▄",
            "▄█▄▄██▄█ ▀███▀▄▄▄▄   ",
            " ▄▄▄▄▄ █▄  ▀█  ██▄ █▄",
            " █   █ █  █▄▀█▄▀ █▄▄█",
            " █▄▄▄█ ██▄█▄▄▀█ ▄ █  ",
            "▄▄▄▄▄▄▄███▄▄▄█▄▄▄█▄▄█",
        ])
    );
}

#[rstest]
fn render_fullblock_inverted(empty_qr: QrCode) {
    type LoD<'a> = QrWidget<'a, FullBlock, LightOnDark>;
    let mut buf = Buffer::empty(Rect::new(0, 0, 21, 21));
    LoD::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_aspect_ratio(AspectRatio::Computed)
        .with_style(Style::default())
        .render(buf.area, &mut buf);
    // Inverted: █↔space
    assert_eq!(
        buf,
        Buffer::with_lines([
            "       ██ ████       ",
            " █████ ███  ██ █████ ",
            " █   █ █     █ █   █ ",
            " █   █ █   █ █ █   █ ",
            " █   █ ██   ██ █   █ ",
            " █████ ██ █  █ █████ ",
            "       █ █ █ █       ",
            "█████████████████████",
            "███  █  ████ ████  ██",
            "  ███ █  ██  ██   █  ",
            "  ███    ███    █ ██ ",
            "█ █  ███    ███  ██ █",
            " █  ██ █ █████       ",
            "████████  ███ ████   ",
            "       █   ██  ██  █ ",
            " █████ ██   █  ███ ██",
            " █   █ █  █ ██ █ █  █",
            " █   █ █  ██ ██  ████",
            " █   █ ██ █  ██   █  ",
            " █████ ██████ █ █ █  ",
            "       ███   █   █  █",
        ])
    );
}

#[rstest]
fn render_braille_exact(empty_qr: QrCode) {
    let mut buf = Buffer::empty(Rect::new(0, 0, 11, 6));
    unstyled::<BR>(&empty_qr).render(buf.area, &mut buf);
    assert_eq!(
        buf,
        Buffer::with_lines([
            "⡏⣭⡍⡇⣬⡶⡄⡏⣭⡍⡇",
            "⠧⠭⠥⠇⠜⠝⠆⠧⠭⠥⠇",
            "⢶⢈⡱⠽⣆⣐⠧⢴⡺⢑⠆",
            "⡥⠭⠤⡅⢷⣄⢰⡍⠩⡟⠇",
            "⡇⠿⠇⡇⠻⠨⢆⢱⢧⢩⡄",
            "⠉⠉⠉⠁⠀⠉⠁⠉⠁⠉⠀",
        ])
    );
}

#[rstest]
fn render_braille_inverted(empty_qr: QrCode) {
    type LoD<'a> = QrWidget<'a, Braille, LightOnDark>;
    let mut buf = Buffer::empty(Rect::new(0, 0, 11, 6));
    LoD::new(&empty_qr)
        .with_quiet_zone(QuietZone::Disabled)
        .with_aspect_ratio(AspectRatio::Computed)
        .with_style(Style::default())
        .render(buf.area, &mut buf);
    assert_eq!(
        buf,
        Buffer::with_lines([
            "⢰⠒⢲⢸⠓⢉⢻⢰⠒⢲⢸",
            "⣘⣒⣚⣸⣣⣢⣹⣘⣒⣚⣸",
            "⡉⡷⢎⣂⠹⠯⣘⡋⢅⡮⣹",
            "⢚⣒⣛⢺⡈⠻⡏⢲⣖⢠⣸",
            "⢸⣀⣸⢸⣄⣗⡹⡎⡘⡖⢻",
            "⣶⣶⣶⣾⣿⣶⣾⣶⣾⣶⣿",
        ])
    );
}

/// Shorthand: no QZ, Computed aspect, default style.
fn unstyled<'a, W>(qr: &'a QrCode) -> W
where
    W: From<UnsBuilder<'a>>,
{
    W::from(UnsBuilder(qr))
}

struct UnsBuilder<'a>(&'a QrCode);

impl<'a, B: Barcode, P: Polarity> From<UnsBuilder<'a>> for QrWidget<'a, B, P> {
    fn from(b: UnsBuilder<'a>) -> Self {
        Self::new(b.0)
            .with_quiet_zone(QuietZone::Disabled)
            .with_aspect_ratio(AspectRatio::Computed)
            .with_style(Style::default())
    }
}
