use alloc::boxed::Box;
use core::panic;

use common::kilobytes;
use mos6502::memory::Bus;

use super::MapperImpl;
use crate::cartridge::Cartridge;
use crate::cartridge::Mirroring;

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PrgBankMode {
  Swap8000FixC000_0 = 0,
  SwapC000Fix8000_1 = 1,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum ChrBankMode {
  TwoKbAt0000_0 = 0,
  TwoKbAt1000_1 = 1,
}

pub struct MMC3 {
  cart: Cartridge,

  prg_rom_banks_total: usize,
  prg_rom_bank_mode: PrgBankMode,

  chr_rom_bank_mode: ChrBankMode,
  mirroring_cb: Option<Box<dyn FnMut(&Mirroring)>>,

  registers: [u8; 8],
  register_to_update: u8, // 3 bits

  irq_enabled: bool,
  irq_latch: u8,
  irq_counter: u8,
  irq_reload: bool,
}

impl MMC3 {
  pub fn new(cart: Cartridge) -> Self {
    Self {
      prg_rom_banks_total: cart.prg().len() / kilobytes::KB8,
      cart,
      prg_rom_bank_mode: PrgBankMode::Swap8000FixC000_0,
      chr_rom_bank_mode: ChrBankMode::TwoKbAt0000_0,
      mirroring_cb: None,
      registers: [0; 8],
      register_to_update: 0,
      irq_enabled: false,
      irq_latch: 0,
      irq_counter: 0,
      irq_reload: false,
    }
  }

  // https://www.nesdev.org/wiki/MMC3#PRG_Banks
  fn read_prg(&self, address: u16) -> u8 {
    let second_last_bank = self.prg_rom_banks_total - 2;
    let d6 = self.prg_rom_bank_mode as u8;
    let bank = match (address, d6) {
      (0x8000..=0x9fff, 0) => self.registers[6] as usize,
      (0x8000..=0x9fff, 1) => second_last_bank,
      (0xa000..=0xbfff, _) => self.registers[7] as usize,
      (0xc000..=0xdfff, 0) => second_last_bank,
      (0xc000..=0xdfff, 1) => self.registers[6] as usize,
      (0xe000..=0xffff, _) => self.prg_rom_banks_total - 1,
      _ => panic!(),
    };

    // Remove top bank indexing bits - 0x1fff == 8kb - 1
    let offset = address as usize & 0x1fff;
    self.cart.prg()[(bank * kilobytes::KB8) + offset]
  }

  // https://www.nesdev.org/wiki/MMC3#CHR_Banks
  fn read_chr(&self, address: u16) -> u8 {
    let d7 = self.chr_rom_bank_mode as u8;

    // R0 and R1 ignore the bottom bit, as the value written still
    // counts banks in 1KB units but odd numbered banks can't be selected.
    let bank: u8 = match (address, d7) {
      (0x0000..=0x03FF, 0) => self.registers[0],
      (0x0000..=0x03FF, 1) => self.registers[2],
      (0x0400..=0x07FF, 0) => self.registers[0] | 1,
      (0x0400..=0x07FF, 1) => self.registers[3],
      (0x0800..=0x0BFF, 0) => self.registers[1],
      (0x0800..=0x0BFF, 1) => self.registers[4],
      (0x0C00..=0x0FFF, 0) => self.registers[1] | 1,
      (0x0C00..=0x0FFF, 1) => self.registers[5],
      (0x1000..=0x13FF, 0) => self.registers[2],
      (0x1000..=0x13FF, 1) => self.registers[0],
      (0x1400..=0x17FF, 0) => self.registers[3],
      (0x1400..=0x17FF, 1) => self.registers[0] | 1,
      (0x1800..=0x1BFF, 0) => self.registers[4],
      (0x1800..=0x1BFF, 1) => self.registers[1],
      (0x1C00..=0x1FFF, 0) => self.registers[5],
      (0x1C00..=0x1FFF, 1) => self.registers[1] | 1,
      _ => panic!(),
    };

    // Remove top bank indexing bits - 0x03ff == 1kb - 1
    let offset = address as usize & 0x3ff;
    let base = kilobytes::KB1 * bank as usize;
    self.cart.chr()[base + offset]
  }
}

impl MapperImpl for MMC3 {
  fn on_runtime_mirroring(&mut self, cb: Box<dyn FnMut(&Mirroring)>) {
    self.mirroring_cb = Some(cb);
  }

  fn irq(&mut self) -> bool {
    if self.irq_reload {
      self.irq_counter = self.irq_latch;
      self.irq_reload = false;
      return false;
    }

    if self.irq_counter == 0 {
      self.irq_counter = self.irq_latch;
      self.irq_enabled
    } else {
      self.irq_counter -= 1;
      false
    }
  }
}

impl Bus for MMC3 {
  fn read8(&self, address: u16) -> u8 {
    // println!("Read: {:#06x}", address);
    match address {
      0x0000..=0x1fff => self.read_chr(address),
      0x6000..=0x7fff => self.cart.prg_ram()[address as usize - 0x6000],
      0x8000..=0xffff => self.read_prg(address),
      _ => 0,
    }
  }

  fn write8(&mut self, val: u8, address: u16) {
    // println!("Write: {:#06x} {:#04x}", address, val);
    let even = address & 1 == 0;
    match address {
      // CPU $6000-$7FFF: 8 KB PRG RAM bank (optional)
      0x6000..=0x7fff => self.cart.prg_ram_mut()[address as usize - 0x6000] = val,

      // Registers
      0x8000..=0x9fff => {
        if even {
          // Bank select
          self.prg_rom_bank_mode = if val & 0x40 == 0 {
            PrgBankMode::Swap8000FixC000_0
          } else {
            PrgBankMode::SwapC000Fix8000_1
          };
          self.chr_rom_bank_mode = if val & 0x80 == 0 {
            ChrBankMode::TwoKbAt0000_0
          } else {
            ChrBankMode::TwoKbAt1000_1
          };
          self.register_to_update = val & 0b111;
        } else {
          // Bank data
          // R6 and R7 will ignore the top two bits,
          // R0 and R1 ignore the bottom bit
          self.registers[self.register_to_update as usize] = match self.register_to_update {
            0 | 1 => val & 0xfe,
            6 | 7 => val & 0x3f,
            _ => val,
          };
        }
      }
      0xa000..=0xbfff => {
        if even {
          let runtime_mirroring = if val & 1 == 1 {
            Mirroring::Horizontal
          } else {
            Mirroring::Vertical
          };

          if self.cart.mirroring() != runtime_mirroring {
            let cb = self
              .mirroring_cb
              .as_mut()
              .expect("mirroring changed, no one to tell");
            (*cb)(&runtime_mirroring)
          }
        }
        // Odd: PRG RAM protect
      }
      0xc000..=0xdfff => {
        if even {
          // Latch
          self.irq_latch = val;
        } else {
          // Reload
          self.irq_reload = true;
        }
      }
      0xe000..=0xffff => {
        if self.irq_enabled && even && self.irq_counter <= 1 {
          panic!("acknowledge any pending interrupts. ");
        }
        self.irq_enabled = !even;
      }
      _ => (),
    }
  }
}
