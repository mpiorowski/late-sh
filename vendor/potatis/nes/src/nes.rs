use alloc::rc::Rc;
use alloc::string::ToString;
use core::cell::RefCell;
use core::time::Duration;
use mos6502::cpu::AC;
use mos6502::cpu::SP;
use mos6502::cpu::X;
use mos6502::cpu::Y;

use mos6502::cpu::Cpu;
#[cfg(feature = "debugger")]
use mos6502::debugger::AttachedDebugger;
use mos6502::memory::Bus;
use mos6502::mos6502::Mos6502;

use crate::apu::Apu;
use crate::cartridge::Cartridge;
use crate::fonts;
use crate::frame::PixelFormatRGB565;
use crate::frame::PixelFormatRGB888;
use crate::frame::RenderFrame;
use crate::joypad::Joypad;
use crate::nesbus::NesBus;
use crate::ppu::ppu::Ppu;
use crate::ppu::ppu::TickEvent;

const NES_NATIVE_FPS: f32 = 60.0988;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmulationSpeed {
  /// Normal NES timing - respects authentic PPU timing (60.0988 FPS)
  Normal,
  /// Fast mode with integer multiplier (2 = 2x speed, 3 = 3x speed, etc.)
  Fast(u32),
  /// Uncapped - run as fast as possible, limited only by host system
  Uncapped,
}

#[derive(PartialEq, Eq)]
pub enum HostEvent {
  Shutdown,
  Reset,
  ChangeSpeed(EmulationSpeed),
  Nothing,
}

#[derive(PartialEq, Default)]
pub enum HostPixelFormat {
  #[default]
  Rgb888,
  Rgb565,
}

pub trait HostPlatform {
  fn render(&mut self, frame: &RenderFrame);
  fn poll_events(&mut self, joypad: &mut Joypad) -> HostEvent;

  fn elapsed_millis(&self) -> usize {
    // Not required. Up to platform to implement for FPS control.
    0
  }

  fn delay(&self, _: Duration) {
    // Not required. Up to platform to implement for FPS control.
  }

  fn set_vsync(&mut self, _enabled: bool) {
    // Not required. Up to platform to implement VSync control.
  }

  fn pixel_format(&self) -> HostPixelFormat {
    HostPixelFormat::default()
  }

  fn alloc_render_frame(&self) -> RenderFrame {
    match self.pixel_format() {
      HostPixelFormat::Rgb888 => RenderFrame::new::<PixelFormatRGB888>(),
      HostPixelFormat::Rgb565 => RenderFrame::new::<PixelFormatRGB565>(),
    }
  }

  fn audio_sample(&mut self, _sample: f32) {
    // Not required. Up to platform to implement audio output.
  }
}

#[derive(Default)]
pub struct HeadlessHost;
impl HostPlatform for HeadlessHost {
  fn render(&mut self, _: &RenderFrame) {}
  fn poll_events(&mut self, _: &mut Joypad) -> HostEvent {
    HostEvent::Nothing
  }
  fn elapsed_millis(&self) -> usize {
    0
  }
  fn delay(&self, _: Duration) {}
}

pub struct Nes<H: HostPlatform + 'static> {
  machine: Mos6502<NesBus>,
  ppu: Rc<RefCell<Ppu>>,
  apu: Rc<RefCell<Apu>>,
  host: H,
  joypad: Rc<RefCell<Joypad>>,
  timing: FrameTiming,
  show_fps: bool,
  shutdown: bool,
  emulation_speed: EmulationSpeed,
}

impl<H: HostPlatform + 'static> Nes<H> {
  pub fn insert(cartridge: Cartridge, host: H) -> Self {
    let mirroring = cartridge.mirroring();
    let rom_mapper = crate::mappers::for_cart(cartridge);

    let frame = host.alloc_render_frame();
    let ppu = Rc::new(RefCell::new(Ppu::new(rom_mapper.clone(), mirroring, frame)));
    let apu = Rc::new(RefCell::new(Apu::new()));
    let joypad = Rc::new(RefCell::new(Joypad::default()));
    let bus = NesBus::new(rom_mapper.clone(), ppu.clone(), apu.clone(), joypad.clone());

    let mut cpu = Cpu::new(bus);
    cpu.reset();

    let machine = Mos6502::new(cpu);

    Self {
      machine,
      ppu,
      apu,
      host,
      joypad,
      timing: FrameTiming::new(),
      shutdown: false,
      show_fps: false,
      emulation_speed: EmulationSpeed::Normal,
    }
  }

  pub fn tick(&mut self) {
    let cpu_cycles = self.machine.tick();

    let apu_irq_pending = {
      // Tick APU for each CPU cycle and generate samples at correct rate
      let mut apu = self.apu.borrow_mut();
      for _ in 0..cpu_cycles {
        apu.tick();
        
        // Check if we should generate a sample after each APU tick
        if apu.should_generate_sample() {
          let audio_sample = apu.output();
          self.host.audio_sample(audio_sample);
        }
      }

      // Handle DMC memory requests
      let dmc_requests = apu.get_dmc_memory_requests();
      for address in dmc_requests {
        let byte = self.bus().read8(address);
        apu.provide_dmc_sample(byte);
      }

      apu.irq_pending()
    };

    let (ppu_event, should_trigger_nmi) = {
      let mut ppu = self.ppu.borrow_mut();
      let ppu_event = ppu.tick(cpu_cycles * 3);
      let should_trigger_nmi = ppu.nmi_on_vblank();

      if ppu_event == TickEvent::EnteredVblank {
        if self.show_fps {
          let fps = self.timing.fps_avg(self.host.elapsed_millis());
          fonts::draw(fps.to_string().as_str(), (10, 10), ppu.frame_mut());
        }

        self.host.render(ppu.frame())
      }

      (ppu_event, should_trigger_nmi)
    };

    if ppu_event == TickEvent::EnteredVblank {
      let event = self.host.poll_events(&mut self.joypad.borrow_mut());
      self.handle_event(event);

      // Apply frame timing based on speed mode
      match self.emulation_speed {
        EmulationSpeed::Normal | EmulationSpeed::Fast(_) => {
          if let Some(delay) = self.timing.post_render(self.host.elapsed_millis()) {
            self.host.delay(delay);
          }
        }
        EmulationSpeed::Uncapped => {
          // No delay - run as fast as possible
        }
      }

      self.timing.post_delay(self.host.elapsed_millis());

      if should_trigger_nmi {
        self.machine.cpu.nmi();
      }
    }

    if ppu_event == TickEvent::TriggerIrq || apu_irq_pending {
      self.machine.cpu.irq();
    }
  }

  fn handle_event(&mut self, event: HostEvent) {
    match event {
      HostEvent::ChangeSpeed(new_speed) => {
        self.set_emulation_speed(new_speed);
      }
      HostEvent::Shutdown => self.shutdown = true,
      HostEvent::Reset => self.machine.cpu.reset(),
      HostEvent::Nothing => (),
    }
  }

  #[cfg(feature = "debugger")]
  pub fn debugger(&mut self) -> AttachedDebugger<NesBus> {
    self.machine.debugger()
  }

  pub fn cpu_cycles(&self) -> usize {
    self.machine.total_cycles
  }

  pub fn cpu(&self) -> &Cpu<NesBus> {
    &self.machine.cpu
  }

  pub fn cpu_mut(&mut self) -> &mut Cpu<NesBus> {
    &mut self.machine.cpu
  }

  pub fn bus(&self) -> &NesBus {
    &self.machine.cpu.bus
  }

  pub fn fps_max(&mut self, fps_max: usize) {
    self.timing.fps_max(fps_max);
  }

  pub fn show_fps(&mut self, show_fps: bool) {
    self.show_fps = show_fps;
  }

  pub fn set_emulation_speed(&mut self, speed: EmulationSpeed) {
    self.emulation_speed = speed;

    match speed {
      EmulationSpeed::Normal => {
        self.timing.fps_max(NES_NATIVE_FPS as usize);
      }
      EmulationSpeed::Fast(multiplier) => {
        let target_fps = (NES_NATIVE_FPS * multiplier as f32) as usize;
        self.timing.fps_max(target_fps.min(1000));
      }
      EmulationSpeed::Uncapped => {
        self.timing.fps_max(u32::MAX as usize);
      }
    }
  }

  pub fn powered_on(&self) -> bool {
    !self.shutdown
  }
}

struct FrameTiming {
  frame_n: usize,
  last_frame_timestamp: usize,
  frame_limit_ms: usize,
  frame_times: [usize; 60], // Store last 60 frame times for rolling average
  frame_times_index: usize,
}

impl FrameTiming {
  pub fn new() -> Self {
    Self {
      frame_n: 0,
      last_frame_timestamp: 0,
      frame_limit_ms: 1000 / NES_NATIVE_FPS as usize,
      frame_times: [0; 60],
      frame_times_index: 0,
    }
  }

  pub fn fps_max(&mut self, fps_max: usize) {
    self.frame_limit_ms = 1000 / fps_max;
  }

  pub fn fps_avg(&self, _elapsed: usize) -> usize {
    // Calculate rolling average of last 60 frames
    let valid_frames = self.frame_times.iter().filter(|&&time| time > 0).count();
    if valid_frames < 2 {
      return 0;
    }

    let total_time: usize = self.frame_times.iter().filter(|&&time| time > 0).sum();
    let avg_frame_time = total_time / valid_frames;

    if avg_frame_time > 0 {
      1000 / avg_frame_time // Convert ms per frame to FPS
    } else {
      0
    }
  }

  pub fn post_render(&mut self, elapsed: usize) -> Option<Duration> {
    if self.last_frame_timestamp != 0 {
      let ms_to_render_frame = elapsed - self.last_frame_timestamp;
      // println!("took: {}ms, target: {}ms", ms_to_render_frame, self.frame_limit_ms);
      if ms_to_render_frame < self.frame_limit_ms {
        return Some(Duration::from_millis(
          (self.frame_limit_ms - ms_to_render_frame) as u64,
        ));
      }
    }

    None
  }

  pub fn post_delay(&mut self, elapsed: usize) {
    // Record frame time for rolling average
    if self.last_frame_timestamp != 0 {
      let frame_time = elapsed - self.last_frame_timestamp;
      self.frame_times[self.frame_times_index] = frame_time;
      self.frame_times_index = (self.frame_times_index + 1) % self.frame_times.len();
    }

    self.frame_n += 1;
    self.last_frame_timestamp = elapsed;
  }
}

// mainly for nestest
impl<H: HostPlatform> core::fmt::Debug for Nes<H> {
  // A:00 X:00 Y:00 P:26 SP:FB PPU:  0,120 CYC:40
  fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    let c = self.cpu();
    let scanline = self.ppu.borrow_mut().scanline();
    let ppu_cycle = self.ppu.borrow_mut().cycle() + 21;
    let ppuw = 3;
    if ppu_cycle < 100 {
      write!(
        f,
        "{:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PPU:{:ppuw$}, {:>2} CYC:{}",
        c.pc,
        c.regs[AC],
        c.regs[X],
        c.regs[Y],
        c.flags.bits(),
        c.regs[SP],
        scanline,
        ppu_cycle,
        self.machine.total_cycles
      )
    } else {
      write!(
        f,
        "{:04X} A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X} PPU:{:ppuw$},{:>2} CYC:{}",
        c.pc,
        c.regs[AC],
        c.regs[X],
        c.regs[Y],
        c.flags.bits(),
        c.regs[SP],
        scanline,
        ppu_cycle,
        self.machine.total_cycles
      )
    }
  }
}
