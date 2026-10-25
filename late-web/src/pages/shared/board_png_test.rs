use dartboard_core::{Canvas, Pos, RgbColor};

use super::board_png::{BACKGROUND, Crop, cell_size, crop, render};

/// Decode a PNG into its size and RGB bytes.
fn decode(png: Vec<u8>) -> (u32, u32, Vec<u8>) {
    let decoder = png::Decoder::new(std::io::Cursor::new(png));
    let mut reader = decoder.read_info().unwrap();
    let (width, height) = (reader.info().width, reader.info().height);
    let mut rgb = vec![0; reader.output_buffer_size().unwrap()];
    let frame = reader.next_frame(&mut rgb).unwrap();
    rgb.truncate(frame.buffer_size());
    (width, height, rgb)
}

/// A coloured full block fills its cell with its ink, an uncoloured glyph
/// draws in the default ink, an empty cell keeps the shell's background,
/// and a glyph no face carries draws nothing rather than failing.
#[test]
fn a_board_renders_its_cells_in_ink_on_the_shell_background() {
    let mut canvas = Canvas::with_size(4, 2);
    canvas.put_glyph_colored(Pos { x: 0, y: 0 }, '█', RgbColor::new(255, 0, 80));
    canvas.put_glyph(Pos { x: 1, y: 0 }, '█');
    canvas.put_glyph(Pos { x: 2, y: 0 }, '界');

    let (cell_w, cell_h) = cell_size(4);
    assert_eq!((cell_w, cell_h), (48, 96));
    let (width, height, rgb) = decode(render(&canvas).unwrap());
    assert_eq!((width, height), (192, 192));

    let pixel = |x: usize, y: usize| {
        let at = (y * width as usize + x) * 3;
        [rgb[at], rgb[at + 1], rgb[at + 2]]
    };
    let centre = |cell_x: usize, cell_y: usize| {
        pixel(cell_x * cell_w + cell_w / 2, cell_y * cell_h + cell_h / 2)
    };
    assert_eq!(centre(0, 0), [255, 0, 80]);
    assert_eq!(centre(1, 0), [0xd1, 0xd5, 0xdb]);
    assert_eq!(centre(2, 0), BACKGROUND);
    assert_eq!(centre(3, 1), BACKGROUND);
}

/// The Artboard's 384 cells come out four pixels wide, a 32-cell poster
/// forty-eight, and nothing leaves the preview-card range.
#[test]
fn cell_size_scales_a_board_to_preview_width() {
    assert_eq!(cell_size(384), (4, 8));
    assert_eq!(cell_size(256), (6, 12));
    assert_eq!(cell_size(32), (48, 96));
    assert_eq!(cell_size(1), (48, 96));
    assert_eq!(cell_size(2000), (2, 4));
}

/// An empty canvas is refused rather than encoded as a zero-size image.
#[test]
fn an_empty_canvas_is_refused() {
    assert!(render(&Canvas::with_size(0, 0)).is_err());
}

/// A crop moves the cells inside to the origin with their colours, leaves
/// out a wide glyph the edge would cut, and refuses to reach past the
/// canvas; `x,y,width,height` parses with no empty side.
#[test]
fn a_crop_keeps_the_cells_inside_and_refuses_past_the_edge() {
    let mut canvas = Canvas::with_size(8, 4);
    canvas.put_glyph_colored(Pos { x: 2, y: 1 }, 'a', RgbColor::new(255, 0, 80));
    canvas.put_glyph(Pos { x: 4, y: 2 }, '界');
    canvas.put_glyph(Pos { x: 0, y: 0 }, 'z');
    let rect = Crop::parse("2,1,3,2").expect("parses");
    assert_eq!(
        rect,
        Crop {
            x: 2,
            y: 1,
            width: 3,
            height: 2
        }
    );
    let cropped = crop(&canvas, rect).expect("inside");
    assert_eq!((cropped.width, cropped.height), (3, 2));
    let cells: Vec<(Pos, char)> = cropped
        .iter()
        .map(|(pos, _)| (*pos, cropped.get(*pos)))
        .collect();
    assert_eq!(
        cells,
        vec![(Pos { x: 0, y: 0 }, 'a')],
        "the wide glyph is cut, z is outside"
    );
    assert_eq!(
        cropped.fg(Pos { x: 0, y: 0 }),
        Some(RgbColor::new(255, 0, 80))
    );
    assert_eq!(crop(&canvas, Crop::parse("6,0,3,1").unwrap()), None);
    for bad in ["1,1,0,1", "1,1,1", "a,b,c,d", ""] {
        assert_eq!(Crop::parse(bad), None, "{bad}");
    }
}

/// A crop whose far edge overflows is refused like any crop past the
/// edge, never wrapped back into range: the wrapped crop's huge side
/// would ask the renderer for tens of gigabytes.
#[test]
fn a_crop_whose_edge_overflows_is_refused() {
    let canvas = Canvas::with_size(8, 4);
    for overflowing in [
        "18446744073709551615,0,1,1",
        "0,18446744073709551615,1,1",
        "18446744072709551616,0,1000000000,1",
    ] {
        let rect = Crop::parse(overflowing).expect("parses");
        assert_eq!(crop(&canvas, rect), None, "{overflowing}");
    }
}
