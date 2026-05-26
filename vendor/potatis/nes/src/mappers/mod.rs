use alloc::boxed::Box;
use alloc::rc::Rc;
use core::cell::RefCell;

use mos6502::memory::Bus;

use crate::cartridge::Cartridge;
use crate::cartridge::Mirroring;

mod cnrom;
mod mmc1;
mod mmc3;
mod nrom;
mod uxrom;

pub trait MapperImpl: Bus {
  fn on_runtime_mirroring(&mut self, _: Box<dyn FnMut(&Mirroring)>) {}
  fn irq(&mut self) -> bool {
    false
  }
}

pub enum Mapper {
  Nrom(nrom::NROM),
  Mmc1(mmc1::MMC1),
  Uxrom(uxrom::UxROM),
  Cnrom(cnrom::CNROM),
  Mmc3(mmc3::MMC3),
}

impl Bus for Mapper {
  fn read8(&self, address: u16) -> u8 {
    match self {
      Mapper::Nrom(m) => m.read8(address),
      Mapper::Mmc1(m) => m.read8(address),
      Mapper::Uxrom(m) => m.read8(address),
      Mapper::Cnrom(m) => m.read8(address),
      Mapper::Mmc3(m) => m.read8(address),
    }
  }

  fn write8(&mut self, val: u8, address: u16) {
    match self {
      Mapper::Nrom(m) => m.write8(val, address),
      Mapper::Mmc1(m) => m.write8(val, address),
      Mapper::Uxrom(m) => m.write8(val, address),
      Mapper::Cnrom(m) => m.write8(val, address),
      Mapper::Mmc3(m) => m.write8(val, address),
    }
  }
}

impl MapperImpl for Mapper {
  fn on_runtime_mirroring(&mut self, callback: Box<dyn FnMut(&Mirroring)>) {
    match self {
      Mapper::Nrom(m) => m.on_runtime_mirroring(callback),
      Mapper::Mmc1(m) => m.on_runtime_mirroring(callback),
      Mapper::Uxrom(m) => m.on_runtime_mirroring(callback),
      Mapper::Cnrom(m) => m.on_runtime_mirroring(callback),
      Mapper::Mmc3(m) => m.on_runtime_mirroring(callback),
    }
  }

  fn irq(&mut self) -> bool {
    match self {
      Mapper::Nrom(m) => m.irq(),
      Mapper::Mmc1(m) => m.irq(),
      Mapper::Uxrom(m) => m.irq(),
      Mapper::Cnrom(m) => m.irq(),
      Mapper::Mmc3(m) => m.irq(),
    }
  }
}

pub(crate) fn for_cart(cart: Cartridge) -> Rc<RefCell<Mapper>> {
  let mapper = match cart.mapper_type() {
    crate::cartridge::MapperType::Nrom => Mapper::Nrom(nrom::NROM::new(cart)),
    crate::cartridge::MapperType::Mmc1 => Mapper::Mmc1(mmc1::MMC1::new(cart)),
    crate::cartridge::MapperType::Uxrom => Mapper::Uxrom(uxrom::UxROM::new(cart)),
    crate::cartridge::MapperType::Cnrom => Mapper::Cnrom(cnrom::CNROM::new(cart)),
    crate::cartridge::MapperType::Mmc3 => Mapper::Mmc3(mmc3::MMC3::new(cart)),
  };
  Rc::new(RefCell::new(mapper))
}
