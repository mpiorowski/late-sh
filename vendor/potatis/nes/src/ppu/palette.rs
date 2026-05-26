const PALETTE_SIZE: usize = 32;

// AKA boot palette?
pub const BLARRG_PALETTE: [u8; PALETTE_SIZE] = [
  0x09, 0x01, 0x00, 0x01, 0x00, 0x02, 0x02, 0x0D, 0x08, 0x10, 0x08, 0x24, 0x00, 0x00, 0x04, 0x2C,
  0x09, 0x01, 0x34, 0x03, 0x00, 0x04, 0x00, 0x14, 0x08, 0x3A, 0x00, 0x02, 0x00, 0x20, 0x2C, 0x08,
];

// https://www.nesdev.org/wiki/PPU_palettes
static PALETTE_RGB: [(u8, u8, u8); 64] = [
  (101, 101, 101),
  (0, 45, 105),
  (19, 31, 127),
  (69, 19, 124),
  (96, 11, 98),
  (115, 10, 55),
  (113, 15, 7),
  (90, 26, 0),
  (52, 40, 0),
  (11, 52, 0),
  (0, 60, 0),
  (0, 61, 16),
  (0, 56, 64),
  (0, 0, 0),
  (0, 0, 0),
  (0, 0, 0),
  (174, 174, 174),
  (15, 99, 179),
  (64, 81, 208),
  (120, 65, 204),
  (167, 54, 169),
  (192, 52, 112),
  (189, 60, 48),
  (159, 74, 0),
  (109, 92, 0),
  (54, 109, 0),
  (7, 119, 4),
  (0, 121, 61),
  (0, 114, 125),
  (0, 0, 0),
  (0, 0, 0),
  (0, 0, 0),
  (254, 254, 255),
  (93, 179, 255),
  (143, 161, 255),
  (200, 144, 255),
  (247, 133, 250),
  (255, 131, 192),
  (255, 139, 127),
  (239, 154, 73),
  (189, 172, 44),
  (133, 188, 47),
  (85, 199, 83),
  (60, 201, 140),
  (62, 194, 205),
  (78, 78, 78),
  (0, 0, 0),
  (0, 0, 0),
  (254, 254, 255),
  (188, 223, 255),
  (209, 216, 255),
  (232, 209, 255),
  (251, 205, 253),
  (255, 204, 229),
  (255, 207, 202),
  (248, 213, 180),
  (228, 220, 168),
  (204, 227, 169),
  (185, 232, 184),
  (174, 232, 208),
  (175, 229, 234),
  (182, 182, 182),
  (0, 0, 0),
  (0, 0, 0),
];

pub struct Palette {
  data: [u8; PALETTE_SIZE],
}

impl Palette {
  pub fn new() -> Self {
    Self {
      data: BLARRG_PALETTE,
    }
  }

  pub fn write(&mut self, val: u8, address: u16) {
    let mirrored = Self::mirror(address) as usize;
    self.data[mirrored % PALETTE_SIZE] = val;
  }

  pub fn read(&self, address: u16) -> u8 {
    let mirrored = Self::mirror(address) as usize;
    self.data[mirrored % PALETTE_SIZE]
  }

  pub fn rgb_from_index(&self, index: u8) -> (u8, u8, u8) {
    PALETTE_RGB[self.data[index as usize] as usize % 64]
  }

  // 0x3f00..=0x3fff
  fn mirror(address: u16) -> u16 {
    // PPU mem layout mirroring
    let mirrored = match address {
      0x3f00..=0x3f1f => address,
      0x3f20..=0x3fff => 0x3f00 + (address % PALETTE_SIZE as u16),
      _ => panic!("invalid palette address: {:#06x}", address),
    };

    // Special palette crap: Addresses $3F10/$3F14/$3F18/$3F1C are mirrors of $3F00/$3F04/$3F08/$3F0C
    match mirrored {
      0x3f10 => 0x3f00,
      0x3f14 => 0x3f04,
      0x3f18 => 0x3f08,
      0x3f1c => 0x3f0c,
      _ => mirrored,
    }
  }
}

#[cfg(test)]
mod tests {
  use crate::ppu::palette::Palette;

  #[test]
  fn palette_mirror() {
    assert_eq!(Palette::mirror(0x3f00), 0x3f00);
    assert_eq!(Palette::mirror(0x3f1f), 0x3f1f);
    assert_eq!(Palette::mirror(0x3f20), 0x3f00);
    assert_eq!(Palette::mirror(0x3fff), 0x3f1f);
    assert_eq!(Palette::mirror(0x3f21), 0x3f01);
    assert_eq!(Palette::mirror(0x3f40), 0x3f00);

    assert_eq!(Palette::mirror(0x3f10), 0x3f00);
    assert_eq!(Palette::mirror(0x3f14), 0x3f04);
    assert_eq!(Palette::mirror(0x3f18), 0x3f08);
    assert_eq!(Palette::mirror(0x3f1c), 0x3f0c);
  }
}
