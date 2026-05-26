use std::fmt::Write;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;

use mos6502::cpu::Flag;
use mos6502::cpu::SP;
use nes::cartridge::Cartridge;
use nes::nes::HeadlessHost;
use nes::nes::Nes;

// cat nes/roms/nestest.log | awk '{print substr(,49)}' > nes/roms/nestest_cycles.log
// cat nes/roms/nestest.log | awk '{printf "%s ",substr(,0,4); print substr(,49)}; ' > nes/roms/nestest_cycles.log

// http://www.qmtpro.com/~nes/misc/nestest.txt
// This test program, when run on "automation", (i.e. set your program counter
// to 0c000h) will perform all tests in sequence and shove the results of
// the tests into locations 02h and 03h.
const NESTEST_ENTRY_POINT: u16 = 0xc000;
const NESTEST_SUCCESS: u16 = 0xc68b; // Here it starts writing to APU, which is not yet implemented.
const NESTEST_RES_BYTE2: u16 = 0x0002;
const NESTEST_RES_BYTE3: u16 = 0x0003;

const ENABLE_TEST_CYCLES: bool = false;

#[test]
fn nestest() {
  let cartridge =
    Cartridge::blow_dust("../test-roms/nestest/nestest.nes".into()).expect("failed to map rom");
  let mut nes = Nes::insert(cartridge, HeadlessHost);

  let logf =
    File::open("../test-roms/nestest/nestest_cycles.log").expect("failed to read test log");
  let log: Vec<String> = BufReader::new(logf).lines().map(|s| s.unwrap()).collect();

  // nes.machine().debugger().enable();

  // nestest startup state
  // reset vector points to 0xc004 - but that's for graphic mode, we want automation at 0xc000
  nes.cpu_mut().set_pc(NESTEST_ENTRY_POINT);
  // nestest startups with these flags... Maybe the CPU should as well? or only for this weird test?
  nes.cpu_mut().flags.remove(Flag::B);
  nes.cpu_mut().flags.insert(Flag::UNUSED);
  nes.cpu_mut().flags.insert(Flag::I);
  nes.cpu_mut().regs[SP] = 0xfd;

  nes
    .debugger()
    .watch_memory_range(NESTEST_RES_BYTE2..=NESTEST_RES_BYTE3, |result| {
      assert_eq!(
        result[0], 0x00,
        "nestest reports error code on byte 2.. check README"
      );
      assert_eq!(
        result[1], 0x00,
        "nestest reports error code on byte 3.. check README"
      );
    });

  let mut i = 0;
  loop {
    let mut sts = String::new();
    write!(&mut sts, "{:?}", nes).unwrap();

    nes.tick();
    if log[i] != sts && ENABLE_TEST_CYCLES {
      // nes.dump_backtrace();
      panic!(
        "nestest cycle test mismatch!\n\nExpected:\t{}\nActual:\t\t{}\n",
        log[i], sts
      );
    }

    i += 1;

    if nes.cpu().pc == NESTEST_SUCCESS {
      break;
    }
  }

  let expected_ticks = 26513;
  assert_eq!(expected_ticks, nes.cpu_cycles(), "wrong tick count");
}
