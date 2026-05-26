const DMC_PERIODS: [u16; 16] = [
    428, 380, 340, 320, 286, 254, 226, 214, 190, 160, 142, 128, 106, 84, 72, 54
];

#[derive(Default)]
pub struct DmcChannel {
    enabled: bool,
    irq_enabled: bool,
    loop_flag: bool,
    frequency_index: u8,
    load_counter: u8,
    
    // Current state
    timer: u16,
    timer_period: u16,
    
    // Sample address and length
    sample_address: u16,
    sample_length: u16,
    current_address: u16,
    bytes_remaining: u16,
    
    // Output and bit manipulation
    output_level: u8,
    shift_register: u8,
    bits_remaining: u8,
    silence: bool,
    
    // Buffer
    sample_buffer: u8,
    sample_buffer_empty: bool,
}

impl DmcChannel {
    pub fn write_control(&mut self, value: u8) {
        // $4010: IL-- RRRR
        self.irq_enabled = (value & 0x80) != 0;
        self.loop_flag = (value & 0x40) != 0;
        self.frequency_index = value & 0x0F;
        self.timer_period = DMC_PERIODS[self.frequency_index as usize];
    }
    
    pub fn write_load_counter(&mut self, value: u8) {
        // $4011: -DDD DDDD
        self.load_counter = value & 0x7F;
        self.output_level = self.load_counter;
    }
    
    pub fn write_sample_address(&mut self, value: u8) {
        // $4012: AAAA AAAA
        // Sample address = $C000 + (A * 64)
        self.sample_address = 0xC000 + ((value as u16) << 6);
    }
    
    pub fn write_sample_length(&mut self, value: u8) {
        // $4013: LLLL LLLL  
        // Sample length = (L * 16) + 1
        self.sample_length = ((value as u16) << 4) + 1;
    }
    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.bytes_remaining = 0;
        } else if self.bytes_remaining == 0 {
            self.restart_sample();
        }
    }
    
    fn restart_sample(&mut self) {
        self.current_address = self.sample_address;
        self.bytes_remaining = self.sample_length;
    }
    
    pub fn tick_timer(&mut self) -> Option<u16> {
        if self.timer == 0 {
            self.timer = self.timer_period;
            
            // Clock the output unit
            if !self.silence {
                if (self.shift_register & 1) != 0 {
                    if self.output_level <= 125 {
                        self.output_level += 2;
                    }
                } else if self.output_level >= 2 {
                    self.output_level -= 2;
                }
            }
            
            self.shift_register >>= 1;
            self.bits_remaining = self.bits_remaining.saturating_sub(1);
            
            // If we've processed all 8 bits, get next sample
            if self.bits_remaining == 0 {
                self.bits_remaining = 8;
                
                if self.sample_buffer_empty {
                    self.silence = true;
                } else {
                    self.silence = false;
                    self.shift_register = self.sample_buffer;
                    self.sample_buffer_empty = true;
                    
                    // Request next byte from memory if available
                    if self.bytes_remaining > 0 {
                        return Some(self.current_address);
                    }
                }
            }
        } else {
            self.timer = self.timer.saturating_sub(1);
        }
        
        None
    }
    
    pub fn load_sample_byte(&mut self, byte: u8) {
        if self.sample_buffer_empty && self.bytes_remaining > 0 {
            self.sample_buffer = byte;
            self.sample_buffer_empty = false;
            
            self.current_address = self.current_address.wrapping_add(1);
            if self.current_address == 0x0000 {
                self.current_address = 0x8000; // Wrap to $8000
            }
            
            self.bytes_remaining -= 1;
            
            // If sample finished
            if self.bytes_remaining == 0 {
                if self.loop_flag {
                    self.restart_sample();
                } else if self.irq_enabled {
                    // TODO: Trigger IRQ
                }
            }
        }
    }
    
    pub fn output(&self) -> u8 {
        if self.enabled {
            self.output_level
        } else {
            0
        }
    }
    
    pub fn bytes_remaining(&self) -> u16 {
        self.bytes_remaining
    }
}