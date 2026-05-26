use common::kilobytes;
use mos6502::memory::Bus;

use super::MapperImpl;
use crate::cartridge::Cartridge;

pub struct UxROM {
  cart: Cartridge,
  bank: u8,
  num_banks: usize,
}

impl MapperImpl for UxROM {}

impl UxROM {
  pub fn new(cart: Cartridge) -> Self {
    Self {
      num_banks: cart.prg().len() / kilobytes::KB16,
      cart,
      bank: 0,
    }
  }
}

impl Bus for UxROM {
  fn read8(&self, address: u16) -> u8 {
    let address = address as usize;
    let selected_bank = self.bank as usize;
    let last_bank = self.num_banks - 1;
    match address {
      0x0000..=0x1fff => self.cart.chr()[address],
      0x8000..=0xbfff => self.cart.prg()[(selected_bank * kilobytes::KB16) + (address - 0x8000)],
      0xc000..=0xffff => self.cart.prg()[(last_bank * kilobytes::KB16) + (address - 0xc000)],
      _ => 0,
    }
  }

  fn write8(&mut self, val: u8, address: u16) {
    match address {
      0x0000..=0x1fff => self.cart.chr_ram()[address as usize] = val,
      0x8000..=0xffff => self.bank = val,
      _ => (),
    }
  }
}
