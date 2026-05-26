use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;

use common::kilobytes;

use crate::cartridge::Mirroring;
use crate::mappers::{Mapper, MapperImpl};

pub(crate) struct Vram {
  nametables: [[u8; kilobytes::KB1]; 2], // AKA CIRAM
  mirror_map: Rc<RefCell<[u8; 4]>>,
}

impl Vram {
  pub fn new(mapper: Rc<RefCell<Mapper>>, cart_mirroring: Mirroring) -> Self {
    let mirror_map = Self::setup_mirror_map(&cart_mirroring);
    let mirror_map = Rc::new(RefCell::new(mirror_map));

    let m = mirror_map.clone();
    mapper
      .borrow_mut()
      .on_runtime_mirroring(Box::new(move |new_mirroring| {
        *m.borrow_mut() = Self::setup_mirror_map(new_mirroring);
      }));

    Self {
      nametables: [[0; kilobytes::KB1]; 2],
      mirror_map,
    }
  }

  pub fn setup_mirror_map(new_mirroring: &Mirroring) -> [u8; 4] {
    // https://www.nesdev.org/wiki/PPU_nametables
    // https://www.nesdev.org/wiki/Mirroring#Nametable_Mirroring
    match new_mirroring {
      Mirroring::Vertical => [0, 1, 0, 1],
      Mirroring::Horizontal => [0, 0, 1, 1],
      Mirroring::SingleScreenLower => [0, 0, 0, 0],
      Mirroring::SingleScreenUpper => [1, 1, 1, 1],
      _ => panic!(),
    }
  }

  pub fn read(&self, address: u16) -> u8 {
    assert!(
      (0x2000..=0x2fff).contains(&address),
      "Invalid vram read: {:#06x}",
      address
    );

    let virtual_index = Self::get_virtual_nametable_index(address);
    let index = self.mirror_map.borrow()[virtual_index] as usize;
    let offset = address & 0x3ff; // Only lower 9 bits, higher are indexing
    self.nametables[index][offset as usize]
  }

  pub fn write(&mut self, val: u8, address: u16) {
    assert!(
      (0x2000..=0x2fff).contains(&address),
      "Invalid vram write: {:#06x}",
      address
    );

    let virtual_index = Self::get_virtual_nametable_index(address);
    let index = self.mirror_map.borrow()[virtual_index] as usize;
    let offset = address & 0x3ff; // Only lower 9 bits, higher are indexing
    self.nametables[index][offset as usize] = val;
  }

  pub fn read_indexed(&self, virtual_index: u16, offset: u16) -> u8 {
    let index = self.mirror_map.borrow()[virtual_index as usize] as usize;
    self.nametables[index][offset as usize]
  }

  fn get_virtual_nametable_index(address: u16) -> usize {
    // (0 = $2000; 1 = $2400; 2 = $2800; 3 = $2C00)
    // Start == 0x2000, bit 11 & 10 selects the nametable index.
    ((address >> 10) & 0b11) as usize
  }
}

#[cfg(test)]
mod tests {
  use crate::ppu::vram::Vram;

  #[test]
  fn get_virtual_nametable_index() {
    assert_eq!(Vram::get_virtual_nametable_index(0x2000), 0);
    assert_eq!(Vram::get_virtual_nametable_index(0x2400), 1);
    assert_eq!(Vram::get_virtual_nametable_index(0x2800), 2);
    assert_eq!(Vram::get_virtual_nametable_index(0x2c00), 3);
    assert_eq!(Vram::get_virtual_nametable_index(0x24ff), 1);
  }
}
