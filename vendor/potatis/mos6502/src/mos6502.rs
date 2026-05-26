use crate::cpu::Cpu;
#[cfg(feature = "debugger")]
use crate::debugger::AttachedDebugger;
#[cfg(feature = "debugger")]
use crate::debugger::Debugger;
use crate::memory::Bus;

pub struct Mos6502<B> {
  pub cpu: Cpu<B>,
  pub total_cycles: usize,
  #[cfg(feature = "debugger")]
  debugger: Debugger<B>,
}

impl<B: Bus> Mos6502<B> {
  pub fn new(cpu: Cpu<B>) -> Self {
    #[cfg(feature = "debugger")]
    {
      let debugger = Debugger::new();
      Self {
        cpu,
        total_cycles: 0,
        debugger,
      }
    }

    #[cfg(not(feature = "debugger"))]
    {
      Self {
        cpu,
        total_cycles: 0,
      }
    }
  }

  #[cfg(feature = "debugger")]
  pub fn debugger(&mut self) -> AttachedDebugger<B> {
    self.debugger.attach(&mut self.cpu)
  }

  // The clock ticks Hzhzhzhz
  pub fn tick(&mut self) -> usize {
    let (inst, operands) = self.cpu.fetch_next_instruction();

    #[cfg(feature = "debugger")]
    self.debugger.on_tick(&self.cpu, inst);

    let cycles = self.cpu.execute(inst, operands);

    self.total_cycles += cycles;
    cycles
  }
}
