use mos6502::cpu::Cpu;
use mos6502::memory::Memory;
use mos6502::mos6502::Mos6502;

fn run_test_rom(
  file: &str,
  load_base: u16,
  entry_point: u16,
  success_address: u16,
) -> (bool, usize) {
  let path = format!("../test-roms/bin/{}", file);
  let program = std::fs::read(path).expect("failed to load test rom");

  let mem = Memory::load(&program[..], load_base);
  let mut cpu = Cpu::new(mem);
  cpu.set_pc(entry_point);

  // debugger.add_breakpoint(Breakpoint::Opcode("DEX".into()));
  let mut machine = Mos6502::new(cpu);

  let mut last_pc: Option<u16> = None;
  let mut ticks = 0usize;

  loop {
    ticks += 1;
    machine.tick();

    let pc = machine.cpu.pc;

    // Panic if looping on PC, most likely functional_tests trap.
    if Some(pc) == last_pc {
      machine.debugger().dump_backtrace();
      return (false, ticks);
    }

    // JMP start == catastrophic error
    if last_pc.is_some() && pc == entry_point {
      machine.debugger().dump_backtrace();
      return (false, ticks);
    }

    if pc == success_address {
      // println!("✅ TEST SUCCESSFUL! Hit success address at {:#06x}", pc);
      // machine.debugger().dump_fun_stats();
      return (true, ticks);
    }

    last_pc = Some(pc);
  }
}

#[test]
fn functional_test_bcd_disabled() {
  let expected_ticks = 26765879;
  let res = run_test_rom("functional_test_bcd_disabled.bin", 0x000, 0x400, 0x336d);
  assert!(res.0, "trapped");
  assert_eq!(expected_ticks, res.1, "wrong tick count");
}

#[test]
fn ttl6502() {
  let expected_ticks = 2738;
  let res = run_test_rom("TTL6502.bin", 0xe000, 0xe000, 0xf5b6);
  assert!(res.0, "trapped");
  assert_eq!(expected_ticks, res.1, "wrong tick count");
}

#[test]
#[ignore = "BCD is not implemented yet"]
fn functional_test_full() {
  let expected_ticks = 0;
  let res = run_test_rom("functional_test_full.bin", 0x000, 0x400, 0x3469);
  assert!(res.0, "trapped");
  assert_eq!(expected_ticks, res.1, "wrong tick count");
}

#[test]
#[ignore = "BCD is not implemented yet"]
fn functional_test_extended_opcodes() {
  let expected_ticks = 26765879;
  let res = run_test_rom("extended_test.bin", 0x000, 0x400, 0x336d);
  assert!(res.0, "trapped");
  assert_eq!(expected_ticks, res.1, "wrong tick count");
}
