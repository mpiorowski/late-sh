use core::panic;

use crate::cpu::{Cpu, X, Y};
use crate::memory::Bus;

pub type Operands = (u8, u8);

#[derive(Debug, PartialEq, Eq)]
pub enum AddressMode {
  Abs,
  AbsX,
  AbsY,
  Imm,
  Impl,
  Ind,
  IndX,
  IndY,
  Rel,
  Zero,
  ZeroX,
  ZeroY,
  Nop, // Not official.. used for dev
}

#[derive(Debug, PartialEq, Eq, Hash)]
#[allow(clippy::upper_case_acronyms)]
pub enum Opcode {
  ADC, // Add Memory to Accumulator with Carry
  SBC, // Subtract Memory from Accumulator with Borrow
  CLD, // Clear decimal mode
  CLC, // Clear carry
  CLI, // Clear interrupt
  CLV, // Clear overflow
  EOR, // Exclusive-OR Memory with Accumulator
  AND, // AND Memory with Accumulator
  ORA, // OR Memory with Accumulator
  LDX,
  LDA,
  LDY,
  TAX, // Transfer Accumulator to Index X
  TAY, // Transfer Accumulator to Index Y
  TSX, // Transfer Stack Pointer to Index X
  TXA, // Transfer Index X to Accumulato
  TXS,
  TYA, // Transfer Index Y to Accumulator
  STA, // Store Accumulator in Memory
  STX, // Store Index X in Memory
  STY, // Store Index Y in Memory
  JMP,
  JSR, // Jump to New Location Saving Return Address
  RTS, // Return from Subroutine
  BNE, // Branch on Result not Zero
  BEQ, // Branch on Result Zero
  BPL, // Branch on Result Plus
  DEX, // Decrement Index X by One
  DEY, // Decrement Index Y by One
  DEC, // Decrement Memory by One
  INC, // Increment Memory by One
  INX,
  INY,
  BIT, // Test Bits in Memory with Accumulator
  BCC, // Branch on Carry Clear
  BCS, // Branch on Carry Set
  BMI, // Branch on Result Minus
  BVC, // Branch on Overflow Clear
  BVS, // Branch on Overflow Set
  SEC, // Set Carry Flag
  SED, // Set Decimal Flag
  SEI, // Set Interrupt Disable Status
  NOP,
  CPY, // Compare Memory and Index Y
  CPX, // Compare Memory and Index X
  LSR, // Shift One Bit Right (Memory or Accumulator)
  ASL, // Shift Left One Bit (Memory or Accumulator)
  ROL, // Rotate One Bit Left (Memory or Accumulator)
  ROR, // Rotate One Bit Right (Memory or Accumulator)
  CMP, // Compare Memory with Accumulator
  PHA, // Push Accumulator on Stack
  PLA, // Pull Accumulator from Stack,
  PLP, // Pull Processor Status from Stack
  PHP, // Push Processor Status on Stack,
  JAM, // Halt, kill. Illegal, only used for own tests so far.
  BRK, // Force Break, Software interrupt
  RTI, // Return from Interrupt

  // Illegal opcodes
  LAX,  // LDA oper + LDX oper
  SAX,  // A AND X -> M
  USBC, // effectively same as normal SBC immediate, instr. E9.
  DCP,  // DEC oper + CMP oper
  ISC,  // INC oper + SBC oper
  SLO,  // ASL oper + ORA oper
  RLA,  // ROL oper + AND oper
  SRE,  // LSR oper + EOR oper
  RRA,  // ROR oper + ADC oper
  PHX,  // push x
  PHY,  // push y
  PLX,  // pull x
  PLY,  // pull y
  ANC,  // AND oper + set C as ASL
  ANC2, // effectively the same as instr. 0B (ANC)
  ALR,  // AND oper + LSR
}

#[derive(Debug, PartialEq, Eq)]
pub struct Instruction {
  pub opcode: Opcode,
  pub mode: AddressMode,
  pub cycles: usize,
  pub size: u8,
}

const UNINIT: Instruction = Instruction::imp(Opcode::JAM, 0);
const JAM: Instruction = Instruction::imp(Opcode::JAM, 0);
const NOP: Instruction = Instruction::imp(Opcode::NOP, 2);
const NOP_2_2: Instruction = Instruction::two(Opcode::NOP, 2, AddressMode::Nop);
const NOP_2_3: Instruction = Instruction::two(Opcode::NOP, 3, AddressMode::Nop);
const NOP_2_4: Instruction = Instruction::two(Opcode::NOP, 4, AddressMode::Nop);
const NOP_3_4: Instruction = Instruction::thr(Opcode::NOP, 4, AddressMode::Nop);
const NOP_3_A: Instruction = Instruction::thr(Opcode::NOP, 4, AddressMode::AbsX);

static INSTRUCTIONS: [Instruction; 256] = {
  let mut i = [UNINIT; 256];

  i[0x02] = JAM;
  i[0x12] = JAM;
  i[0x22] = JAM;
  i[0x32] = JAM;
  i[0x42] = JAM;
  i[0x52] = JAM;
  i[0x62] = JAM;
  i[0x72] = JAM;
  i[0x92] = JAM;
  i[0xB2] = JAM;
  i[0xD2] = JAM;
  i[0xF2] = JAM;
  i[0x00] = Instruction::imp(Opcode::BRK, 7);
  i[0x40] = Instruction::imp(Opcode::RTI, 6);

  i[0x38] = Instruction::imp(Opcode::SEC, 2);
  i[0xf8] = Instruction::imp(Opcode::SED, 2);
  i[0x78] = Instruction::imp(Opcode::SEI, 2);

  i[0x18] = Instruction::imp(Opcode::CLC, 2);
  i[0xd8] = Instruction::imp(Opcode::CLD, 2);
  i[0x58] = Instruction::imp(Opcode::CLI, 2);
  i[0xb8] = Instruction::imp(Opcode::CLV, 2);
  i[0xaa] = Instruction::imp(Opcode::TAX, 2);
  i[0xa8] = Instruction::imp(Opcode::TAY, 2);
  i[0xba] = Instruction::imp(Opcode::TSX, 2);
  i[0x8a] = Instruction::imp(Opcode::TXA, 2);
  i[0x9a] = Instruction::imp(Opcode::TXS, 2);
  i[0x98] = Instruction::imp(Opcode::TYA, 2);

  i[0x24] = Instruction::two(Opcode::BIT, 3, AddressMode::Zero);
  i[0x2c] = Instruction::thr(Opcode::BIT, 4, AddressMode::Abs);

  i[0x69] = Instruction::two(Opcode::ADC, 2, AddressMode::Imm);
  i[0x65] = Instruction::two(Opcode::ADC, 3, AddressMode::Zero);
  i[0x75] = Instruction::two(Opcode::ADC, 4, AddressMode::ZeroX);
  i[0x6d] = Instruction::thr(Opcode::ADC, 4, AddressMode::Abs);
  i[0x7d] = Instruction::thr(Opcode::ADC, 4, AddressMode::AbsX);
  i[0x79] = Instruction::thr(Opcode::ADC, 4, AddressMode::AbsY);
  i[0x61] = Instruction::two(Opcode::ADC, 6, AddressMode::IndX);
  i[0x71] = Instruction::two(Opcode::ADC, 5, AddressMode::IndY);

  i[0xe9] = Instruction::two(Opcode::SBC, 2, AddressMode::Imm);
  i[0xe5] = Instruction::two(Opcode::SBC, 3, AddressMode::Zero);
  i[0xf5] = Instruction::two(Opcode::SBC, 4, AddressMode::ZeroX);
  i[0xed] = Instruction::thr(Opcode::SBC, 4, AddressMode::Abs);
  i[0xfd] = Instruction::thr(Opcode::SBC, 4, AddressMode::AbsX);
  i[0xf9] = Instruction::thr(Opcode::SBC, 4, AddressMode::AbsY);
  i[0xe1] = Instruction::two(Opcode::SBC, 6, AddressMode::IndX);
  i[0xf1] = Instruction::two(Opcode::SBC, 5, AddressMode::IndY);

  i[0xa2] = Instruction::two(Opcode::LDX, 2, AddressMode::Imm);
  i[0xa6] = Instruction::two(Opcode::LDX, 3, AddressMode::Zero);
  i[0xb6] = Instruction::two(Opcode::LDX, 4, AddressMode::ZeroY);
  i[0xae] = Instruction::thr(Opcode::LDX, 4, AddressMode::Abs);
  i[0xbe] = Instruction::thr(Opcode::LDX, 4, AddressMode::AbsY);

  i[0x49] = Instruction::two(Opcode::EOR, 2, AddressMode::Imm);
  i[0x45] = Instruction::two(Opcode::EOR, 3, AddressMode::Zero);
  i[0x55] = Instruction::two(Opcode::EOR, 4, AddressMode::ZeroX);
  i[0x4d] = Instruction::thr(Opcode::EOR, 4, AddressMode::Abs);
  i[0x5d] = Instruction::thr(Opcode::EOR, 4, AddressMode::AbsX);
  i[0x59] = Instruction::thr(Opcode::EOR, 4, AddressMode::AbsY);
  i[0x41] = Instruction::two(Opcode::EOR, 6, AddressMode::IndX);
  i[0x51] = Instruction::two(Opcode::EOR, 5, AddressMode::IndY);

  i[0x09] = Instruction::two(Opcode::ORA, 2, AddressMode::Imm);
  i[0x05] = Instruction::two(Opcode::ORA, 3, AddressMode::Zero);
  i[0x15] = Instruction::two(Opcode::ORA, 4, AddressMode::ZeroX);
  i[0x0d] = Instruction::thr(Opcode::ORA, 4, AddressMode::Abs);
  i[0x1d] = Instruction::thr(Opcode::ORA, 4, AddressMode::AbsX);
  i[0x19] = Instruction::thr(Opcode::ORA, 4, AddressMode::AbsY);
  i[0x01] = Instruction::two(Opcode::ORA, 6, AddressMode::IndX);
  i[0x11] = Instruction::two(Opcode::ORA, 5, AddressMode::IndY);

  i[0x29] = Instruction::two(Opcode::AND, 2, AddressMode::Imm);
  i[0x25] = Instruction::two(Opcode::AND, 3, AddressMode::Zero);
  i[0x35] = Instruction::two(Opcode::AND, 4, AddressMode::ZeroX);
  i[0x2d] = Instruction::thr(Opcode::AND, 4, AddressMode::Abs);
  i[0x3d] = Instruction::thr(Opcode::AND, 4, AddressMode::AbsX);
  i[0x39] = Instruction::thr(Opcode::AND, 4, AddressMode::AbsY);
  i[0x21] = Instruction::two(Opcode::AND, 6, AddressMode::IndX);
  i[0x31] = Instruction::two(Opcode::AND, 5, AddressMode::IndY);

  i[0xa0] = Instruction::two(Opcode::LDY, 2, AddressMode::Imm);
  i[0xa4] = Instruction::two(Opcode::LDY, 3, AddressMode::Zero);
  i[0xb4] = Instruction::two(Opcode::LDY, 4, AddressMode::ZeroX);
  i[0xac] = Instruction::thr(Opcode::LDY, 4, AddressMode::Abs);
  i[0xbc] = Instruction::thr(Opcode::LDY, 4, AddressMode::AbsX);

  i[0xa9] = Instruction::two(Opcode::LDA, 2, AddressMode::Imm);
  i[0xa5] = Instruction::two(Opcode::LDA, 3, AddressMode::Zero);
  i[0xb5] = Instruction::two(Opcode::LDA, 4, AddressMode::ZeroX);
  i[0xad] = Instruction::thr(Opcode::LDA, 4, AddressMode::Abs);
  i[0xbd] = Instruction::thr(Opcode::LDA, 4, AddressMode::AbsX);
  i[0xb9] = Instruction::thr(Opcode::LDA, 4, AddressMode::AbsY);
  i[0xa1] = Instruction::two(Opcode::LDA, 6, AddressMode::IndX);
  i[0xb1] = Instruction::two(Opcode::LDA, 5, AddressMode::IndY);

  i[0x85] = Instruction::two(Opcode::STA, 3, AddressMode::Zero);
  i[0x95] = Instruction::two(Opcode::STA, 4, AddressMode::ZeroX);
  i[0x8d] = Instruction::thr(Opcode::STA, 4, AddressMode::Abs);
  i[0x9d] = Instruction::thr(Opcode::STA, 5, AddressMode::AbsX);
  i[0x99] = Instruction::thr(Opcode::STA, 5, AddressMode::AbsY);
  i[0x81] = Instruction::two(Opcode::STA, 6, AddressMode::IndX);
  i[0x91] = Instruction::two(Opcode::STA, 6, AddressMode::IndY);

  i[0x86] = Instruction::two(Opcode::STX, 3, AddressMode::Zero);
  i[0x96] = Instruction::two(Opcode::STX, 4, AddressMode::ZeroY);
  i[0x8e] = Instruction::thr(Opcode::STX, 4, AddressMode::Abs);

  i[0x84] = Instruction::two(Opcode::STY, 3, AddressMode::Zero);
  i[0x94] = Instruction::two(Opcode::STY, 4, AddressMode::ZeroX);
  i[0x8c] = Instruction::thr(Opcode::STY, 4, AddressMode::Abs);

  i[0x4c] = Instruction::thr(Opcode::JMP, 3, AddressMode::Abs);
  i[0x6c] = Instruction::thr(Opcode::JMP, 5, AddressMode::Ind);
  i[0x20] = Instruction::thr(Opcode::JSR, 6, AddressMode::Abs);
  i[0x60] = Instruction::imp(Opcode::RTS, 6);

  i[0xd0] = Instruction::two(Opcode::BNE, 2, AddressMode::Rel);
  i[0xf0] = Instruction::two(Opcode::BEQ, 2, AddressMode::Rel);
  i[0x10] = Instruction::two(Opcode::BPL, 2, AddressMode::Rel);
  i[0x90] = Instruction::two(Opcode::BCC, 2, AddressMode::Rel);
  i[0xb0] = Instruction::two(Opcode::BCS, 2, AddressMode::Rel);
  i[0x30] = Instruction::two(Opcode::BMI, 2, AddressMode::Rel);
  i[0x50] = Instruction::two(Opcode::BVC, 2, AddressMode::Rel);
  i[0x70] = Instruction::two(Opcode::BVS, 2, AddressMode::Rel);

  i[0xca] = Instruction::imp(Opcode::DEX, 2);
  i[0x88] = Instruction::imp(Opcode::DEY, 2);
  i[0xe8] = Instruction::imp(Opcode::INX, 2);
  i[0xc8] = Instruction::imp(Opcode::INY, 2);

  i[0xc6] = Instruction::two(Opcode::DEC, 5, AddressMode::Zero);
  i[0xd6] = Instruction::two(Opcode::DEC, 6, AddressMode::ZeroX);
  i[0xce] = Instruction::thr(Opcode::DEC, 6, AddressMode::Abs);
  i[0xde] = Instruction::thr(Opcode::DEC, 7, AddressMode::AbsX);

  i[0xe6] = Instruction::two(Opcode::INC, 5, AddressMode::Zero);
  i[0xf6] = Instruction::two(Opcode::INC, 6, AddressMode::ZeroX);
  i[0xee] = Instruction::thr(Opcode::INC, 6, AddressMode::Abs);
  i[0xfe] = Instruction::thr(Opcode::INC, 7, AddressMode::AbsX);

  i[0xc0] = Instruction::two(Opcode::CPY, 2, AddressMode::Imm);
  i[0xc4] = Instruction::two(Opcode::CPY, 3, AddressMode::Zero);
  i[0xcc] = Instruction::thr(Opcode::CPY, 4, AddressMode::Abs);
  i[0xe0] = Instruction::two(Opcode::CPX, 2, AddressMode::Imm);
  i[0xe4] = Instruction::two(Opcode::CPX, 3, AddressMode::Zero);
  i[0xec] = Instruction::thr(Opcode::CPX, 4, AddressMode::Abs);

  i[0xc9] = Instruction::two(Opcode::CMP, 2, AddressMode::Imm);
  i[0xc5] = Instruction::two(Opcode::CMP, 3, AddressMode::Zero);
  i[0xd5] = Instruction::two(Opcode::CMP, 4, AddressMode::ZeroX);
  i[0xcd] = Instruction::thr(Opcode::CMP, 4, AddressMode::Abs);
  i[0xdd] = Instruction::thr(Opcode::CMP, 4, AddressMode::AbsX);
  i[0xd9] = Instruction::thr(Opcode::CMP, 4, AddressMode::AbsY);
  i[0xc1] = Instruction::two(Opcode::CMP, 6, AddressMode::IndX);
  i[0xd1] = Instruction::two(Opcode::CMP, 5, AddressMode::IndY);

  i[0x4a] = Instruction::imp(Opcode::LSR, 2);
  i[0x46] = Instruction::two(Opcode::LSR, 5, AddressMode::Zero);
  i[0x56] = Instruction::two(Opcode::LSR, 6, AddressMode::ZeroX);
  i[0x4e] = Instruction::thr(Opcode::LSR, 6, AddressMode::Abs);
  i[0x5e] = Instruction::thr(Opcode::LSR, 7, AddressMode::AbsX);

  i[0x0a] = Instruction::imp(Opcode::ASL, 2);
  i[0x06] = Instruction::two(Opcode::ASL, 5, AddressMode::Zero);
  i[0x16] = Instruction::two(Opcode::ASL, 6, AddressMode::ZeroX);
  i[0x0e] = Instruction::thr(Opcode::ASL, 6, AddressMode::Abs);
  i[0x1e] = Instruction::thr(Opcode::ASL, 7, AddressMode::AbsX);

  i[0x2a] = Instruction::imp(Opcode::ROL, 2);
  i[0x26] = Instruction::two(Opcode::ROL, 5, AddressMode::Zero);
  i[0x36] = Instruction::two(Opcode::ROL, 6, AddressMode::ZeroX);
  i[0x2e] = Instruction::thr(Opcode::ROL, 6, AddressMode::Abs);
  i[0x3e] = Instruction::thr(Opcode::ROL, 7, AddressMode::AbsX);

  i[0x6a] = Instruction::imp(Opcode::ROR, 2);
  i[0x66] = Instruction::two(Opcode::ROR, 5, AddressMode::Zero);
  i[0x76] = Instruction::two(Opcode::ROR, 6, AddressMode::ZeroX);
  i[0x6e] = Instruction::thr(Opcode::ROR, 6, AddressMode::Abs);
  i[0x7e] = Instruction::thr(Opcode::ROR, 7, AddressMode::AbsX);

  i[0x48] = Instruction::imp(Opcode::PHA, 3);
  i[0x68] = Instruction::imp(Opcode::PLA, 4);
  i[0x08] = Instruction::imp(Opcode::PHP, 3);
  i[0x28] = Instruction::imp(Opcode::PLP, 4);

  i[0xa7] = Instruction::two(Opcode::LAX, 3, AddressMode::Zero);
  i[0xb7] = Instruction::two(Opcode::LAX, 4, AddressMode::ZeroY);
  i[0xaf] = Instruction::thr(Opcode::LAX, 4, AddressMode::Abs);
  i[0xbf] = Instruction::thr(Opcode::LAX, 4, AddressMode::AbsY);
  i[0xa3] = Instruction::two(Opcode::LAX, 6, AddressMode::IndX);
  i[0xb3] = Instruction::two(Opcode::LAX, 5, AddressMode::IndY);

  i[0x87] = Instruction::two(Opcode::SAX, 3, AddressMode::Zero);
  i[0x97] = Instruction::two(Opcode::SAX, 4, AddressMode::ZeroY);
  i[0x8f] = Instruction::thr(Opcode::SAX, 4, AddressMode::Abs);
  i[0x83] = Instruction::two(Opcode::SAX, 6, AddressMode::IndX);

  i[0xeb] = Instruction::two(Opcode::USBC, 2, AddressMode::Imm);

  i[0xc7] = Instruction::two(Opcode::DCP, 5, AddressMode::Zero);
  i[0xd7] = Instruction::two(Opcode::DCP, 6, AddressMode::ZeroX);
  i[0xcf] = Instruction::thr(Opcode::DCP, 6, AddressMode::Abs);
  i[0xdf] = Instruction::thr(Opcode::DCP, 7, AddressMode::AbsX);
  i[0xdb] = Instruction::thr(Opcode::DCP, 7, AddressMode::AbsY);
  i[0xc3] = Instruction::two(Opcode::DCP, 8, AddressMode::IndX);
  i[0xd3] = Instruction::two(Opcode::DCP, 8, AddressMode::IndY);

  i[0xe7] = Instruction::two(Opcode::ISC, 5, AddressMode::Zero);
  i[0xf7] = Instruction::two(Opcode::ISC, 6, AddressMode::ZeroX);
  i[0xef] = Instruction::thr(Opcode::ISC, 6, AddressMode::Abs);
  i[0xff] = Instruction::thr(Opcode::ISC, 7, AddressMode::AbsX);
  i[0xfb] = Instruction::thr(Opcode::ISC, 7, AddressMode::AbsY);
  i[0xe3] = Instruction::two(Opcode::ISC, 8, AddressMode::IndX);
  i[0xf3] = Instruction::two(Opcode::ISC, 4, AddressMode::IndY);

  i[0x07] = Instruction::two(Opcode::SLO, 5, AddressMode::Zero);
  i[0x17] = Instruction::two(Opcode::SLO, 6, AddressMode::ZeroX);
  i[0x0f] = Instruction::thr(Opcode::SLO, 6, AddressMode::Abs);
  i[0x1f] = Instruction::thr(Opcode::SLO, 7, AddressMode::AbsX);
  i[0x1b] = Instruction::thr(Opcode::SLO, 7, AddressMode::AbsY);
  i[0x03] = Instruction::two(Opcode::SLO, 8, AddressMode::IndX);
  i[0x13] = Instruction::two(Opcode::SLO, 8, AddressMode::IndY);

  i[0x27] = Instruction::two(Opcode::RLA, 5, AddressMode::Zero);
  i[0x37] = Instruction::two(Opcode::RLA, 6, AddressMode::ZeroX);
  i[0x2f] = Instruction::thr(Opcode::RLA, 6, AddressMode::Abs);
  i[0x3f] = Instruction::thr(Opcode::RLA, 7, AddressMode::AbsX);
  i[0x3b] = Instruction::thr(Opcode::RLA, 7, AddressMode::AbsY);
  i[0x23] = Instruction::two(Opcode::RLA, 8, AddressMode::IndX);
  i[0x33] = Instruction::two(Opcode::RLA, 8, AddressMode::IndY);

  i[0x47] = Instruction::two(Opcode::SRE, 5, AddressMode::Zero);
  i[0x57] = Instruction::two(Opcode::SRE, 6, AddressMode::ZeroX);
  i[0x4f] = Instruction::thr(Opcode::SRE, 6, AddressMode::Abs);
  i[0x5f] = Instruction::thr(Opcode::SRE, 7, AddressMode::AbsX);
  i[0x5b] = Instruction::thr(Opcode::SRE, 7, AddressMode::AbsY);
  i[0x43] = Instruction::two(Opcode::SRE, 8, AddressMode::IndX);
  i[0x53] = Instruction::two(Opcode::SRE, 8, AddressMode::IndY);

  i[0x67] = Instruction::two(Opcode::RRA, 5, AddressMode::Zero);
  i[0x77] = Instruction::two(Opcode::RRA, 6, AddressMode::ZeroX);
  i[0x6f] = Instruction::thr(Opcode::RRA, 6, AddressMode::Abs);
  i[0x7f] = Instruction::thr(Opcode::RRA, 7, AddressMode::AbsX);
  i[0x7b] = Instruction::thr(Opcode::RRA, 7, AddressMode::AbsY);
  i[0x63] = Instruction::two(Opcode::RRA, 8, AddressMode::IndX);
  i[0x73] = Instruction::two(Opcode::RRA, 8, AddressMode::IndY);

  // 1 byte NOPs
  i[0xea] = NOP;
  i[0x1a] = NOP;
  i[0x3a] = NOP;
  i[0x5a] = NOP;
  i[0x7a] = NOP;
  i[0xda] = NOP;
  i[0xfa] = NOP;

  // 2 byte NOPs 2 cycles
  i[0x80] = NOP_2_2;
  i[0x82] = NOP_2_2;
  i[0x89] = NOP_2_2;
  i[0xc2] = NOP_2_2;
  i[0xe2] = NOP_2_2;

  // 2 byte NOPs 3 cycles
  i[0x04] = NOP_2_3;
  i[0x44] = NOP_2_3;
  i[0x64] = NOP_2_3;

  // 2 byte NOPs 4 cycles
  i[0x14] = NOP_2_4;
  i[0x34] = NOP_2_4;
  i[0x54] = NOP_2_4;
  i[0x74] = NOP_2_4;
  i[0xd4] = NOP_2_4;
  i[0xf4] = NOP_2_4;

  // 3 byte nop
  i[0x0c] = NOP_3_4;

  // 3 byte NOPs, absX for cycle
  i[0x1c] = NOP_3_A;
  i[0x3c] = NOP_3_A;
  i[0x5c] = NOP_3_A;
  i[0x7c] = NOP_3_A;
  i[0xdc] = NOP_3_A;
  i[0xfc] = NOP_3_A;

  i
};

impl Instruction {
  pub const fn imp(opcode: Opcode, cycles: usize) -> Self {
    Self {
      opcode,
      cycles,
      mode: AddressMode::Impl,
      size: 1,
    }
  }

  const fn two(opcode: Opcode, cycles: usize, mode: AddressMode) -> Self {
    Self {
      opcode,
      cycles,
      mode,
      size: 2,
    }
  }

  const fn thr(opcode: Opcode, cycles: usize, mode: AddressMode) -> Self {
    Self {
      opcode,
      cycles,
      mode,
      size: 3,
    }
  }

  fn num_extra_cycles(&self) -> usize {
    match self.opcode {
      // these instructions don't add a cycle when they cross page bounds
      Opcode::DCP => 0,
      Opcode::STA => 0,
      Opcode::SLO => 0,
      Opcode::RLA => 0,
      Opcode::SRE => 0,
      Opcode::RRA => 0,
      // isc in indy apparently adds 4 cycles.. bc many instructions in one i guess
      Opcode::ISC => match self.mode {
        AddressMode::IndY => 4,
        _ => 0,
      },
      _ => 1,
    }
  }

  pub fn disassemble(opbyte: u8) -> &'static Instruction {
    #[cfg(debug_assertions)]
    {
      let inst = &INSTRUCTIONS[opbyte as usize];
      if inst == &UNINIT {
        panic!("Uninitialized instruction: {:02X}", opbyte);
      }
      inst
    }

    #[cfg(not(debug_assertions))]
    &INSTRUCTIONS[opbyte as usize]
  }

  pub fn resolve_operand_value_and_address(
    &self,
    cpu: &mut Cpu<impl Bus>,
    operands: &Operands,
  ) -> (u8, u16) {
    let address = self.resolve(cpu, operands, self.num_extra_cycles());
    let value = cpu.bus.read8(address);
    (value, address)
  }

  pub fn resolve_operand_value(&self, cpu: &mut Cpu<impl Bus>, operands: &Operands) -> u8 {
    match self.mode {
      AddressMode::Imm => operands.0,
      _ => {
        let address = self.resolve(cpu, operands, self.num_extra_cycles());
        cpu.bus.read8(address)
      }
    }
  }

  pub fn resolve_operand_address(&self, cpu: &mut Cpu<impl Bus>, operands: &Operands) -> u16 {
    self.resolve(cpu, operands, self.num_extra_cycles())
  }

  fn resolve<B: Bus>(&self, cpu: &mut Cpu<B>, operands: &Operands, num_extra_cycles: usize) -> u16 {
    if self.size == 2 {
      self.resolve_zeropage(cpu, operands.0, num_extra_cycles)
    } else {
      let low = operands.0;
      let high = operands.1;
      let address: u16 = ((high as u16) << 8) | low as u16;

      match self.mode {
        AddressMode::Abs => address,
        AddressMode::AbsX => self.cycle_aware_add(cpu, address, cpu.regs[X], num_extra_cycles),
        AddressMode::AbsY => self.cycle_aware_add(cpu, address, cpu.regs[Y], num_extra_cycles),
        AddressMode::Ind => self.read16(&cpu.bus, low, high),
        _ => panic!(),
      }
    }
  }

  fn resolve_zeropage<B: Bus>(
    &self,
    cpu: &mut Cpu<B>,
    operand: u8,
    likes_extra_cycles: usize,
  ) -> u16 {
    // Zeropage indices should wrap!
    // Casting everything to u16 here is safe because hi == 0x00 == zeropage!
    match self.mode {
      AddressMode::IndX => self.read16(&cpu.bus, operand.wrapping_add(cpu.regs[X]), 0x00), // Zeropage, no carry
      AddressMode::IndY => {
        let address = self.read16(&cpu.bus, operand, 0x00);
        self.cycle_aware_add(cpu, address, cpu.regs[Y], likes_extra_cycles)
      }
      AddressMode::Zero => operand as u16,
      AddressMode::ZeroX => operand.wrapping_add(cpu.regs[X]) as u16, // Zeropage
      AddressMode::ZeroY => operand.wrapping_add(cpu.regs[Y]) as u16, // zeropage
      _ => {
        println!("{:?}", self);
        panic!()
      }
    }
  }

  fn cycle_aware_add<B: Bus>(
    &self,
    cpu: &mut Cpu<B>,
    address: u16,
    v: u8,
    likes_extra_cycles: usize,
  ) -> u16 {
    let res = address.wrapping_add(v as u16);
    if res & 0xff00 != address & 0xff00 {
      // page cross
      cpu.add_extra_cycles(likes_extra_cycles);
    }
    res
  }

  fn read16(&self, mem: &impl Bus, address_low: u8, address_hi: u8) -> u16 {
    let byte1_address = ((address_hi as u16) << 8) | address_low as u16;
    let byte2_address = ((address_hi as u16) << 8) | address_low.wrapping_add(1) as u16;
    let val_low = mem.read8(byte1_address) as u16;
    let val_high = mem.read8(byte2_address) as u16;
    (val_high << 8) | val_low
  }
}
