#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod cpu;
#[cfg(feature = "debugger")]
pub mod debugger;
mod instructions;
pub mod memory;
pub mod mos6502;
