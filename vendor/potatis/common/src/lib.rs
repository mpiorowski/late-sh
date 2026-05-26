#![no_std]

pub mod kilobytes {
  pub const KB1: usize = 1024;
  pub const KB2: usize = 2048;
  pub const KB4: usize = 4096;
  pub const KB8: usize = 8192;
  pub const KB16: usize = 16384;
  pub const KB32: usize = 32768;
}

pub mod utils {
  pub fn parse_hex(src: &str) -> core::result::Result<u16, core::num::ParseIntError> {
    u16::from_str_radix(src, 16)
  }
}

#[allow(dead_code)]
pub mod bits {
  pub fn is_signed(n: u8) -> bool {
    n & (1 << 7) != 0
  }

  pub fn is_overflow(res: u8, lhs: u8, rhs: u8) -> bool {
    if is_signed(lhs) && is_signed(rhs) && !is_signed(res) {
      true
    } else {
      !is_signed(lhs) && !is_signed(rhs) && is_signed(res)
    }
  }
}
