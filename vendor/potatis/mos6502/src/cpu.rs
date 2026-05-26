use crate::instructions::AddressMode;
use crate::instructions::Instruction;
use crate::instructions::Opcode;
use crate::instructions::Operands;
use crate::memory::Bus;

use bitflags::bitflags;

pub const AC: usize = 0;
pub const X: usize = 1;
pub const Y: usize = 2;
pub const SP: usize = 3;

bitflags! {
  pub struct Flag: u8 {
      const C = 0b00000001; // Carry
      const Z = 0b00000010; // Zero
      const I = 0b00000100; // Interrupt (IRQ disable)
      const D = 0b00001000; // Decimal (use BCD for arithmetics)
      const B = 0b00010000; // Break (only set when SR is pushed to stack)
      const UNUSED = 0b00100000; // Unused
      const V = 0b01000000; // Overflow
      const N = 0b10000000; // Negative

      const BUNUSEDMASK = Self::B.bits() | Self::UNUSED.bits();
  }
}

pub struct Cpu<B> {
  pub pc: u16,
  pub flags: Flag,
  pub regs: [u8; 4],
  pub bus: B,
  pub extra_cycles: usize,
}

impl<B: Bus> Cpu<B> {
  // LIFO, top-down, 8 bit range, 0x0100 - 0x01FF
  pub const STACK_TOP: usize = 0x0100;
  pub const STACK_BOTTOM: usize = 0x01ff;

  const NMI_VECTOR: u16 = 0xfffa;
  const RESET_VECTOR: u16 = 0xfffc;
  const IRQ_VECTOR: u16 = 0xfffe;

  pub fn new(mem: B) -> Self {
    Self {
      pc: 0,
      flags: Flag::empty(),
      regs: [0; 4],
      bus: mem,
      extra_cycles: 0,
    }
  }

  pub fn fetch_next_instruction<'a>(&mut self) -> (&'a Instruction, Operands) {
    self.extra_cycles = 0;
    let opbyte = self.bus.read8(self.pc);
    let inst = Instruction::disassemble(opbyte);

    // Only read operands based on instruction size to avoid unnecessary bus reads
    let operands = match inst.size {
      1 => (0, 0),                           // 1-byte instruction (implied) - no operands
      2 => (self.bus.read8(self.pc + 1), 0), // 2-byte instruction - only read first operand
      3 => (self.bus.read8(self.pc + 1), self.bus.read8(self.pc + 2)), // 3-byte instruction - read both operands
      _ => panic!(),
    };

    (inst, operands)
  }

  pub fn execute(&mut self, inst: &Instruction, operands: Operands) -> usize {
    let pc_before_exec = self.pc;
    let opcode = &inst.opcode;

    if opcode == &Opcode::JMP {
      let address = inst.resolve_operand_address(self, &operands);
      if inst.mode == AddressMode::Ind {
        self.set_pc(address + 1);
      }
      self.set_pc(address);
    } else if opcode == &Opcode::STA {
      let address = inst.resolve_operand_address(self, &operands);
      self.bus.write8(self.regs[AC], address);
    } else if opcode == &Opcode::LDA {
      let val = inst.resolve_operand_value(self, &operands);
      self.regs[AC] = val;
      self.flags_set_neg_zero(val)
    } else if opcode == &Opcode::CMP {
      let val = inst.resolve_operand_value(self, &operands);
      self.cmp(AC, val);
    } else {
      match opcode {
        Opcode::JMP | Opcode::STA | Opcode::LDA | Opcode::CMP => unreachable!("unrolled"),
        Opcode::JAM => panic!("jammed"),
        Opcode::NOP => {
          // cycles on absX nops
          if inst.mode == AddressMode::AbsX {
            let _ = inst.resolve_operand_value(self, &operands);
          }
        }
        Opcode::DEX => self.dec_reg(X),
        Opcode::DEY => self.dec_reg(Y),
        Opcode::INX => self.inc_reg(X),
        Opcode::INY => self.inc_reg(Y),
        Opcode::DEC => {
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = val.wrapping_sub(1);
          self.flags_set_neg_zero(res);
          self.bus.write8(res, address);
        }
        Opcode::INC => {
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = val.wrapping_add(1);
          self.flags_set_neg_zero(res);
          self.bus.write8(res, address);
        }
        Opcode::DCP => {
          // DEC oper
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = val.wrapping_sub(1);
          self.bus.write8(res, address);

          // CMP oper
          self.cmp(AC, res);
        }
        Opcode::CLC => self.flags.remove(Flag::C),
        Opcode::CLD => self.flags.remove(Flag::D),
        Opcode::CLI => self.flags.remove(Flag::I),
        Opcode::CLV => self.flags.remove(Flag::V),
        Opcode::LDX => {
          let val = inst.resolve_operand_value(self, &operands);
          self.regs[X] = val;
          self.flags_set_neg_zero(val);
        }
        Opcode::LAX => {
          let val = inst.resolve_operand_value(self, &operands);
          self.regs[AC] = val;
          self.regs[X] = val;
          self.flags_set_neg_zero(val);
        }
        Opcode::SAX => {
          let address = inst.resolve_operand_address(self, &operands);
          let res = self.regs[AC] & self.regs[X];
          self.bus.write8(res, address);
        }
        Opcode::TAX => self.mv_with_neg_zero(AC, X),
        Opcode::TAY => self.mv_with_neg_zero(AC, Y),
        Opcode::TSX => self.mv_with_neg_zero(SP, X),
        Opcode::TXA => self.mv_with_neg_zero(X, AC),
        Opcode::TXS => self.regs[SP] = self.regs[X],
        Opcode::TYA => self.mv_with_neg_zero(Y, AC),
        Opcode::SEC => self.flags |= Flag::C,
        Opcode::SED => self.flags |= Flag::D,
        Opcode::SEI => self.flags |= Flag::I,
        Opcode::LDY => {
          let val = inst.resolve_operand_value(self, &operands);
          self.regs[Y] = val;
          self.flags_set_neg_zero(val)
        }
        Opcode::STX => {
          let address = inst.resolve_operand_address(self, &operands);
          self.bus.write8(self.regs[X], address);
        }
        Opcode::STY => {
          let address = inst.resolve_operand_address(self, &operands);
          self.bus.write8(self.regs[Y], address);
        }
        Opcode::JSR => {
          self.push_word(self.pc + 2);
          let address = inst.resolve_operand_address(self, &operands);
          self.set_pc(address);
        }
        Opcode::RTS => {
          let ret = self.pop_word();
          self.set_pc(ret + 1); // pull PC, PC+1 -> PC
        }
        Opcode::BRK => {
          self.push_word(self.pc + 2);

          let mut res = self.flags.bits();
          res |= Flag::BUNUSEDMASK.bits(); // break and 5 should always be set to 1 on stack
          self.push(res);
          self.flags |= Flag::I;

          // Jump to IRQ vector, TODO cycles
          self.set_pc(self.read16(Self::IRQ_VECTOR));
        }
        Opcode::RTI => {
          let flags = self.pop();
          self.set_flags_ignore_5_4(flags);
          let ret = self.pop_word();
          self.set_pc(ret);
        }
        Opcode::BNE => {
          self.branch_if(operands.0, !self.flags.contains(Flag::Z));
        }
        Opcode::BEQ => {
          self.branch_if(operands.0, self.flags.contains(Flag::Z));
        }
        Opcode::BPL => {
          self.branch_if(operands.0, !self.flags.contains(Flag::N));
        }
        Opcode::BMI => {
          self.branch_if(operands.0, self.flags.contains(Flag::N));
        }
        Opcode::BCC => {
          self.branch_if(operands.0, !self.flags.contains(Flag::C));
        }
        Opcode::BCS => {
          self.branch_if(operands.0, self.flags.contains(Flag::C));
        }
        Opcode::BVC => {
          self.branch_if(operands.0, !self.flags.contains(Flag::V));
        }
        Opcode::BVS => {
          self.branch_if(operands.0, self.flags.contains(Flag::V));
        }
        Opcode::CPY => {
          let val = inst.resolve_operand_value(self, &operands);
          self.cmp(Y, val);
        }
        Opcode::CPX => {
          let val = inst.resolve_operand_value(self, &operands);
          self.cmp(X, val);
        }
        Opcode::SRE => {
          // LSR oper
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = self.shift_right(val);
          self.bus.write8(res, address);

          // EOR oper
          let res = self.regs[AC] ^ res;
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::LSR => {
          match inst.mode {
            AddressMode::Impl => {
              let res = self.shift_right(self.regs[AC]);
              self.regs[AC] = res;
            }
            _ => {
              let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
              let res = self.shift_right(val);
              self.bus.write8(res, address);
            }
          };
        }
        Opcode::SLO => {
          // ASL oper
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = self.shift_left(val);
          self.bus.write8(res, address);

          // ORA oper
          let res = self.regs[AC] | res;
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::RLA => {
          // ROL oper
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = self.rotate_left(val);
          self.bus.write8(res, address);

          // AND oper
          let res = self.regs[AC] & res;
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::RRA => {
          // ROR oper
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = self.rotate_right(val);
          self.bus.write8(res, address);

          // ADC oper
          self.regs[AC] = self.add_with_carry(self.regs[AC], res);
        }
        Opcode::ASL => {
          match inst.mode {
            AddressMode::Impl => {
              let res = self.shift_left(self.regs[AC]);
              self.regs[AC] = res;
            }
            _ => {
              let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
              let res = self.shift_left(val);
              self.bus.write8(res, address);
            }
          };
        }
        Opcode::ROL => {
          match inst.mode {
            AddressMode::Impl => {
              let res = self.rotate_left(self.regs[AC]);
              self.regs[AC] = res;
            }
            _ => {
              let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
              let res = self.rotate_left(val);
              self.bus.write8(res, address);
            }
          };
        }
        Opcode::ROR => {
          match inst.mode {
            AddressMode::Impl => {
              let res = self.rotate_right(self.regs[AC]);
              self.regs[AC] = res;
            }
            _ => {
              let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
              let res = self.rotate_right(val);
              self.bus.write8(res, address);
            }
          };
        }
        Opcode::ADC => {
          let val = inst.resolve_operand_value(self, &operands);
          self.regs[AC] = self.add_with_carry(self.regs[AC], val);
        }
        Opcode::SBC | Opcode::USBC => {
          let val = inst.resolve_operand_value(self, &operands);
          self.regs[AC] = self.sub_with_borrow(self.regs[AC], val);
        }
        Opcode::ISC => {
          // INC oper
          let (val, address) = inst.resolve_operand_value_and_address(self, &operands);
          let res = val.wrapping_add(1);
          self.bus.write8(res, address);

          // SBC oper
          self.regs[AC] = self.sub_with_borrow(self.regs[AC], res);
          // self.add_extra_cycles(10);
        }
        Opcode::EOR => {
          let val = inst.resolve_operand_value(self, &operands);
          let res = self.regs[AC] ^ val;
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::ORA => {
          let val = inst.resolve_operand_value(self, &operands);
          let res = self.regs[AC] | val;
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::AND => {
          let val = inst.resolve_operand_value(self, &operands);
          let res = self.regs[AC] & val;
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::PHA => {
          self.push(self.regs[AC]);
        }
        Opcode::PLA => {
          let res = self.pop();
          self.regs[AC] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::PHX => self.push(self.regs[X]),
        Opcode::PHY => self.push(self.regs[Y]),
        Opcode::PLX => {
          let res = self.pop();
          self.regs[X] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::PLY => {
          let res = self.pop();
          self.regs[Y] = res;
          self.flags_set_neg_zero(res);
        }
        Opcode::PLP => {
          let res = self.pop();
          self.set_flags_ignore_5_4(res);
        }
        Opcode::PHP => {
          let mut res = self.flags.bits();
          res |= Flag::BUNUSEDMASK.bits(); // break and 5 should always be set to 1 on stack
          self.push(res);
        }
        Opcode::BIT => {
          let val = inst.resolve_operand_value(self, &operands);
          let res = self.regs[AC] & val;
          self.flags.set(Flag::Z, res == 0);
          self.flags.set(Flag::N, (val & (1 << 7)) != 0);
          self.flags.set(Flag::V, (val & (1 << 6)) != 0);
        }
        Opcode::ANC | Opcode::ANC2 => {
          let val = inst.resolve_operand_value(self, &operands);
          let res = self.regs[AC] & val;
          self.flags.set(Flag::C, common::bits::is_signed(res));
          self.flags_set_neg_zero(res);
        }
        Opcode::ALR => {
          let val = inst.resolve_operand_value(self, &operands);
          let res = self.regs[AC] & val;
          self.flags.set(Flag::C, (res & 1u8) == 1);
          self.flags_set_neg_zero(res);
        }
      }
    }

    // No jmp; advance.
    // TODO: I was really wrong here, JMP to PC is legit (a way to wait for nmi or something?)
    //       Added tmp check for JMP, should find a better way.
    //       Probably more opcodes than JMP affected.. branch ops??
    if pc_before_exec == self.pc && inst.opcode != Opcode::JMP {
      self.inc_pc(inst.size);
    }

    inst.cycles + self.extra_cycles
  }

  pub fn add_extra_cycles(&mut self, cycles: usize) {
    self.extra_cycles += cycles;
  }

  pub fn set_pc(&mut self, pc: u16) {
    self.pc = pc
  }

  pub fn inc_pc(&mut self, inc: u8) {
    self.pc += inc as u16
  }

  pub fn reset(&mut self) {
    // TODO: Cycles
    self.regs[AC] = 0;
    self.regs[X] = 0;
    self.regs[Y] = 0;
    self.regs[SP] = 0xfd;

    self.flags = Flag::empty();
    self.flags = Flag::I | Flag::UNUSED;

    let start = self.read16(Self::RESET_VECTOR);
    self.set_pc(start);
  }

  pub fn nmi(&mut self) {
    self.interrupt(Self::NMI_VECTOR);
  }

  pub fn irq(&mut self) {
    if !self.flags.contains(Flag::I) {
      self.interrupt(Self::IRQ_VECTOR);
    }
  }

  fn interrupt(&mut self, vector: u16) {
    // TODO: Cycles
    self.push_word(self.pc);

    let mut stackflags = self.flags.bits();
    stackflags &= 0b11101111; // B should be off
    stackflags |= 0b00100000; // unused should be on
    self.push(stackflags);
    self.flags |= Flag::I;

    // TODO cycles
    let vector = self.read16(vector);
    self.set_pc(vector);
  }

  fn push_word(&mut self, val: u16) {
    let ret_high = (val >> 8) as u8;
    let ret_low = (val & 0x00ff) as u8;
    self.push(ret_high);
    self.push(ret_low);
  }

  fn pop_word(&mut self) -> u16 {
    let ret_low = self.pop();
    let ret_high = self.pop();
    (ret_high as u16) << 8 | ret_low as u16
  }

  fn inc_reg(&mut self, reg: usize) {
    let res = self.regs[reg].wrapping_add(1);
    self.regs[reg] = res;
    self.flags_set_neg_zero(res);
  }

  fn dec_reg(&mut self, reg: usize) {
    let res = self.regs[reg].wrapping_sub(1);
    self.regs[reg] = res;
    self.flags_set_neg_zero(res);
  }

  fn mv_with_neg_zero(&mut self, src: usize, dst: usize) {
    let val = self.regs[src];
    self.regs[dst] = val;
    self.flags_set_neg_zero(val);
  }

  fn flags_set_neg_zero(&mut self, res: u8) {
    // Optimized direct bit manipulation instead of bitflags::set
    let mut bits = self.flags.bits();
    bits &= !(Flag::Z.bits() | Flag::N.bits());
    bits |= if res == 0 { Flag::Z.bits() } else { 0 };
    bits |= res & 0x80;
    self.flags = Flag::from_bits_truncate(bits);
  }

  fn cmp(&mut self, reg: usize, val: u8) {
    let (res, overflow) = self.regs[reg].overflowing_sub(val);
    self.flags.set(Flag::C, !overflow);
    self.flags_set_neg_zero(res);
  }

  fn shift_right(&mut self, val: u8) -> u8 {
    // All shift and rotate instructions preserve the bit shifted out in the carry flag.
    self.flags.set(Flag::C, val & 1 != 0);
    let (res, _) = val.overflowing_shr(1);
    self.flags_set_neg_zero(res);
    res
  }

  fn shift_left(&mut self, val: u8) -> u8 {
    // All shift and rotate instructions preserve the bit shifted out in the carry flag.
    self.flags.set(Flag::C, val & (1 << 7) != 0);
    let (res, _) = val.overflowing_shl(1);
    self.flags_set_neg_zero(res);
    res
  }

  fn rotate_left(&mut self, val: u8) -> u8 {
    // All shift and rotate instructions preserve the bit shifted out in the carry flag.
    // rotate left (shifts in carry bit on the right)
    let carry_bit_before_shift = self.flags.contains(Flag::C) as u8;
    self.flags.set(Flag::C, val & (1 << 7) != 0);
    let (mut res, _) = val.overflowing_shl(1);
    res |= carry_bit_before_shift;
    self.flags_set_neg_zero(res);
    res
  }

  fn rotate_right(&mut self, val: u8) -> u8 {
    // All shift and rotate instructions preserve the bit shifted out in the carry flag.
    // rotate right (shifts in CARRY bit on the left) (masswerk says zero bit but I think it's an error)
    let carry_bit_before_shift = self.flags.contains(Flag::C) as u8;
    self.flags.set(Flag::C, val & 1 != 0);
    let (mut res, _) = val.overflowing_shr(1);
    res |= carry_bit_before_shift << 7;
    self.flags_set_neg_zero(res);
    res
  }

  pub fn calc_offset_pc(&self, offset: u8) -> u16 {
    let signed = offset as i8;
    if signed >= 0 {
      let effective_offset = offset as u16;
      self.pc.wrapping_add(effective_offset)
    } else {
      let signed_offset = ((offset as u16) | 0xff00) as i16;
      let effective_offset = (-signed_offset) as u16;
      self.pc.wrapping_sub(effective_offset)
    }
  }

  fn add_with_carry(&mut self, lhs: u8, rhs: u8) -> u8 {
    // if self[Flag::D] == 1 {
    // panic!("implement decimal mode");
    // }

    let (step1, carry1) = lhs.overflowing_add(self.flags.contains(Flag::C) as u8);
    let (res, carry2) = step1.overflowing_add(rhs);
    self
      .flags
      .set(Flag::V, common::bits::is_overflow(res, lhs, rhs));
    self.flags.set(Flag::C, carry1 || carry2);
    self.flags_set_neg_zero(res);
    res
  }

  fn sub_with_borrow(&mut self, lhs: u8, rhs: u8) -> u8 {
    // Do not understand how this works, but it works.
    self.add_with_carry(lhs, rhs ^ 0xff)
  }

  fn push(&mut self, val: u8) {
    let sp = self.regs[SP] as usize;
    let address = (Cpu::<B>::STACK_TOP + sp) as u16;
    self.bus.write8(val, address);
    self.regs[SP] = self.regs[SP].wrapping_sub(1);
  }

  fn pop(&mut self) -> u8 {
    self.regs[SP] = self.regs[SP].wrapping_add(1);
    let sp = self.regs[SP] as usize;
    let address = (Cpu::<B>::STACK_TOP + sp) as u16;
    self.bus.read8(address)
  }

  fn set_flags_ignore_5_4(&mut self, val: u8) {
    let original_b_and_unused = self.flags.bits() & 0b00110000;
    self.flags = Flag::from_bits_truncate((val & !0b00110000) | original_b_and_unused);
  }

  fn branch_if(&mut self, offset: u8, cond: bool) {
    if offset == 0 {
      // (An offset of #0 corresponds to the immedately following address — or a rather odd and expensive NOP.)
      return;
    }
    if cond {
      self.inc_pc(2);
      let branch_target = self.calc_offset_pc(offset);

      // if hi byte changes, we crossed a page boundary and should add extra cycles
      // "add 1 to cycles if branch occurs on same page, add 2 to cycles if branch occurs to different page"
      let crossed_page = self.pc & 0xff00 != branch_target & 0xff00;
      if crossed_page {
        self.add_extra_cycles(2);
      } else {
        self.add_extra_cycles(1);
      }

      self.set_pc(branch_target);
    }
  }

  fn read16(&self, address: u16) -> u16 {
    let val_low = self.bus.read8(address) as u16;
    let val_high = self.bus.read8(address + 1) as u16;
    (val_high << 8) | val_low
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  struct TestBus([u8; 0xffff + 1]);

  impl Bus for TestBus {
    fn read8(&self, address: u16) -> u8 {
      self.0[address as usize]
    }

    fn write8(&mut self, val: u8, address: u16) {
      self.0[address as usize] = val;
    }
  }

  fn sut() -> Cpu<impl Bus> {
    Cpu::new(TestBus([0; 0xffff + 1]))
  }

  #[test]
  fn test_lda() {
    let mut mem = TestBus([0; 0xffff + 1]);
    mem.write8(0xaa, 0x0666);
    mem.write8(0xad, 0); // LDA
    mem.write8(0x66, 1);
    mem.write8(0x06, 2);
    let mut cpu: Cpu<TestBus> = Cpu::new(mem);
    let (i, o) = cpu.fetch_next_instruction();
    assert_eq!(i.opcode, Opcode::LDA);
    assert_eq!(o.0, 0x66);
    assert_eq!(o.1, 0x06);
    cpu.execute(i, o);
    assert_eq!(cpu.regs[AC], 0xaa);
  }

  #[test]
  fn stack_pop_push_should_wrap() {
    let mut cpu = sut();
    cpu.regs[SP] = 0;

    cpu.push(42);
    assert_eq!(cpu.regs[SP], 0xff);

    let val = cpu.pop();
    assert_eq!(val, 42);
    assert_eq!(cpu.regs[SP], 0);
  }

  #[test]
  fn inc_dec_regs_should_wrap() {
    let mut cpu = sut();
    cpu.regs[X] = 0xff;
    cpu.regs[Y] = 0xff;

    cpu.inc_reg(X);
    cpu.inc_reg(Y);

    assert_eq!(cpu.regs[X], 0);
    assert_eq!(cpu.regs[Y], 0);

    cpu.dec_reg(X);
    cpu.dec_reg(Y);

    assert_eq!(cpu.regs[X], 0xff);
    assert_eq!(cpu.regs[Y], 0xff);
  }

  #[test]
  fn set_flags() {
    let mut cpu = sut();
    cpu.set_flags_ignore_5_4(0b10100001);
    assert!(cpu.flags.contains(Flag::N));
    assert!(!cpu.flags.contains(Flag::V));
    assert!(!cpu.flags.contains(Flag::D));
    assert!(!cpu.flags.contains(Flag::I));
    assert!(!cpu.flags.contains(Flag::Z));
    assert!(cpu.flags.contains(Flag::C));

    let mut cpu = sut();
    cpu.set_flags_ignore_5_4(0b11001010);
    assert!(cpu.flags.contains(Flag::N));
    assert!(cpu.flags.contains(Flag::V));
    assert!(cpu.flags.contains(Flag::D));
    assert!(!cpu.flags.contains(Flag::I));
    assert!(cpu.flags.contains(Flag::Z));
    assert!(!cpu.flags.contains(Flag::C));
  }

  #[test]
  fn get_flags() {
    let mut cpu = sut();

    assert_eq!(cpu.flags.bits(), 0b00000000);

    cpu.flags |= Flag::N;
    cpu.flags |= Flag::Z;

    assert_eq!(cpu.flags.bits(), 0b10000010);
  }

  #[test]
  fn add_with_carry() {
    let mut cpu = sut();

    // -5 + -124
    cpu.add_with_carry(0b11111011, 0b10000100);
    assert!(cpu.flags.contains(Flag::V));

    let mut cpu = sut();
    cpu.add_with_carry(255, 1);
    assert!(!cpu.flags.contains(Flag::V));
    assert!(cpu.flags.contains(Flag::C));
    assert!(cpu.flags.contains(Flag::Z));
    assert!(!cpu.flags.contains(Flag::N));

    let mut cpu = sut();
    cpu.add_with_carry(254, 1);
    assert!(!cpu.flags.contains(Flag::V));
    assert!(!cpu.flags.contains(Flag::C));
    assert!(!cpu.flags.contains(Flag::Z));
    assert!(cpu.flags.contains(Flag::N));
  }

  #[test]
  fn cmp() {
    let mut cpu = sut();

    cpu.regs[Y] = 10;
    cpu.cmp(Y, 11);
    assert!(!cpu.flags.contains(Flag::Z));
    assert!(!cpu.flags.contains(Flag::C));
    assert!(cpu.flags.contains(Flag::N));

    cpu.regs[Y] = 10;
    cpu.cmp(Y, 10);
    assert!(cpu.flags.contains(Flag::Z));
    assert!(cpu.flags.contains(Flag::C));
    assert!(!cpu.flags.contains(Flag::N));

    cpu.regs[Y] = 11;
    cpu.cmp(Y, 10);
    assert!(!cpu.flags.contains(Flag::Z));
    assert!(cpu.flags.contains(Flag::C));
    assert!(!cpu.flags.contains(Flag::N));
  }

  #[test]
  fn shift_right() {
    let mut cpu = sut();

    cpu.shift_right(0b001);
    cpu.flags |= Flag::Z;
    cpu.flags |= Flag::C;

    cpu.shift_right(0b100);
    cpu.flags |= Flag::Z;
    cpu.flags |= Flag::C;
  }

  #[test]
  fn offset_pc() {
    let mut cpu = sut();

    cpu.set_pc(0x10);
    assert_eq!(cpu.calc_offset_pc(1), 0x11); // + 1

    cpu.set_pc(0x20); // 32
    assert_eq!(cpu.calc_offset_pc(0xf4), 0x14); // // -12 == 20

    cpu.set_pc(0x0000);
    assert_eq!(cpu.calc_offset_pc(255), 0xffff); // -1

    cpu.set_pc(0xFFFF);
    assert_eq!(cpu.calc_offset_pc(1), 0x0000); // +1

    cpu.set_pc(0x0000);
    assert_eq!(cpu.calc_offset_pc(0xc1), 0xffc1); // -63
  }
}
