use alloc::boxed::Box;
use alloc::vec::Vec;
use core::fmt::Display;
use core::ops::Range;

use common::kilobytes;

use self::error::CartridgeError;

pub const MAGIC: [u8; 4] = [0x4e, 0x45, 0x53, 0x1a];
pub const HEADER_SIZE: usize = 16;
const PRG_ROM_BLOCK_SIZE: usize = kilobytes::KB16;
const CHR_ROM_BLOCK_SIZE: usize = kilobytes::KB8;

pub mod error {
  use alloc::string::String;

  #[derive(Debug)]
  pub enum CartridgeError {
    #[cfg(feature = "std")]
    IO(std::io::Error),
    InvalidCartridge(&'static str),
    NotYetImplemented(String),
  }

  #[cfg(feature = "std")]
  impl std::error::Error for CartridgeError {}

  impl core::fmt::Display for CartridgeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
      write!(f, "{:?}", self)
    }
  }

  #[cfg(feature = "std")]
  impl From<std::io::Error> for CartridgeError {
    fn from(e: std::io::Error) -> Self {
      CartridgeError::IO(e)
    }
  }
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct Header {
  magic: [u8; 4],
  prg_rom_blocks: u8,
  chr_rom_blocks: u8,
  flags6: u8,
  flags7: u8,
  flags8: u8,
  flags9: u8,
  flags10: u8,
  padding: [u8; 5],
}

impl Header {
  pub fn parse(bin: &[u8]) -> Result<Header, CartridgeError> {
    if bin.len() < HEADER_SIZE {
      return Err(CartridgeError::InvalidCartridge("strange size"));
    }

    let magic = &bin[0..4];
    if magic != MAGIC {
      return Err(CartridgeError::InvalidCartridge("magic"));
    }

    Ok(Header {
      magic: magic
        .try_into()
        .map_err(|_| CartridgeError::InvalidCartridge("magic 2"))?,
      prg_rom_blocks: bin[4],
      chr_rom_blocks: bin[5],
      flags6: bin[6],
      flags7: bin[7],
      flags8: bin[8],
      flags9: bin[9],
      flags10: bin[10],
      padding: bin[11..16]
        .try_into()
        .map_err(|_| CartridgeError::InvalidCartridge("padding"))?,
    })
  }

  pub fn total_size_excluding_header(&self) -> usize {
    (self.prg_rom_blocks as usize * PRG_ROM_BLOCK_SIZE)
      + (self.chr_rom_blocks as usize * CHR_ROM_BLOCK_SIZE)
  }
}

#[derive(Debug, PartialEq, Eq)]
enum Format {
  Nes2,
  Ines,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MapperType {
  Nrom = 0,
  Mmc1 = 1,
  Uxrom = 2,
  Cnrom = 3,
  Mmc3 = 4,
}

impl TryFrom<&Header> for MapperType {
  type Error = CartridgeError;

  fn try_from(header: &Header) -> Result<Self, Self::Error> {
    let id: u8 = (header.flags7 & 0xf0) | header.flags6 >> 4;
    match id {
      0 => Ok(MapperType::Nrom),
      1 => Ok(MapperType::Mmc1),
      2 => Ok(MapperType::Uxrom),
      3 => Ok(MapperType::Cnrom),
      4 => Ok(MapperType::Mmc3),
      _ => Err(CartridgeError::NotYetImplemented(format!("Mapper {}", id))),
    }
  }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Mirroring {
  Horizontal,
  Vertical,
  HardwiredFourScreen,
  SingleScreenUpper,
  SingleScreenLower,
}

#[derive(PartialEq, Eq)]
pub enum Rom {
  Heap(Vec<u8>),
  Embedded(&'static [u8]),
}

impl Rom {
  pub fn len(&self) -> usize {
    match self {
      Rom::Heap(v) => v.len(),
      Rom::Embedded(s) => s.len(),
    }
  }

  pub fn get(&self) -> &[u8] {
    match self {
      Rom::Heap(v) => v,
      Rom::Embedded(s) => s,
    }
  }
}

#[derive(PartialEq, Eq)]
pub struct Cartridge {
  rom: Rom,
  mirroring: Mirroring,
  prg: Range<usize>,
  chr: Range<usize>,
  chr_ram: Option<Box<[u8; CHR_ROM_BLOCK_SIZE]>>,
  // PRG RAM is optional for some mappers, but 8kb is wastable.
  // It's also used by some test ROMs anyways.
  prg_ram: Box<[u8; kilobytes::KB8]>,
  mapper: MapperType,
  format: Format,
  // Cached slices to avoid slice creation overhead on every access
  uses_chr_ram: bool,
}

impl Cartridge {
  #[cfg(feature = "std")]
  pub fn blow_dust(path: std::path::PathBuf) -> Result<Cartridge, CartridgeError> {
    let rom = std::fs::read(path)?;
    Cartridge::load(Rom::Heap(rom))
  }

  pub fn blow_dust_vec(rom: Vec<u8>) -> Result<Cartridge, CartridgeError> {
    Cartridge::load(Rom::Heap(rom))
  }

  pub fn blow_dust_no_heap(rom: &'static [u8]) -> Result<Cartridge, CartridgeError> {
    Cartridge::load(Rom::Embedded(rom))
  }

  pub fn load(rom: Rom) -> Result<Cartridge, CartridgeError> {
    let bin = rom.get();
    if bin.len() < HEADER_SIZE + PRG_ROM_BLOCK_SIZE || bin[0..4] != MAGIC {
      return Err(CartridgeError::InvalidCartridge("strange size"));
    }

    let header = Header::parse(bin)?;
    if header.magic != MAGIC {
      return Err(CartridgeError::InvalidCartridge("magic"));
    }

    let format = if (header.flags7 & 0x0c) == 0x08 {
      Format::Nes2
    } else {
      Format::Ines
    };
    // if format == Format::Nes2 {
    // return Err(CartridgeError::NotYetImplemented("NES 2.0".into()));
    // }

    let mapper = MapperType::try_from(&header)?;

    let skip_trainer = header.flags6 & 0b100 != 0;
    if skip_trainer || (header.flags6 & (1 << 3)) != 0 {
      return Err(CartridgeError::NotYetImplemented("Trainer".into()));
    }

    // if header.flags6 & 0b10 != 0 {
    //   return Err(CartridgeError::NotYetImplemented("Cartridge contains battery-backed PRG RAM ($6000-7FFF) or other persistent memory".into()));
    // }

    if header.flags6 & 0b1000 != 0 {
      return Err(CartridgeError::NotYetImplemented(
        "cartidge fiddles w VRAM address space..".into(),
      ));
    }

    let mut mirroring = match header.flags6 & 1 {
      1 => Mirroring::Vertical,
      _ => Mirroring::Horizontal,
    };

    if header.flags6 & 0b1000 != 0 {
      mirroring = Mirroring::HardwiredFourScreen
    }

    let prg_size = (header.prg_rom_blocks as usize) * PRG_ROM_BLOCK_SIZE;
    let prg_start = HEADER_SIZE;
    let prg_end = prg_start + prg_size;

    let uses_chr_ram = header.chr_rom_blocks == 0;
    let chr_range = if uses_chr_ram {
      0..CHR_ROM_BLOCK_SIZE
    } else {
      let chr_start = prg_end;
      let chr_size = (header.chr_rom_blocks as usize) * CHR_ROM_BLOCK_SIZE;
      let chr_end = chr_start + chr_size;
      chr_start..chr_end
    };

    let chr_ram = uses_chr_ram.then_some(Box::new([0; CHR_ROM_BLOCK_SIZE]));

    Ok(Cartridge {
      prg: prg_start..prg_end,
      chr: chr_range,
      rom,
      mirroring,
      mapper,
      format,
      chr_ram,
      prg_ram: Box::new([0; kilobytes::KB8]),
      uses_chr_ram,
    })
  }

  pub fn mirroring(&self) -> Mirroring {
    self.mirroring
  }

  pub fn prg(&self) -> &[u8] {
    // Optimized: direct slice access without range creation
    unsafe {
      let rom_data = self.rom.get();
      alloc::slice::from_raw_parts(
        rom_data.as_ptr().add(self.prg.start),
        self.prg.end - self.prg.start,
      )
    }
  }

  pub fn chr(&self) -> &[u8] {
    // Optimized: remove branch by using cached flag
    if self.uses_chr_ram {
      &self.chr_ram.as_ref().unwrap()[..]
    } else {
      unsafe {
        let rom_data = self.rom.get();
        alloc::slice::from_raw_parts(
          rom_data.as_ptr().add(self.chr.start),
          self.chr.end - self.chr.start,
        )
      }
    }
  }

  pub fn chr_ram(&mut self) -> &mut [u8] {
    &mut self.chr_ram.as_mut().unwrap()[..]
  }

  pub fn prg_ram_mut(&mut self) -> &mut [u8] {
    &mut self.prg_ram.as_mut()[..]
  }

  pub fn prg_ram(&self) -> &[u8] {
    &self.prg_ram[..]
  }

  pub fn mapper_type(&self) -> MapperType {
    self.mapper
  }
}

impl Display for Cartridge {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let chr_ram_or_rom = if self.chr_ram.is_some() { " RAM" } else { "" };
    write!(
      f,
      "[{:?}] Mapper: {:?}, Mirroring: {:?}, CHR{}: {}x{}K, PRG: {}x{}K",
      self.format,
      self.mapper,
      self.mirroring,
      chr_ram_or_rom,
      self.chr().len() / CHR_ROM_BLOCK_SIZE,
      CHR_ROM_BLOCK_SIZE / 1000,
      self.prg().len() / PRG_ROM_BLOCK_SIZE,
      PRG_ROM_BLOCK_SIZE / 1000
    )
  }
}

impl core::fmt::Debug for Cartridge {
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(f, "{self}")
  }
}

#[cfg(test)]
mod tests {
  use alloc::string::ToString;

  use super::Cartridge;
  use crate::cartridge::Rom;

  fn assert_cart(r: &'static [u8], s: &str) {
    assert_eq!(Cartridge::load(Rom::Embedded(r)).unwrap().to_string(), s);
  }

  #[test]
  fn cart_invalid_len() {
    assert!(Cartridge::load(Rom::Embedded(&[b'N', b'E', b'S'])).is_err())
  }

  #[test]
  fn cart_valid_nrom() {
    assert_cart(
      include_bytes!("../../test-roms/nestest/nestest.nes"),
      "[Ines] Mapper: Nrom, Mirroring: Horizontal, CHR: 1x8K, PRG: 1x16K",
    );
  }

  #[test]
  fn cart_valid_mmc1() {
    assert_cart(
      include_bytes!("../../test-roms/nes-test-roms/instr_test-v5/official_only.nes"),
      "[Ines] Mapper: Mmc1, Mirroring: Vertical, CHR RAM: 1x8K, PRG: 16x16K",
    );
  }

  #[test]
  fn cart_valid_nrom_chr_ram() {
    assert_cart(
      include_bytes!("../../test-roms/nes-test-roms/blargg_ppu_tests_2005.09.15b/vram_access.nes"),
      "[Ines] Mapper: Nrom, Mirroring: Horizontal, CHR RAM: 1x8K, PRG: 1x16K",
    );
  }
}
