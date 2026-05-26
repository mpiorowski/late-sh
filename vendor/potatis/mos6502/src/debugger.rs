use alloc::vec::Vec;
use core::marker::PhantomData;
use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt::Write;
use std::ops::RangeInclusive;

use getch::Getch;

use crate::cpu::Cpu;
use crate::cpu::Flag;
use crate::cpu::AC;
use crate::cpu::SP;
use crate::cpu::X;
use crate::cpu::Y;
use crate::instructions::Instruction;
use crate::instructions::Opcode;
use crate::memory::Bus;

const BACKTRACE_LIMIT: usize = 11;

pub struct Debugger<B> {
  stdin: Getch,
  breakpoints: Vec<Breakpoint>,
  last_pc: Option<u16>,
  suspended: bool,
  verbose: bool,
  backtrace: VecDeque<BacktraceEntry>,
  watches: Vec<Watch>,
  opcodes: HashMap<&'static Opcode, usize>,
  _pd: PhantomData<B>,
}

pub struct AttachedDebugger<'cpu, B> {
  debugger: &'cpu mut Debugger<B>,
  cpu: &'cpu mut Cpu<B>,
}

struct BacktraceEntry {
  inst: &'static Instruction,
  pc: u16,
  opbyte: u8,
}

#[derive(PartialEq, Eq)]
pub enum Breakpoint {
  Address(u16),
  Opcode(String),
  OpcodeSequence(Vec<&'static str>), // TODO add support to break on opcode WITH operands
}

enum Watch {
  Range {
    address: RangeInclusive<u16>,
    state: Option<Vec<u8>>,
    f: Box<dyn Fn(Vec<u8>)>,
  },
  Address {
    address: u16,
    state: Option<u8>,
    f: Box<dyn Fn(u8)>,
  },
  // TODO: Reg, Flag, PC watches
}

impl<B: Bus> Default for Debugger<B> {
  fn default() -> Self {
    Self::new()
  }
}

impl<B: Bus> Debugger<B> {
  pub fn new() -> Self {
    Self {
      stdin: Getch::new(),
      breakpoints: Vec::with_capacity(2),
      last_pc: None,
      suspended: false,
      verbose: false,
      backtrace: VecDeque::with_capacity(BACKTRACE_LIMIT),
      watches: Vec::new(),
      opcodes: HashMap::new(),
      _pd: PhantomData,
    }
  }

  pub fn attach<'a>(&'a mut self, cpu: &'a mut Cpu<B>) -> AttachedDebugger<'a, B> {
    AttachedDebugger {
      debugger: self,
      cpu,
    }
  }
  pub fn add_breakpoint(&mut self, breakpoint: Breakpoint) {
    let mut breakpoint = breakpoint;
    if let Breakpoint::Opcode(opstr) = &breakpoint {
      breakpoint = Breakpoint::Opcode(opstr.to_uppercase());
    }
    self.breakpoints.push(breakpoint);
  }

  pub(crate) fn on_tick(&mut self, cpu: &Cpu<B>, next_inst: &'static Instruction) {
    let pc = cpu.pc;
    let opbyte = cpu.bus.read8(pc);

    *self.opcodes.entry(&next_inst.opcode).or_insert(0) += 1;

    self.backtrace.push_back(BacktraceEntry {
      inst: next_inst,
      pc,
      opbyte,
    });
    if self.backtrace.len() == BACKTRACE_LIMIT {
      self.backtrace.remove(0);
    }

    if self.suspended || self.verbose {
      Debugger::print_instruction(&cpu.bus, pc, opbyte, next_inst);
    }

    self.check_watches(cpu);

    if self.suspended {
      self.user_input(cpu);
    } else if self.is_breakpoint(pc, &next_inst.opcode) {
      self.suspend(cpu, pc);
    }

    self.last_pc = Some(pc);
  }

  fn is_breakpoint(&self, pc: u16, opcode: &Opcode) -> bool {
    for b in &self.breakpoints {
      match b {
        Breakpoint::Address(addr) => {
          if *addr == pc {
            return true;
          }
        }
        Breakpoint::Opcode(opstr) => {
          if *opstr == opcode.to_string() {
            return true;
          }
        }
        Breakpoint::OpcodeSequence(seq) => {
          let history: Vec<String> = self
            .backtrace
            .iter()
            .rev()
            .take(seq.len())
            .map(|b| b.inst.opcode.to_string())
            .collect();
          let upper: Vec<String> = seq.iter().rev().map(|&s| s.to_uppercase()).collect();
          if history == upper {
            return true;
          }
        }
      }
    }
    false
  }

  pub fn watch_memory_range(&mut self, range: RangeInclusive<u16>, f: impl Fn(Vec<u8>) + 'static) {
    let watch = Watch::Range {
      address: range,
      state: None,
      f: Box::new(f),
    };
    self.watches.push(watch)
  }

  pub fn watch_memory(&mut self, address: u16, f: impl Fn(u8) + 'static) {
    let watch = Watch::Address {
      address,
      state: None,
      f: Box::new(f),
    };
    self.watches.push(watch)
  }

  fn check_watches(&mut self, cpu: &Cpu<impl Bus>) {
    for watch in self.watches.iter_mut() {
      match watch {
        Watch::Range { address, state, f } => {
          let current_state: Vec<u8> = cpu.bus.read_range(address.clone());
          if state.as_ref() != Some(&current_state) {
            *state = Some(current_state.clone());
            f(current_state);
          }
        }
        Watch::Address { address, state, f } => {
          let current_state = cpu.bus.read8(*address);
          if *state != Some(current_state) {
            *state = Some(current_state);
            f(current_state);
          }
        }
      }
    }
  }

  fn dump_backtrace(&mut self, cpu: &Cpu<impl Bus>) {
    println!("...");
    for entry in self.backtrace.iter() {
      Debugger::print_instruction(&cpu.bus, entry.pc, entry.opbyte, entry.inst);
    }
  }

  fn suspend(&mut self, cpu: &Cpu<B>, address: u16) {
    self.suspended = true;
    if !self.verbose {
      // Print some instructions if we hit a break and we're not verbose already.
      self.dump_backtrace(cpu);
    }
    println!("break at {:#06x}. step: <space>, cpu: <enter>, stack: <s>, continue: <c>, mute & continue: <m>", address);
    self.user_input(cpu);
  }

  fn user_input(&mut self, cpu: &Cpu<B>) {
    let ch = self.stdin.getch().unwrap();
    match ch {
      0x20 => (), // Space, step
      0x0a => {
        // Enter
        println!("{:?}", cpu);
        println!("{}", cpu);
        self.user_input(cpu);
      }
      b'c' => {
        println!("continuing...");
        self.suspended = false;
      }
      b'm' => {
        // TODO: Only mute current suspended address, not everything.
        println!("continuing...");
        self.suspended = false;
        self.breakpoints.clear()
      }
      b's' => {
        self.dump_stack(cpu);
        self.user_input(cpu);
      }
      _ => {
        println!("Unknown debugger command: {}", ch);
        self.user_input(cpu);
      }
    }
  }

  fn dump_stack(&self, cpu: &Cpu<B>) {
    for a in Cpu::<B>::STACK_TOP..=Cpu::<B>::STACK_BOTTOM {
      print!("{:#06x}: {:#04x}", a, cpu.bus.read8(a as u16));
      if a as u8 == cpu.regs[SP] {
        print!(" <----");
      }
      println!();
    }
  }

  fn print_instruction(bus: &B, pc: u16, opbyte: u8, inst: &Instruction) {
    let mut pc_str = String::new();
    write!(&mut pc_str, "{:#06x}", pc).unwrap();

    let mut opbyte_str = String::new();
    write!(&mut opbyte_str, "{:#04x}", opbyte).unwrap();

    let mut operands_str = String::new();
    for o in 1..=inst.size - 1 {
      let operand = bus.read8(pc + o as u16);
      write!(&mut operands_str, "{:#04x} ", operand).unwrap();
    }

    let mut mnemonic_str = String::new();
    write!(
      &mut mnemonic_str,
      "{:?} {:?} {}",
      inst.opcode, inst.mode, operands_str
    )
    .unwrap();

    println!(
      "{:<10} {} {:<10} {}",
      pc_str, opbyte_str, operands_str, mnemonic_str
    );
  }
}

impl<'cpu, B: Bus> AttachedDebugger<'cpu, B> {
  pub fn dump_backtrace(&mut self) {
    self.debugger.dump_backtrace(self.cpu);
  }

  pub fn add_breakpoint(&mut self, breakpoint: Breakpoint) {
    self.debugger.add_breakpoint(breakpoint);
  }

  pub fn watch_memory_range(&mut self, range: RangeInclusive<u16>, f: impl Fn(Vec<u8>) + 'static) {
    self.debugger.watch_memory_range(range, f);
  }

  pub fn watch_memory(&mut self, address: u16, f: impl Fn(u8) + 'static) {
    self.debugger.watch_memory(address, f);
  }

  pub fn dump_stack(&self) {
    self.debugger.dump_stack(self.cpu);
  }

  pub fn verbose(&mut self, verbose: bool) {
    self.debugger.verbose = verbose;
  }

  pub fn suspend(&mut self) {
    self.debugger.suspend(self.cpu, self.cpu.pc);
  }

  pub fn dump_opcodes(&self) {
    let mut sorted: Vec<_> = self.debugger.opcodes.iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(a.1));

    for (opcode, count) in sorted.into_iter().take(10) {
      println!("Opcode {:?}: {} times", opcode, count);
    }
  }
}

impl<B: Bus> std::fmt::Debug for Cpu<B> {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    fn hexdec(val: u8) -> String {
      format!("{:#04x} ({})", val, val)
    }

    writeln!(f, "--------")?;
    write!(
      f,
      "pc:\t{:#06x}\nsp:\t{}\nacc:\t{}\nx:\t{}\ny:\t{}\n",
      self.pc,
      hexdec(self.regs[SP]),
      hexdec(self.regs[AC]),
      hexdec(self.regs[X]),
      hexdec(self.regs[Y])
    )?;
    write!(
      f,
      "NEG={}, OVF={}, DEC={}, INT={}, ZER={}, CAR={} ({:#010b}) ({:#04x})",
      self.flags.contains(Flag::N),
      self.flags.contains(Flag::V),
      self.flags.contains(Flag::D),
      self.flags.contains(Flag::I),
      self.flags.contains(Flag::Z),
      self.flags.contains(Flag::C),
      self.flags.bits(),
      self.flags.bits()
    )
  }
}

impl<B: Bus> std::fmt::Display for Cpu<B> {
  // nestest format
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(
      f,
      "A:{:02X} X:{:02X} Y:{:02X} P:{:02X} SP:{:02X}",
      self.regs[AC],
      self.regs[X],
      self.regs[Y],
      self.flags.bits(),
      self.regs[SP]
    )
  }
}

impl std::fmt::Display for Opcode {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{:?}", self)
  }
}
