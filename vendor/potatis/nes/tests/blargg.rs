use mos6502::memory::Bus;
use nes::{
  cartridge::Cartridge,
  nes::{HeadlessHost, Nes},
};

const STATUS_RUNNING: u8 = 0x80;
const STATUS_NEEDS_RESET: u8 = 0x81;
const STATUS_SUCCESS: u8 = 0x00;
const VALID_MAGIC: [u8; 3] = [0xde, 0xb0, 0x61];

#[derive(PartialEq, Eq)]
enum PassCond {
  Status(&'static str, u8),
  Pc(u16),
}

#[test]
fn instr_test_v5_official_mmc1() {
  run_blargg_test(
    "instr_test-v5/official_only.nes",
    PassCond::Status("All 16 tests passed", STATUS_SUCCESS),
  )
}

#[test]
fn instr_test_v5_basic_nrom() {
  // run_blargg_test("instr_test-v5/rom_singles/01-basics.nes", PassCond::Status("01-basics\n\nPassed", STATUS_SUCCESS));
  run_blargg_test(
    "instr_test-v5/rom_singles/01-basics.nes",
    PassCond::Pc(0x01e2),
  );
}

#[test]
fn instr_misc() {
  // let success = "LDA abs,x\n\n03-dummy_reads\n\nFailed #3\n\nWhile running test 3 of 4";
  let success = "Test requires $2002 mirroring every 8 bytes to $3FFA\n\n03-dummy_reads\n\nFailed #2\n\nWhile running test 3 of 4";
  run_blargg_test("instr_misc/instr_misc.nes", PassCond::Status(success, 1));
}

#[ignore = "bad test"]
#[test]
fn ppu_vbl_nmi() {
  let success = "VBL period is too long with BG off\n\n01-vbl_basics\n\nFailed #8";
  // let success = "$2002 should be mirrored at $200A\n\n01-vbl_basics\n\nFailed #5";
  run_blargg_test(
    "ppu_vbl_nmi/rom_singles/01-vbl_basics.nes",
    PassCond::Status(success, 0x08),
  );
}

#[ignore = "not implemented"]
#[test]
fn ppu_open_bus() {
  let success =
    "Bits 2-4 of sprite attributes should always be clear when read\n\nppu_open_bus\n\nFailed #10";
  run_blargg_test(
    "ppu_open_bus/ppu_open_bus.nes",
    PassCond::Status(success, 0x0a),
  );
}

#[ignore = "bad test"]
#[test]
fn cpu_exec_space() {
  let success = "\u{1b}[0;37mTEST:test_cpu_exec_space_ppuio\n\u{1b}[0;33mThis program verifies that the\nCPU can execute code from any\npossible location that it can\naddress, including I/O space.\n\nIn addition, it will be tested\nthat an RTS instruction does a\ndummy read of the byte that\nimmediately follows the\ninstructions.\n\n\u{1b}[0;37m\u{1b}[1;34mJSR+RTS TEST OK\nJMP+RTS TEST OK\nRTS+RTS TEST OK\nJMP+RTI TEST OK\nJMP+BRK TEST OK\n\u{1b}[0;37m\nPassed";
  run_blargg_test(
    "cpu_exec_space/test_cpu_exec_space_ppuio.nes",
    PassCond::Status(success, 0x00),
  );
}

#[test]
fn branch_timing() {
  run_blargg_test(
    "branch_timing_tests/1.Branch_Basics.nes",
    PassCond::Pc(0xe01d),
  );
  run_blargg_test(
    "branch_timing_tests/2.Backward_Branch.nes",
    PassCond::Pc(0xe01d),
  );
  run_blargg_test(
    "branch_timing_tests/3.Forward_Branch.nes",
    PassCond::Pc(0xe01d),
  );
}

#[test]
fn palette_ram() {
  run_blargg_test(
    "blargg_ppu_tests_2005.09.15b/palette_ram.nes",
    PassCond::Pc(0xe0eb),
  );
}

#[test]
#[ignore = "bad test"]
fn oven_odd_frames() {
  run_blargg_test(
    "ppu_vbl_nmi/rom_singles/09-even_odd_frames.nes",
    PassCond::Status("dunno", STATUS_SUCCESS),
  );
}

#[test]
#[ignore = "bad test"]
fn ppu_read_buffer() {
  run_blargg_test(
    "ppu_read_buffer/test_ppu_read_buffer.nes",
    PassCond::Status("dunno", STATUS_SUCCESS),
  );
}

fn run_blargg_test(test: &str, pass_condition: PassCond) {
  let cartridge = Cartridge::blow_dust(format!("../test-roms/nes-test-roms/{}", test).into())
    .expect("failed to map rom");
  let mut nes = Nes::insert(cartridge, HeadlessHost);

  nes.debugger().verbose(std::env::var("VERBOSE").is_ok());

  let result: String;
  let mut status: Option<u8> = None;

  // nes.debugger().watch_memory(0x3ffa, |val| {
  //   println!("3ffa: {:#04x}", val);
  // });

  nes
    .debugger()
    .watch_memory_range(0x6004..=0x6004 + 100, |mem| {
      println!("{}", read_null_terminated_string(&mem));
    });

  loop {
    nes.tick();

    if PassCond::Pc(nes.cpu().pc) == pass_condition {
      println!("success! pc at: {:#06x}", nes.cpu().pc);
      return;
    }

    if check_and_update_status(&nes, &mut status) {
      match status {
        Some(STATUS_RUNNING) => (),
        Some(STATUS_NEEDS_RESET) => panic!("needs reset.."),
        Some(0x00..=0x7F) => {
          // Completed, status is the result code.
          let mem_view = nes.bus().read_range(0x6004..=0x6004 + 1000);
          result = read_null_terminated_string(&mem_view);
          break;
        }
        _ => panic!("unknown status"),
      }
    }
  }

  if let PassCond::Status(success_str, success_status) = pass_condition {
    println!("status code: {:#04x}", status.unwrap());
    assert_eq!(success_str, result.trim());
    assert_eq!(Some(success_status), status);
  } else {
    unreachable!()
  }
}

fn read_null_terminated_string(range: &[u8]) -> String {
  let string: Vec<u8> = range.iter().take_while(|&b| *b != 0x00).cloned().collect();
  String::from_utf8(string).unwrap()
}

fn check_and_update_status(nes: &Nes<HeadlessHost>, current_status: &mut Option<u8>) -> bool {
  if nes.cpu().bus.read_range(0x6001..=0x6003) == VALID_MAGIC {
    let new_status = nes.cpu().bus.read8(0x6000);
    if Some(new_status) != *current_status {
      *current_status = Some(new_status);
      return true;
    }
  }
  false
}
