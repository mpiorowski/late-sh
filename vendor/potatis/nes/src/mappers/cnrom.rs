use common::kilobytes;
use mos6502::memory::Bus;

use super::MapperImpl;
use crate::cartridge::Cartridge;

const BANK_SIZE: usize = kilobytes::KB8;

// Mapper 3
pub struct CNROM {
  cart: Cartridge,
  selected_bank: usize,
  is_16kb: bool,
}

impl MapperImpl for CNROM {}

impl CNROM {
  pub fn new(cart: Cartridge) -> Self {
    let is_16kb = match cart.prg().len() {
      kilobytes::KB16 => true,
      kilobytes::KB32 => false,
      _ => panic!("invalid size for mapper 3 prg rom"),
    };

    Self {
      cart,
      selected_bank: 0,
      is_16kb,
    }
  }
}

impl Bus for CNROM {
  fn read8(&self, address: u16) -> u8 {
    match address {
      0x0000..=0x1fff => self.cart.chr()[(self.selected_bank * BANK_SIZE) + address as usize],
      0x8000..=0xffff => {
        if self.is_16kb {
          self.cart.prg()[address as usize - 0x8000 - kilobytes::KB16] // see tests
        } else {
          self.cart.prg()[address as usize - 0x8000]
        }
      }
      _ => self.cart.prg_ram()[address as usize],
    }
  }

  fn write8(&mut self, val: u8, address: u16) {
    match address {
      // 0x0000..=0x1fff => self.cart.chr_mut()[(self.selected_bank * BANK_SIZE) + address as usize] = val,
      0x8000..=0xffff => {
        self.selected_bank = (val & 0b00000011) as usize;
        // println!("mapper 3 selected bank: {}", self.selected_bank);
      }
      _ => self.cart.prg_ram_mut()[address as usize] = val,
    }
  }
}
