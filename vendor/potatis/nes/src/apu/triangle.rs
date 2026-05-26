const TRIANGLE_STEPS: [u8; 32] = [
    15, 14, 13, 12, 11, 10, 9, 8, 7, 6, 5, 4, 3, 2, 1, 0,
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15,
];

#[derive(Default)]
pub struct TriangleWave {
    enabled: bool,
    length_counter_halt: bool,
    linear_counter_control: bool,
    linear_counter_reload_value: u8,
    
    timer: u16,
    timer_period: u16,
    sequence_position: u8,
    
    length_counter: u8,
    linear_counter: u8,
    linear_counter_reload_flag: bool,
}

impl TriangleWave {
    pub fn write_control(&mut self, value: u8) {
        // $4008: CRRR RRRR
        self.length_counter_halt = (value & 0x80) != 0;
        self.linear_counter_control = (value & 0x80) != 0; // Same bit
        self.linear_counter_reload_value = value & 0x7F;
    }
    
    pub fn write_timer_low(&mut self, value: u8) {
        // $400A: TTTT TTTT
        self.timer_period = (self.timer_period & 0xFF00) | value as u16;
    }
    
    pub fn write_timer_high(&mut self, value: u8) {
        // $400B: LLLL LTTT
        self.timer_period = (self.timer_period & 0x00FF) | ((value as u16 & 0x07) << 8);
        
        // Length counter load
        if self.enabled {
            const LENGTH_TABLE: [u8; 32] = [
                10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14,
                12, 16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30
            ];
            self.length_counter = LENGTH_TABLE[(value >> 3) as usize];
        }
        
        self.linear_counter_reload_flag = true;
    }
    
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.length_counter = 0;
        }
    }
    
    pub fn tick_timer(&mut self) {
        if self.timer == 0 {
            self.timer = self.timer_period;
            // Triangle channel only advances if both counters are non-zero
            if self.length_counter > 0 && self.linear_counter > 0 {
                self.sequence_position = (self.sequence_position + 1) % 32;
            }
        } else {
            self.timer = self.timer.saturating_sub(1);
        }
    }
    
    pub fn tick_linear_counter(&mut self) {
        if self.linear_counter_reload_flag {
            self.linear_counter = self.linear_counter_reload_value;
        } else if self.linear_counter > 0 {
            self.linear_counter -= 1;
        }
        
        if !self.linear_counter_control {
            self.linear_counter_reload_flag = false;
        }
    }
    
    pub fn tick_length_counter(&mut self) {
        if !self.length_counter_halt && self.length_counter > 0 {
            self.length_counter -= 1;
        }
    }
    
    pub fn output(&self) -> u8 {
        if !self.enabled || self.length_counter == 0 || self.linear_counter == 0 {
            return 0;
        }
        
        // Triangle channel silenced for ultrasonic frequencies (< 2)
        if self.timer_period < 2 {
            return 0;
        }
        
        TRIANGLE_STEPS[self.sequence_position as usize]
    }

    pub fn length_counter(&self) -> u8 {
        self.length_counter
    }
}