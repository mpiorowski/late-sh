use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use nes::frame::RenderFrame;
use nes::joypad::Joypad;
use nes::nes::{EmulationSpeed, HostEvent, HostPlatform, Nes};

/// A host platform that counts frames by tracking render() calls
struct BenchmarkHost {
  frame_count: Rc<RefCell<usize>>,
  time: Instant,
}

impl BenchmarkHost {
  fn new() -> (Self, Rc<RefCell<usize>>) {
    let frame_count = Rc::new(RefCell::new(0));
    let host = Self {
      frame_count: frame_count.clone(),
      time: Instant::now(),
    };
    (host, frame_count)
  }
}

impl HostPlatform for BenchmarkHost {
  fn render(&mut self, _frame: &RenderFrame) {
    *self.frame_count.borrow_mut() += 1;
  }

  fn poll_events(&mut self, _joypad: &mut Joypad) -> HostEvent {
    HostEvent::Nothing
  }

  // Not used since we run uncapped, but implemented in case we want to benchmark capped or native FPS
  fn elapsed_millis(&self) -> usize {
    self.time.elapsed().as_millis() as usize
  }

  // Not used since we run uncapped, but implemented in case we want to benchmark capped or native FPS
  fn delay(&self, _duration: Duration) {
    std::thread::sleep(_duration);
  }
}

#[derive(Debug)]
struct BenchmarkResults {
  /// Total frames rendered
  frames_rendered: usize,
  /// Total CPU cycles executed
  cpu_cycles: usize,
  /// Wall clock time elapsed
  wall_time: Duration,
  /// Average frames per second
  fps: f64,
  /// CPU cycles per frame
  cycles_per_frame: f64,
  /// CPU cycles per second
  cycles_per_second: f64,
}

impl BenchmarkResults {
  fn print_summary(&self) {
    println!("\n=== BENCHMARK RESULTS ===");
    println!("Frames rendered:      {}", self.frames_rendered);
    println!("CPU cycles:           {}", self.cpu_cycles);
    println!("Wall time:            {:.3}s", self.wall_time.as_secs_f64());
    println!("Average FPS:          {:.2}", self.fps);
    println!("Cycles per frame:     {:.0}", self.cycles_per_frame);
    println!("Cycles per second:    {:.0}", self.cycles_per_second);
    println!("=========================");
  }
}

/// Run a benchmark on the given ROM file
fn run_benchmark(rom_path: &str, max_duration: Duration) -> BenchmarkResults {
  println!("\nLoading ROM: {}", rom_path);

  let (benchmark_host, frame_counter) = BenchmarkHost::new();
  let cartridge =
    nes::cartridge::Cartridge::blow_dust(rom_path.into()).expect("failed to load ROM");

  let mut nes = Nes::insert(cartridge, benchmark_host);
  nes.set_emulation_speed(EmulationSpeed::Uncapped);

  let start_time = Instant::now();
  let start_cycles = nes.cpu_cycles();

  println!("Starting benchmark for {}s", max_duration.as_secs());

  while start_time.elapsed() < max_duration {
    nes.tick();
  }

  let end_time = Instant::now();
  let end_cycles = nes.cpu_cycles();

  let wall_time = end_time.duration_since(start_time);
  let total_cycles = end_cycles - start_cycles;
  let frames_rendered = *frame_counter.borrow();
  let fps = frames_rendered as f64 / wall_time.as_secs_f64();
  let cycles_per_frame = if frames_rendered > 0 {
    total_cycles as f64 / frames_rendered as f64
  } else {
    0.0
  };
  let cycles_per_second = total_cycles as f64 / wall_time.as_secs_f64();

  BenchmarkResults {
    frames_rendered,
    cpu_cycles: total_cycles,
    wall_time,
    fps,
    cycles_per_frame,
    cycles_per_second,
  }
}

/// Quick benchmark test - runs nestest for a short duration
///
/// Note: This test runs with opt-level=3 due to [profile.test] in workspace Cargo.toml
/// No need for --release flag.
#[test]
#[ignore = "benchmark test"]
fn benchmark_nestest_quick_life() {
  let results = run_benchmark(
    "../nes-cloud/included-roms/life.nes",
    Duration::from_secs(5),
  );

  results.print_summary();
}

#[test]
#[ignore = "benchmark test"]
fn benchmark_nestest_quick_nestest() {
  let results = run_benchmark(
    "../nes-cloud/included-roms/nestest.nes",
    Duration::from_secs(5),
  );

  results.print_summary();
}
