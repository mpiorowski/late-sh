const NOISE_PERIODS: [u16; 16] = [
    4, 8, 16, 32, 64, 96, 128, 160, 202, 254, 380, 508, 762, 1016, 2034, 4068
];

pub struct NoiseChannel {
    enabled: bool,
    volume: u8,
    constant_volume: bool,
    length_counter_halt: bool,
    envelope_divider: u8,
    envelope_decay_level: u8,
    envelope_start: bool,
    
    mode: bool, // false = 15-bit, true = 6-bit
    timer: u16,
    timer_period: u16,
    
    length_counter: u8,
    shift_register: u16,
}

impl Default for NoiseChannel {
    fn default() -> Self {
        Self {
            enabled: false,
            volume: 0,
            constant_volume: false,
            length_counter_halt: false,
            envelope_divider: 0,
            envelope_decay_level: 0,
            envelope_start: false,
            mode: false,
            timer: 0,
            timer_period: 0,
            length_counter: 0,
            shift_register: 1, // Initialize to 1, not 0
        }
    }
}

impl NoiseChannel {
    pub fn write_control(&mut self, value: u8) {
        // $400C: --LC VVVV
        self.length_counter_halt = (value & 0x20) != 0;
        self.constant_volume = (value & 0x10) != 0;
        self.volume = value & 0x0F;
        
        if self.constant_volume {
            self.envelope_decay_level = self.volume;
        }
        self.envelope_start = true;
    }
    
    pub fn write_period(&mut self, value: u8) {
        // $400E: M--- PPPP
        self.mode = (value & 0x80) != 0;
        let period_index = value & 0x0F;
        self.timer_period = NOISE_PERIODS[period_index as usize];
    }
    
    pub fn write_length(&mut self, value: u8) {
        // $400F: LLLL L---
        if self.enabled {
            const LENGTH_TABLE: [u8; 32] = [
                10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14,
                12, 16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30
            ];
            self.length_counter = LENGTH_TABLE[(value >> 3) as usize];
        }
        
        self.envelope_start = true;
    }
    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        } else if self.shift_register == 0 {
            // Initialize shift register if it's zero
            self.shift_register = 1;
        }
    }
    
    pub fn tick_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            self.tick_shift_register();
        } else {
            self.timer = self.timer.saturating_sub(1);
        }
    }
    
    fn tick_shift_register(&mut self) {
        // Linear Feedback Shift Register (LFSR)
        let bit0 = self.shift_register & 1;
        let other_bit = if self.mode {
            // 6-bit mode: feedback from bit 6
            (self.shift_register >> 6) & 1
        } else {
            // 15-bit mode: feedback from bit 1
            (self.shift_register >> 1) & 1
        };
        
        let feedback = bit0 ^ other_bit;
        self.shift_register = (self.shift_register >> 1) | (feedback << 14);
    }
    
    pub fn tick_envelope(&mut self) {
        if self.envelope_start {
            self.envelope_decay_level = 15;
            self.envelope_divider = self.volume;
            self.envelope_start = false;
        } else if self.envelope_divider > 0 {
            self.envelope_divider -= 1;
        } else {
            self.envelope_divider = self.volume;
            if self.envelope_decay_level > 0 {
                self.envelope_decay_level -= 1;
            } else if self.length_counter_halt {
                self.envelope_decay_level = 15;
            }
        }
    }
    
    pub fn tick_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }
    
    pub fn output(&self) -> u8 {
        if !self.enabled || self.length_counter == 0 {
            return 0;
        }
        
        // Output depends on bit 0 of shift register
        if (self.shift_register & 1) == 0 {
            let volume = if self.constant_volume {
                self.volume
            } else {
                self.envelope_decay_level
            };
            
            // NES APU volume table (non-linear)
            const VOLUME_TABLE: [u8; 16] = [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15
            ];
            
            // Apply volume table and scale down for noise
            let linear_volume = VOLUME_TABLE[volume as usize];
            linear_volume / 3 // Reduce noise to ~33% of squares
        } else {
            0
        }
    }

    pub fn length_counter(&self) -> u8 {
        self.length_counter
    }
}