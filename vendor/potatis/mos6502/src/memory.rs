use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ops::RangeInclusive;

const MEM_SIZE: usize = 0xffff + 1;

pub trait Bus {
  fn read8(&self, address: u16) -> u8;
  fn write8(&mut self, val: u8, address: u16);

  fn read_range(&self, range: RangeInclusive<u16>) -> Vec<u8> {
    range.map(|a| self.read8(a)).collect()
  }
}

pub struct Memory(Box<[u8; MEM_SIZE]>);

impl Memory {
  pub fn load(program: &[u8], base: u16) -> Self {
    let mut mem = Box::new([0x00; MEM_SIZE]);
    let base = base as usize;
    let end = base + program.len() - 1;
    mem[base..=end].copy_from_slice(program);
    Self(mem)
  }
}

impl Bus for Memory {
  fn read8(&self, address: u16) -> u8 {
    self.0[address as usize]
  }

  fn write8(&mut self, val: u8, address: u16) {
    self.0[address as usize] = val;
  }
}
