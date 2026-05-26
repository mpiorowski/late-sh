use alloc::rc::Rc;
use alloc::vec::Vec;
use core::cell::RefCell;

use mos6502::memory::Bus;

use super::palette::Palette;
use super::state::State;
use super::vram::Vram;
use crate::cartridge::Mirroring;
use crate::frame::RenderFrame;
use crate::mappers::{Mapper, MapperImpl};
use crate::ppu::state::Phase;
use crate::ppu::state::Rendering;

#[derive(Default, Clone, Copy, Debug)]
struct Sprite {
  pixels: [u8; 8], // Only 8 pixels per line
  priority: bool,  // Priority (0: in front of background; 1: behind background)
  x: u8,
  zero: bool,
}

#[derive(Debug)]
#[repr(u16)]
#[allow(dead_code)]
enum Register {
  Ctrl2000 = 0, // ... + base 0x2000
  Mask2001 = 1,
  Status2002 = 2,
  OamAddr2003 = 3,
  OamData2004 = 4,
  Scroll2005 = 5,
  Addr2006 = 6,
  Data2007 = 7,
  OamDma2008 = 8,
}

impl From<u16> for Register {
  fn from(n: u16) -> Register {
    unsafe { core::mem::transmute(n) }
  }
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum TickEvent {
  Nothing,
  EnteredVblank,
  TriggerIrq,
}

#[allow(dead_code)]
pub struct Ppu {
  vram: Vram,
  rom_mapper: Rc<RefCell<Mapper>>,
  palette: Palette,
  frame: RenderFrame,
  state: State,

  oam: [u8; 256],
  oam_address: u8,
  sprites: Vec<Sprite>, // AKA secondary OAM

  v: u16,     // Current VRAM address (15 bits)
  t: u16, // Temporary VRAM address (15 bits); can also be thought of as the address of the top left onscreen tile.
  fine_x: u8, // Fine X scroll (3 bits)
  w_latch: bool,

  in_vblank: bool,
  sprite_0_hit: bool,
  sprite_overflow: bool,

  data_buffer: u8,

  vram_addr_inc: u8,
  sprite_table_address_8: u16,
  sprite_size_16: bool,
  background_table_address: u16,
  nmi_at_start_of_vblank: bool,

  show_background: bool,
  show_background_left: bool,
  show_sprites: bool,
  show_sprites_left: bool,
  rendering_enabled: bool,
}

#[allow(dead_code)]
impl Ppu {
  pub fn new(
    mapper: Rc<RefCell<Mapper>>,
    cart_mirroring: Mirroring,
    frame: RenderFrame,
  ) -> Ppu {
    Ppu {
      vram: Vram::new(mapper.clone(), cart_mirroring),
      rom_mapper: mapper,
      palette: Palette::new(),
      frame,
      state: State::default(),

      oam: [0; 256],
      oam_address: 0,
      sprites: Vec::with_capacity(8),

      v: 0,
      t: 0,
      fine_x: 0,
      w_latch: true,

      in_vblank: false,
      sprite_0_hit: false,
      sprite_overflow: false,

      data_buffer: 0,

      vram_addr_inc: 0,
      sprite_table_address_8: 0x0000,
      sprite_size_16: false,
      background_table_address: 0x0000,
      nmi_at_start_of_vblank: false,

      show_background: false,
      show_background_left: false,
      show_sprites: false,
      show_sprites_left: false,
      rendering_enabled: false,
    }
  }

  pub fn cpu_read_register(&mut self, address: u16) -> u8 {
    // if address != 0x0002 {
    //   panic!("invalid read: {:#06x}", address);
    // }
    match Register::from(address) {
      Register::Status2002 => {
        let mut status = 0;
        if self.in_vblank {
          status |= 0x80;
        }
        if self.sprite_0_hit {
          status |= 0x40;
        }
        if self.sprite_overflow {
          status |= 0x20;
        }
        self.in_vblank = false;
        self.w_latch = true;
        status
      }
      Register::OamData2004 => self.oam[self.oam_address as usize],
      Register::Data2007 => {
        let address = self.v & 0x3fff; // 14 bits wide
        let value = match address {
          0x0000..=0x1fff => self.rom_mapper.borrow().read8(address), // CHR
          0x2000..=0x2fff => self.vram.read(address),
          0x3000..=0x3eff => self.vram.read(address - 0x1000),
          0x3f00..=0x3fff => self.palette.read(address),
          _ => panic!("invalid read: {:#06x}", address),
        };
        let return_value = match address {
          0..=0x3eff => self.data_buffer,
          _ => value, // Palette is not buffered
        };
        self.data_buffer = value;

        self.inc_v();
        return_value
      }
      _ => 0,
    }
  }

  pub fn cpu_write_register(&mut self, val: u8, address: u16) {
    match Register::from(address) {
      Register::Ctrl2000 => {
        self.vram_addr_inc = if val & 0x04 == 0x04 { 32 } else { 1 };
        self.sprite_table_address_8 = if val & 0x08 == 0x08 { 0x1000 } else { 0x0000 };
        self.background_table_address = if val & 0x10 == 0x10 { 0x1000 } else { 0x0000 };
        self.sprite_size_16 = val & 0x20 == 0x20;
        self.nmi_at_start_of_vblank = (val & 0x80) == 0x80;

        // t: ...GH.. ........ <- d: ......GH
        //    <used elsewhere> <- d: ABCDEF..
        self.t = (self.t & 0xf3ff) | ((val as u16 & 0x3) << 10);
      }
      Register::Mask2001 => {
        self.show_background_left = val & 0x02 == 0x02;
        self.show_sprites_left = val & 0x04 == 0x04;
        self.show_background = val & 0x08 == 0x08;
        self.show_sprites = val & 0x10 == 0x10;
        self.rendering_enabled = self.show_background || self.show_sprites;
      }
      Register::OamAddr2003 => self.oam_address = val,
      Register::OamData2004 => {
        self.oam[self.oam_address as usize] = val;
        self.oam_address = self.oam_address.wrapping_add(1);
      }
      Register::Scroll2005 => {
        if self.w_latch {
          let abcde = val as u16 >> 3;
          self.t &= 0xffe0;
          self.t |= abcde;
          self.fine_x = val & 0x7;
        } else {
          let abcde = val >> 3;
          let fgh = val & 0x7;
          self.t &= 0xc1f;
          self.t |= (fgh as u16) << 12;
          self.t |= (abcde as u16) << 5;
        }

        self.w_latch = !self.w_latch;
      }
      Register::Addr2006 => {
        if self.w_latch {
          let cdefgh = val & 0x3f;
          self.t &= 0xff;
          self.t |= (cdefgh as u16) << 8;
        } else {
          self.t &= 0xff00;
          self.t |= val as u16;
          self.v = self.t;
        }

        self.w_latch = !self.w_latch;
      }
      Register::Data2007 => {
        let address = self.v & 0x3fff; // It's only 14 bits wide
        match address {
          0x0000..=0x1fff => self.rom_mapper.borrow_mut().write8(val, address), // CHR RAM
          0x2000..=0x2fff => self.vram.write(val, address),
          0x3000..=0x3eff => self.vram.write(val, address - 0x1000),
          0x3f00..=0x3fff => self.palette.write(val, address),
          _ => (), //panic!("invalid write: {:#06x} for {:?}", address, kind)
        }
        self.inc_v();
      }
      _ => (),
    }
  }

  pub fn tick(&mut self, ppu_cycles_to_tick: usize) -> TickEvent {
    let vblank_pre_ticks = self.in_vblank;
    let mut irq = false;

    for _ in 0..ppu_cycles_to_tick {
      match self.state.next(self.rendering_enabled) {
        (Phase::PreRender, 1, _) => {
          self.in_vblank = false;
          self.sprite_0_hit = false;
          self.sprite_overflow = false;
        }
        (Phase::Render | Phase::PreRender, 256, Rendering::Enabled) => self.inc_y(),
        (Phase::Render | Phase::PreRender, 257, Rendering::Enabled) => {
          self.copy_horizontal_from_t_to_v()
        }
        (Phase::PreRender, 280..=304, Rendering::Enabled) => self.copy_vertical_from_t_to_v(),
        (Phase::Render, 0..=255, _) => {
          // Visible pixels
          let x = self.state.cycle();
          let y = self.state.scanline();
          let mut bg_pixel_drawn = false;
          let show_bg = self.show_background && (self.show_background_left || x >= 8);
          if show_bg {
            bg_pixel_drawn = self.render_background_pixel(x, y);
          }

          let sprites_visible = self.show_sprites && (self.show_sprites_left || x >= 8);
          if sprites_visible {
            self.render_sprite_pixel(x, y, bg_pixel_drawn);
          }
        }
        (Phase::Render, 320, _) => {
          // Load sprites for next line (sprite tile loading interval)
          // https://www.nesdev.org/wiki/PPU_rendering#Cycles_257-320
          // 320 is the end of sprite (secondary OAM) loading interval.
          self.load_sprites_for_next_scanline();
        }
        (Phase::EnteringVblank, 1, _) => self.in_vblank = true,
        (Phase::Render | Phase::PostRender, 260, Rendering::Enabled) => {
          irq = self.rom_mapper.borrow_mut().irq()
        }
        _ => (),
      }
    }

    if !vblank_pre_ticks && self.in_vblank {
      TickEvent::EnteredVblank
    } else if irq {
      TickEvent::TriggerIrq
    } else {
      TickEvent::Nothing
    }
  }

  fn render_background_pixel(&mut self, x: usize, y: usize) -> bool {
    let v = self.v;
    let fine_x = self.fine_x as u16;

    let scroll_x = (v & 0x1f) * 8 + fine_x;
    let scroll_y = ((v >> 5) & 0x1f) * 8 + (v >> 12);

    let mut virtual_x = x as u16 + scroll_x;
    let mut virtual_y = scroll_y;

    let mut virtual_nametable_index = (v >> 10) & 0x3;
    if virtual_x >= 256 {
      virtual_nametable_index ^= 0x1;
      virtual_x -= 256;
    }

    if virtual_y >= 240 {
      virtual_nametable_index ^= 0x2;
      virtual_y -= 240;
    }

    let vertical_tile: u16 = virtual_y >> 3; // Optimized: division by 8
    let horizontal_tile: u16 = virtual_x >> 3; // Optimized: division by 8

    let nametable_offset = vertical_tile * 32 + horizontal_tile;
    let nametable_entry = self
      .vram
      .read_indexed(virtual_nametable_index, nametable_offset);

    let vertical_attr = vertical_tile >> 2; // Optimized: division by 4
    let horizontal_attr = horizontal_tile >> 2; // Optimized: division by 4

    let attr_offset = 0x3c0 + vertical_attr * 8 + horizontal_attr;
    let attr = self.vram.read_indexed(virtual_nametable_index, attr_offset);

    let horizontal_box_pos = (horizontal_tile & 3) >> 1; // Optimized: modulo 4, then division by 2
    let vertical_box_pos = (vertical_tile & 3) >> 1; // Optimized: modulo 4, then division by 2

    let color_bits = (attr >> ((horizontal_box_pos * 2) + (vertical_box_pos * 4))) & 0x3;

    let chr_mapper = self.rom_mapper.borrow();
    let first_plane_byte = chr_mapper.read8(
      self.background_table_address + (nametable_entry as u16 * 0x10 + (virtual_y & 7)), // Optimized: modulo 8
    );
    let second_plane_byte = chr_mapper.read8(
      self.background_table_address + (nametable_entry as u16 * 0x10 + (virtual_y & 7) + 8), // Optimized: modulo 8
    );
    drop(chr_mapper);

    let first_plane_bit = first_plane_byte >> (7 - (virtual_x & 7)) & 0x1; // Optimized: modulo 8
    let second_plane_bit = second_plane_byte >> (7 - (virtual_x & 7)) & 0x1; // Optimized: modulo 8

    if first_plane_bit == 0 && second_plane_bit == 0 {
      // Transparent
      let rgb = self.palette.rgb_from_index(0);
      self.frame.set_pixel_xy(x, y, rgb);
      false
    } else {
      let palette_entry = first_plane_bit + (second_plane_bit * 2) + (color_bits * 4);
      let rgb = self.palette.rgb_from_index(palette_entry);
      self.frame.set_pixel_xy(x, y, rgb);
      true
    }
  }

  fn render_sprite_pixel(&mut self, x: usize, y: usize, bg_pixel_drawn: bool) {
    let x = x as u8;

    for sprite in self.sprites.iter() {
      if x < sprite.x || x >= (sprite.x.saturating_add(8)) {
        // Not in bounds
        continue;
      }

      let pixel = 7 - (x - sprite.x);
      let entry = sprite.pixels[pixel as usize];
      let transparent = (entry & 0x03) == 0;

      if !transparent {
        if !sprite.priority || !bg_pixel_drawn {
          let rgb = self.palette.rgb_from_index(0x10 + entry);
          self.frame.set_pixel_xy(x as usize, y, rgb);
        }

        // https://www.nesdev.org/wiki/PPU_OAM#Sprite_zero_hits
        let s0_hit_disabled =
          x == 255 || (x <= 7 && (!self.show_sprites_left || !self.show_background_left));

        if !s0_hit_disabled && !self.sprite_0_hit && sprite.zero && bg_pixel_drawn {
          self.sprite_0_hit = true;
        }
      }
    }
  }

  fn load_sprites_for_next_scanline(&mut self) {
    // Clear current sprites
    self.sprites.clear();

    let sprite_height = if self.sprite_size_16 { 16 } else { 8 };
    let next_line = self.state.scanline() as u8 + 1;
    let mut sprite_n = 0;

    for sprite in 0..64 {
      // there's 64 sprites in total
      let sprite_addr = sprite * 4; // each sprite is 4 bytes

      // https://www.nesdev.org/wiki/PPU_OAM
      // Hide a sprite by writing any values in $EF-$FF here
      if self.oam[sprite_addr] > 0xee {
        continue;
      }

      let sprite_y = self.oam[sprite_addr] + 1;
      if next_line >= sprite_y && next_line < (sprite_y + sprite_height) {
        if sprite_n >= 8 {
          self.sprite_overflow = true;
          break;
        }

        let (sprite_table, number) = if self.sprite_size_16 {
          let number = self.oam[sprite_addr + 1];
          let sprite_table_address_16 = if number & 1 == 1 { 0x1000 } else { 0x0000 };
          (sprite_table_address_16, number >> 1)
        } else {
          (self.sprite_table_address_8, self.oam[sprite_addr + 1])
        };

        let attr = self.oam[sprite_addr + 2];
        let x = self.oam[sprite_addr + 3];

        let vflip = (attr & 0x80) == 0x80;
        let tile_row = match vflip {
          true => sprite_height - 1 - (next_line - sprite_y),
          false => next_line - sprite_y,
        };

        let index = if self.sprite_size_16 {
          let bottom = if tile_row > 7 { 8 } else { 0 };
          number as u16 * 32 + tile_row as u16 + bottom
        } else {
          number as u16 * 16 + tile_row as u16
        };

        let address = sprite_table + index;
        let chr_mapper = self.rom_mapper.borrow();
        let first_plane = chr_mapper.read8(address);
        let second_plane = chr_mapper.read8(address + 8);
        drop(chr_mapper);

        // Read pixels for sprite row
        let mut pixels = [0u8; 8];
        let hflip = (attr & 0x40) == 0x40;
        for (mut i, p) in pixels.iter_mut().enumerate() {
          if hflip {
            i = 7 - i;
          }
          *p = ((first_plane >> i) & 0x1) | ((second_plane >> i & 0x1) << 1) | ((attr & 0x3) << 2);
        }

        self.sprites.push(Sprite {
          pixels,
          priority: (attr & 0x20) == 0x20,
          x,
          zero: sprite_n == 0,
        });

        sprite_n += 1;
      }
    }
  }

  fn read_chr_rom(&self, address: u16) -> u8 {
    self.rom_mapper.borrow().read8(address)
  }

  pub fn cpu_oam_dma(&mut self, mem: impl Iterator<Item = u8>) {
    // assert!(mem.len() == 256);
    for byte in mem {
      self.oam[self.oam_address as usize] = byte;
      self.oam_address = self.oam_address.wrapping_add(1);
    }
    // assert!(self.oam_address == 0x0000);
    self.oam_address = 0;

    if self.state.even_frame() {
      self.tick(513 * 3);
    } else {
      self.tick(514 * 3);
    }
  }

  fn inc_v(&mut self) {
    self.v += self.vram_addr_inc as u16;
  }

  fn inc_y(&mut self) {
    // https://www.nesdev.org/wiki/PPU_scrolling#Y_increment
    let mut v = self.v;
    if (v & 0x7000) != 0x7000 {
      v += 0x1000;
    } else {
      v &= !0x7000;
      let mut y = (v & 0x3e0) >> 5;
      if y == 29 {
        y = 0;
        v ^= 0x800
      } else if y == 31 {
        y = 0
      } else {
        y += 1
      }
      v = (v & !0x03e0u16) | (y << 5);
    }
    self.v = v;
  }

  fn copy_horizontal_from_t_to_v(&mut self) {
    // https://www.nesdev.org/wiki/PPU_scrolling#At_dot_257_of_each_scanline
    // v: ....A.. ...BCDEF <- t: ....A.. ...BCDEF
    let abcdef = self.t & 0x41f;
    self.v &= 0x7be0;
    self.v |= abcdef;
  }

  fn copy_vertical_from_t_to_v(&mut self) {
    // https://www.nesdev.org/wiki/PPU_scrolling#During_dots_280_to_304_of_the_pre-render_scanline_(end_of_vblank)
    // v: GHIA.BC DEF..... <- t: GHIA.BC DEF.....
    let ghiabcdef = self.t & 0x7be0;
    self.v &= 0x41f;
    self.v |= ghiabcdef;
  }

  pub fn frame(&self) -> &RenderFrame {
    &self.frame
  }

  pub fn frame_mut(&mut self) -> &mut RenderFrame {
    &mut self.frame
  }

  pub fn scanline(&self) -> usize {
    self.state.scanline()
  }

  pub fn cycle(&self) -> usize {
    self.state.cycle()
  }

  pub fn in_vblank(&self) -> bool {
    self.in_vblank
  }

  pub fn nmi_on_vblank(&self) -> bool {
    self.nmi_at_start_of_vblank
  }
}
