use alloc::vec::Vec;

type SetPixelFn = fn(&mut [u8], i: usize, rgb: (u8, u8, u8));

pub trait SetPixel {
  fn set_pixel(buf: &mut [u8], i: usize, rgb: (u8, u8, u8));
}

pub trait PixelFormat {
  const BYTES_PER_PIXEL: usize;
}

pub struct PixelFormatRGB888;

impl PixelFormat for PixelFormatRGB888 {
  const BYTES_PER_PIXEL: usize = 3;
}

impl SetPixel for PixelFormatRGB888 {
  fn set_pixel(buf: &mut [u8], i: usize, rgb: (u8, u8, u8)) {
    buf[i] = rgb.0;
    buf[i + 1] = rgb.1;
    buf[i + 2] = rgb.2;
  }
}

pub struct PixelFormatRGB565;

impl PixelFormat for PixelFormatRGB565 {
  const BYTES_PER_PIXEL: usize = 2;
}

impl SetPixel for PixelFormatRGB565 {
  fn set_pixel(buf: &mut [u8], i: usize, rgb: (u8, u8, u8)) {
    // RGB888 to 565
    let b = (rgb.2 as u16 >> 3) & 0x1f;
    let g = ((rgb.1 as u16 >> 2) & 0x3f) << 5;
    let r = ((rgb.0 as u16 >> 3) & 0x1f) << 11;

    // u16 to 2xu8
    let p = r | g | b;
    buf[i] = (p >> 8) as u8;
    buf[i + 1] = (p & 0xff) as u8;
  }
}

// Also PAL res
pub const NES_WIDTH: usize = 256;
pub const NES_HEIGHT: usize = 240;

pub const NTSC_WIDTH: usize = 240;
pub const NTSC_HEIGHT: usize = 224;
const NTSC_OVERSCAN_PIXELS: usize = 8;

pub struct RenderFrame {
  bytes_per_pixel: usize,
  buf: Vec<u8>,
  set_pixel_fn: SetPixelFn,
  pitch_ntsc: usize,
  pitch_pal: usize,
}

impl RenderFrame {
  pub fn new<FORMAT>() -> Self
  where
    FORMAT: PixelFormat + SetPixel + 'static,
  {
    Self {
      bytes_per_pixel: FORMAT::BYTES_PER_PIXEL,
      buf: vec![0; NES_WIDTH * NES_HEIGHT * FORMAT::BYTES_PER_PIXEL],
      set_pixel_fn: FORMAT::set_pixel,
      pitch_ntsc: NTSC_WIDTH * FORMAT::BYTES_PER_PIXEL,
      pitch_pal: NES_WIDTH * FORMAT::BYTES_PER_PIXEL,
    }
  }

  // The NES PPU always generates a 256x240 pixel picture.
  pub fn set_pixel_xy(&mut self, x: usize, y: usize, rgb: (u8, u8, u8)) {
    let i = ((y * NES_WIDTH) + x) * self.bytes_per_pixel;
    (self.set_pixel_fn)(&mut self.buf, i, rgb);
  }

  pub fn replace_buf(&mut self, buf: &[u8]) {
    self.buf = buf.to_vec();
  }

  pub fn pixels_pal(&self) -> &[u8] {
    &self.buf
  }

  // This is an iter to avoid allocations
  pub fn pixels_ntsc(&self) -> impl Iterator<Item = u8> + '_ {
    let buf_pitch = NES_WIDTH * self.bytes_per_pixel;
    let overscan_pitch = NTSC_OVERSCAN_PIXELS * self.bytes_per_pixel;
    self
      .buf
      .chunks(buf_pitch) // Chunk as rows
      .skip(NTSC_OVERSCAN_PIXELS) // Skip first X rows
      .map(move |row| &row[overscan_pitch..buf_pitch - overscan_pitch]) // Skip col edges
      .take(NTSC_HEIGHT)
      .flatten()
      .copied()
  }

  pub fn pitch_ntsc(&self) -> usize {
    self.pitch_ntsc
  }

  pub fn pitch_pal(&self) -> usize {
    self.pitch_pal
  }
}
