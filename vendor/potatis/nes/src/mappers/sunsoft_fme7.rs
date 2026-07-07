use alloc::boxed::Box;

use common::kilobytes;
use mos6502::memory::Bus;

use super::MapperImpl;
use crate::cartridge::{Cartridge, Mirroring};

pub struct SunsoftFme7 {
    cart: Cartridge,
    command: u8,
    chr_banks: [u8; 8],
    prg_banks: [u8; 3],
    prg_ram_control: u8,
    mirroring_cb: Option<Box<dyn FnMut(&Mirroring)>>,
}

impl SunsoftFme7 {
    pub fn new(cart: Cartridge) -> Self {
        Self {
            cart,
            command: 0,
            chr_banks: [0; 8],
            prg_banks: [0, 1, 2],
            prg_ram_control: 0,
            mirroring_cb: None,
        }
    }

    fn chr_index(&self, address: u16) -> usize {
        let bank = self.chr_banks[(address as usize) / kilobytes::KB1] as usize;
        ((bank * kilobytes::KB1) + ((address as usize) & 0x03ff)) % self.cart.chr().len()
    }

    fn read_prg_bank(&self, bank: usize, offset: usize) -> u8 {
        let prg_banks = self.cart.prg().len() / kilobytes::KB8;
        self.cart.prg()[((bank % prg_banks) * kilobytes::KB8) + offset]
    }

    fn runtime_mirroring(val: u8) -> Mirroring {
        match val & 0x03 {
            0 => Mirroring::Vertical,
            1 => Mirroring::Horizontal,
            2 => Mirroring::SingleScreenLower,
            _ => Mirroring::SingleScreenUpper,
        }
    }
}

impl MapperImpl for SunsoftFme7 {
    fn on_runtime_mirroring(&mut self, cb: Box<dyn FnMut(&Mirroring)>) {
        self.mirroring_cb = Some(cb);
    }
}

impl Bus for SunsoftFme7 {
    fn read8(&self, address: u16) -> u8 {
        match address {
            0x0000..=0x1fff => self.cart.chr()[self.chr_index(address)],
            0x6000..=0x7fff => {
                let offset = address as usize - 0x6000;
                if self.prg_ram_control & 0x80 != 0 {
                    self.cart.prg_ram()[offset]
                } else {
                    self.read_prg_bank((self.prg_ram_control & 0x3f) as usize, offset)
                }
            }
            0x8000..=0x9fff => {
                self.read_prg_bank(self.prg_banks[0] as usize, address as usize - 0x8000)
            }
            0xa000..=0xbfff => {
                self.read_prg_bank(self.prg_banks[1] as usize, address as usize - 0xa000)
            }
            0xc000..=0xdfff => {
                self.read_prg_bank(self.prg_banks[2] as usize, address as usize - 0xc000)
            }
            0xe000..=0xffff => {
                let last_bank = (self.cart.prg().len() / kilobytes::KB8) - 1;
                self.read_prg_bank(last_bank, address as usize - 0xe000)
            }
            _ => 0,
        }
    }

    fn write8(&mut self, val: u8, address: u16) {
        match address {
            0x0000..=0x1fff => {
                if self.cart.uses_chr_ram() {
                    let idx = self.chr_index(address);
                    self.cart.chr_ram()[idx] = val;
                }
            }
            0x6000..=0x7fff => {
                if self.prg_ram_control & 0xc0 == 0xc0 {
                    self.cart.prg_ram_mut()[address as usize - 0x6000] = val;
                }
            }
            0x8000..=0x9fff => self.command = val & 0x0f,
            0xa000..=0xbfff => match self.command {
                0x0..=0x7 => self.chr_banks[self.command as usize] = val,
                0x8 => self.prg_ram_control = val,
                0x9 => self.prg_banks[0] = val & 0x3f,
                0xa => self.prg_banks[1] = val & 0x3f,
                0xb => self.prg_banks[2] = val & 0x3f,
                0xc => {
                    let runtime_mirroring = Self::runtime_mirroring(val);
                    if self.cart.mirroring() != runtime_mirroring {
                        if let Some(cb) = self.mirroring_cb.as_mut() {
                            (*cb)(&runtime_mirroring);
                        }
                    }
                }
                _ => {}
            },
            _ => {}
        }
    }
}
