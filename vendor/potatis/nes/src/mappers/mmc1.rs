use core::panic;

use alloc::boxed::Box;
use common::kilobytes;
use mos6502::memory::Bus;

use super::MapperImpl;
use crate::cartridge::Cartridge;
use crate::cartridge::Mirroring;

#[derive(Debug, PartialEq, Eq)]
enum PrgBankMode {
  Switch32Kb,
  FixFirstLowerSwitchUpper,
  FixLastUpperSwitchLower,
}

impl From<u8> for PrgBankMode {
  fn from(i: u8) -> Self {
    match i {
      0 | 1 => PrgBankMode::Switch32Kb,
      2 => PrgBankMode::FixFirstLowerSwitchUpper,
      3 => PrgBankMode::FixLastUpperSwitchLower,
      _ => panic!(),
    }
  }
}

#[derive(Debug, PartialEq, Eq)]
enum ChrBankMode {
  Switch8Kb,
  SwitchTwo4KbBanks,
}

pub struct MMC1 {
  cart: Cartridge,

  prg_rom_bank_mode: PrgBankMode,
  prg_rom_bank_num: usize,
  selected_prg_bank: u8,

  chr_rom_bank_mode: ChrBankMode,
  selected_chr_bank_0: u8,
  selected_chr_bank_1: u8,
  mirroring_cb: Option<Box<dyn FnMut(&Mirroring)>>,

  num_shift_writes: u8,
  shift_register: u8,
}

impl MapperImpl for MMC1 {
  fn on_runtime_mirroring(&mut self, cb: Box<dyn FnMut(&Mirroring)>) {
    self.mirroring_cb = Some(cb);
  }
}

impl MMC1 {
  pub fn new(cart: Cartridge) -> Self {
    MMC1 {
      prg_rom_bank_num: cart.prg().len() / kilobytes::KB16,
      cart,
      // "seems to reliably power on in the last bank (by setting the "PRG ROM bank mode" to 3)"
      // https://www.nesdev.org/wiki/MMC1#Control_(internal,_$8000-$9FFF)
      prg_rom_bank_mode: PrgBankMode::FixLastUpperSwitchLower,
      chr_rom_bank_mode: ChrBankMode::Switch8Kb,
      selected_chr_bank_0: 0,
      selected_chr_bank_1: 0,
      shift_register: 0,
      num_shift_writes: 0,
      selected_prg_bank: 0,
      mirroring_cb: None,
    }
  }

  fn reset_shift_register(&mut self) {
    self.num_shift_writes = 0;
    self.shift_register = 0;
  }

  fn write_to_shift_register(&mut self, val: u8, address: u16) {
    if common::bits::is_signed(val) {
      self.reset_shift_register();
      return;
    }

    let bit_to_write = val & 1;

    // shift in bit, lsb first. max width of shift reg is 5 bits, so we only shift to bit 4.
    self.shift_register = (self.shift_register >> 1) | (bit_to_write << 4);

    self.num_shift_writes += 1;

    if self.num_shift_writes == 5 {
      match address {
        0x8000..=0x9fff => {
          // Control
          self.update_control_register(self.shift_register);
        }
        0xa000..=0xbfff => {
          // CHR bank 0
          self.switch_lower_chr_bank(self.shift_register)
        }
        0xc000..=0xdfff => {
          // CHR bank 1
          self.switch_upper_chr_bank(self.shift_register)
        }
        0xe000..=0xffff => {
          // PRG bank
          self.selected_prg_bank = self.shift_register & 0b01111;
        }
        _ => panic!("unknown register"),
      }

      self.reset_shift_register()
    }
  }

  fn switch_lower_chr_bank(&mut self, selected_bank: u8) {
    // https://www.nesdev.org/wiki/MMC1#iNES_Mapper_001
    match self.chr_rom_bank_mode {
      ChrBankMode::Switch8Kb => self.selected_chr_bank_0 = selected_bank >> 1,
      ChrBankMode::SwitchTwo4KbBanks => self.selected_chr_bank_0 = selected_bank,
    }
  }

  fn switch_upper_chr_bank(&mut self, selected_bank: u8) {
    // https://www.nesdev.org/wiki/MMC1#iNES_Mapper_001
    match self.chr_rom_bank_mode {
      ChrBankMode::Switch8Kb => (), // (ignored in 8 KB mode)
      ChrBankMode::SwitchTwo4KbBanks => self.selected_chr_bank_1 = selected_bank,
    }
  }

  fn update_control_register(&mut self, val: u8) {
    let runtime_mirroring = match val & 0b11 {
      0 => Mirroring::SingleScreenLower,
      1 => Mirroring::SingleScreenUpper,
      2 => Mirroring::Vertical,
      3 => Mirroring::Horizontal,
      _ => unreachable!(),
    };

    if self.cart.mirroring() != runtime_mirroring {
      let cb = self
        .mirroring_cb
        .as_mut()
        .expect("mirroring changed, no one to tell");
      (*cb)(&runtime_mirroring)
    }

    let chr_rom_bank_mode = (val & 0b10000) >> 4;
    self.chr_rom_bank_mode = match chr_rom_bank_mode {
      0 => ChrBankMode::Switch8Kb,
      _ => ChrBankMode::SwitchTwo4KbBanks,
    };

    let prg_rom_bank_mode = (val & 0b01100) >> 2;
    self.prg_rom_bank_mode = prg_rom_bank_mode.into();
  }

  fn lower_prg_bank(&self) -> &[u8] {
    let bank = match self.prg_rom_bank_mode {
      PrgBankMode::Switch32Kb => self.selected_prg_bank as usize >> 1,
      PrgBankMode::FixFirstLowerSwitchUpper => 0,
      PrgBankMode::FixLastUpperSwitchLower => self.selected_prg_bank as usize,
    };
    let bank_start = bank * kilobytes::KB16;
    &self.cart.prg()[bank_start..bank_start + kilobytes::KB16]
  }

  fn upper_prg_bank(&self) -> &[u8] {
    let bank = match self.prg_rom_bank_mode {
      PrgBankMode::Switch32Kb => (self.selected_prg_bank as usize >> 1) + 1,
      PrgBankMode::FixFirstLowerSwitchUpper => self.selected_prg_bank as usize,
      PrgBankMode::FixLastUpperSwitchLower => self.prg_rom_bank_num - 1,
    };
    let bank_start = bank * kilobytes::KB16;
    &self.cart.prg()[bank_start..bank_start + kilobytes::KB16]
  }

  fn lower_chr_bank(&self) -> &[u8] {
    let bank = match self.chr_rom_bank_mode {
      ChrBankMode::Switch8Kb => self.selected_chr_bank_0 as usize,
      ChrBankMode::SwitchTwo4KbBanks => self.selected_chr_bank_0 as usize,
    };
    let bank_start = bank * kilobytes::KB4;
    &self.cart.chr()[bank_start..bank_start + kilobytes::KB4]
  }

  fn upper_chr_bank(&self) -> &[u8] {
    let bank = match self.chr_rom_bank_mode {
      ChrBankMode::Switch8Kb => self.selected_chr_bank_0 as usize + 1,
      ChrBankMode::SwitchTwo4KbBanks => self.selected_chr_bank_1 as usize,
    };
    let bank_start = bank * kilobytes::KB4;
    &self.cart.chr()[bank_start..bank_start + kilobytes::KB4]
  }
}

impl Bus for MMC1 {
  fn read8(&self, address: u16) -> u8 {
    // println!("Read: {:#06x}", address);
    match address {
      // PPU
      0x0000..=0x0fff => self.lower_chr_bank()[address as usize],
      0x1000..=0x1fff => self.upper_chr_bank()[address as usize - 0x1000],

      // CPU
      0x6000..=0x7fff => self.cart.prg_ram()[address as usize - 0x6000],
      0x8000..=0xbfff => self.lower_prg_bank()[address as usize - 0x8000],
      0xc000..=0xffff => self.upper_prg_bank()[address as usize - 0xc000],
      // TODO: In most mappers, banks past the end of PRG or CHR ROM show up as mirrors of earlier banks.
      _ => 0, //panic!("unknown mmc1 memory range")
    }
  }

  fn write8(&mut self, val: u8, address: u16) {
    // println!("Write: {:#06x} {:#04x}", address, val);
    match address {
      // PPU
      0x0000..=0x1fff => self.cart.chr_ram()[address as usize] = val,

      // CPU
      0x6000..=0x7fff => self.cart.prg_ram_mut()[address as usize - 0x6000] = val,
      0x8000..=0xffff => self.write_to_shift_register(val, address),
      _ => (), //panic!("writing to rom: {:#06x}", address)
    }
  }
}
