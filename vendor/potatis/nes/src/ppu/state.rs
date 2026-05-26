#[derive(Default, PartialEq, Eq, Copy, Clone)]
pub(crate) enum Phase {
  PreRender,
  #[default]
  Render,
  PostRender,
  EnteringVblank,
  Vblank,
}

pub(crate) enum Rendering {
  Enabled,
  Disabled,
}

#[derive(Default)]
pub(crate) struct State {
  phase: Phase,
  cycle: usize,
  scanline: usize,
  clock: usize,
  odd_frame: bool,
}

impl State {
  pub fn next(&mut self, rendering_enabled: bool) -> (Phase, usize, Rendering) {
    // Optimized: maintain cycle and scanline incrementally instead of division/modulo
    self.cycle += 1;
    
    // Handle cycle wrapping and scanline increment
    if self.cycle >= 341 {
      self.cycle = 0;
      self.scanline += 1;
      
      // Handle scanline wrapping and frame toggling
      if self.scanline > 261 {
        self.scanline = 0;
        self.odd_frame = !self.odd_frame;
      }
    }
    
    // Optimized phase calculation using direct comparisons instead of range matching
    self.phase = if self.scanline == 261 {
      Phase::PreRender
    } else if self.scanline <= 239 {
      Phase::Render
    } else if self.scanline == 240 {
      Phase::PostRender
    } else if self.scanline == 241 {
      Phase::EnteringVblank
    } else {
      Phase::Vblank
    };

    // Handle odd frame skip for pre-render
    if self.phase == Phase::PreRender {
      if self.cycle == 339 && self.odd_frame && rendering_enabled {
        self.cycle = 0;
        self.scanline = 0;
        self.odd_frame = !self.odd_frame;
      } else if self.cycle == 340 {
        self.cycle = 0;
        self.scanline = 0;
        self.odd_frame = !self.odd_frame;
      }
    }

    // Pre-compute rendering enum to avoid repeated construction
    let rendering = if rendering_enabled {
      Rendering::Enabled
    } else {
      Rendering::Disabled
    };

    (self.phase, self.cycle, rendering)
  }

  pub fn even_frame(&self) -> bool {
    !self.odd_frame
  }

  pub fn scanline(&self) -> usize {
    self.scanline
  }

  pub fn cycle(&self) -> usize {
    self.cycle
  }

  #[allow(dead_code)]
  pub fn clock(&self) -> usize {
    self.clock
  }
}
