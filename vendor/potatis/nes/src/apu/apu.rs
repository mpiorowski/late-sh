use alloc::vec::Vec;

use super::dmc::DmcChannel;
use super::filters::NesAudioFilters;
use super::noise::NoiseChannel;
use super::square::SquareWave;
use super::triangle::TriangleWave;

const FRAME_COUNTER_STEPS_4: [bool; 4] = [true, true, true, false];
const FRAME_COUNTER_STEPS_5: [bool; 5] = [true, true, true, false, false];

// Audio timing constants
const NES_CPU_FREQ_HZ: f32 = 1_789_773.0; // NTSC
const TARGET_SAMPLE_RATE_HZ: f32 = 44_100.0;
const FRAME_COUNTER_FREQ_HZ: f32 = 240.0;

#[derive(Default)]
pub struct Apu {
  square1: SquareWave,
  square2: SquareWave,
  triangle: TriangleWave,
  noise: NoiseChannel,
  dmc: DmcChannel,

  // Frame counter
  frame_counter: u16,
  frame_step: u8,
  frame_mode: bool, // false = 4-step, true = 5-step
  frame_irq_inhibit: bool,
  frame_irq_flag: bool,

  // CPU cycle tracking
  cycle_count: u32,

  // Audio sampling
  audio_accumulator: f32,

  // DMC memory requests
  dmc_memory_requests: Vec<u16>,

  // Audio filtering
  audio_filters: NesAudioFilters,
}

impl Apu {
  pub fn new() -> Self {
    Self {
      square1: SquareWave::new(0), // Channel 0
      square2: SquareWave::new(1), // Channel 1
      dmc_memory_requests: Vec::new(),
      audio_filters: NesAudioFilters::new(TARGET_SAMPLE_RATE_HZ),
      ..Default::default()
    }
  }

  pub fn read(&mut self, address: u16) -> u8 {
    match address {
      0x15 => {
        // Status register - bit set if length counter > 0
        let mut status = 0;
        if self.square1.length_counter() > 0 {
          status |= 0x01;
        }
        if self.square2.length_counter() > 0 {
          status |= 0x02;
        }
        if self.triangle.length_counter() > 0 {
          status |= 0x04;
        }
        if self.noise.length_counter() > 0 {
          status |= 0x08;
        }
        if self.dmc.bytes_remaining() > 0 {
          status |= 0x10;
        }
        if self.frame_irq_flag {
          status |= 0x40;
        }
        // Reading status clears frame IRQ flag
        self.frame_irq_flag = false;
        status
      }
      _ => 0,
    }
  }

  pub fn write(&mut self, address: u16, value: u8) {
    match address {
      // Square 1
      0x00 => self.square1.write_control(value),
      0x01 => self.square1.write_sweep(value),
      0x02 => self.square1.write_timer_low(value),
      0x03 => self.square1.write_timer_high(value),

      // Square 2
      0x04 => self.square2.write_control(value),
      0x05 => self.square2.write_sweep(value),
      0x06 => self.square2.write_timer_low(value),
      0x07 => self.square2.write_timer_high(value),

      // Triangle
      0x08 => self.triangle.write_control(value),
      0x09 => {} // Unused
      0x0A => self.triangle.write_timer_low(value),
      0x0B => self.triangle.write_timer_high(value),

      // Noise
      0x0C => self.noise.write_control(value),
      0x0D => {} // Unused
      0x0E => self.noise.write_period(value),
      0x0F => self.noise.write_length(value),

      // DMC
      0x10 => self.dmc.write_control(value),
      0x11 => self.dmc.write_load_counter(value),
      0x12 => self.dmc.write_sample_address(value),
      0x13 => self.dmc.write_sample_length(value),

      // Status
      0x15 => {
        self.square1.set_enabled((value & 0x01) != 0);
        self.square2.set_enabled((value & 0x02) != 0);
        self.triangle.set_enabled((value & 0x04) != 0);
        self.noise.set_enabled((value & 0x08) != 0);
        self.dmc.set_enabled((value & 0x10) != 0);
      }

      // Frame Counter
      0x17 => {
        self.frame_mode = (value & 0x80) != 0;
        self.frame_irq_inhibit = (value & 0x40) != 0;

        // Clear IRQ flag when inhibit flag is set
        if self.frame_irq_inhibit {
          self.frame_irq_flag = false;
        }

        // Reset frame counter
        self.frame_counter = 0;
        self.frame_step = 0;
      }

      _ => {}
    }
  }

  pub fn tick(&mut self) {
    self.cycle_count += 1;

    // APU channels (except DMC) run at half CPU speed
    if self.cycle_count % 2 == 0 {
      self.square1.tick_timer();
      self.square2.tick_timer();
      self.triangle.tick_timer();
      self.noise.tick_timer();
    }

    // DMC runs at CPU speed
    if let Some(address) = self.dmc.tick_timer() {
      self.dmc_memory_requests.push(address);
    }

    // Frame counter runs at FRAME_COUNTER_FREQ_HZ (240Hz)
    // 1789773 / 240 ≈ 7457.5 cycles per frame counter tick
    // More precise: every 7457.5 cycles = every 14915 half-cycles
    const FRAME_COUNTER_CYCLES: u32 = (NES_CPU_FREQ_HZ / FRAME_COUNTER_FREQ_HZ * 2.0) as u32;
    if self.cycle_count % FRAME_COUNTER_CYCLES < 2 {
      self.tick_frame_counter();
    }
  }

  pub fn should_generate_sample(&mut self) -> bool {
    self.audio_accumulator += TARGET_SAMPLE_RATE_HZ;
    if self.audio_accumulator >= NES_CPU_FREQ_HZ {
      self.audio_accumulator -= NES_CPU_FREQ_HZ;
      true
    } else {
      false
    }
  }

  fn tick_frame_counter(&mut self) {
    let should_clock = if self.frame_mode {
      // 5-step mode
      if self.frame_step < 5 {
        FRAME_COUNTER_STEPS_5[self.frame_step as usize]
      } else {
        false
      }
    } else {
      // 4-step mode
      if self.frame_step < 4 {
        FRAME_COUNTER_STEPS_4[self.frame_step as usize]
      } else {
        false
      }
    };

    if should_clock {
      // Clock envelope and triangle linear counter
      self.square1.tick_envelope();
      self.square2.tick_envelope();
      self.triangle.tick_linear_counter();
      self.noise.tick_envelope();

      // Clock length counters and sweep units on steps 1 and 3 (4-step) or 1 and 4 (5-step)
      if (self.frame_step == 1)
        || (!self.frame_mode && self.frame_step == 3)
        || (self.frame_mode && self.frame_step == 4)
      {
        self.square1.tick_length_counter();
        self.square2.tick_length_counter();
        self.triangle.tick_length_counter();
        self.noise.tick_length_counter();

        // Tick sweep units
        self.square1.tick_sweep();
        self.square2.tick_sweep();
      }
    }

    self.frame_step += 1;
    let max_steps = if self.frame_mode { 5 } else { 4 };
    if self.frame_step >= max_steps {
      self.frame_step = 0;
    }

    // Set IRQ flag at step 3 in 4-step mode (if IRQ not inhibited)
    if !self.frame_mode && self.frame_step == 3 && !self.frame_irq_inhibit {
      self.frame_irq_flag = true;
    }
  }

  pub fn output(&mut self) -> f32 {
    let square1_out = self.square1.output() as f32;
    let square2_out = self.square2.output() as f32;
    let triangle_out = self.triangle.output() as f32;
    let noise_out = self.noise.output() as f32;
    let dmc_out = self.dmc.output() as f32;

    let square_sum = square1_out + square2_out;

    // NES mixer formula
    let square_component = if square_sum == 0.0 {
      0.0
    } else {
      95.88 / (8128.0 / square_sum + 100.0)
    };

    let tnd_sum = triangle_out / 8227.0 + noise_out / 12241.0 + dmc_out / 22638.0;
    let tnd_component = if tnd_sum == 0.0 {
      0.0
    } else {
      159.79 / (1.0 / tnd_sum + 100.0)
    };

    let raw_output = square_component + tnd_component;

    // Apply NES hardware audio filters for authentic sound reproduction
    self.audio_filters.process(raw_output)
  }

  pub fn get_dmc_memory_requests(&mut self) -> Vec<u16> {
    core::mem::take(&mut self.dmc_memory_requests)
  }

  pub fn provide_dmc_sample(&mut self, byte: u8) {
    self.dmc.load_sample_byte(byte);
  }

  pub fn irq_pending(&self) -> bool {
    self.frame_irq_flag
  }
}
