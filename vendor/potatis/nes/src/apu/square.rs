const DUTY_CYCLES: [[u8; 8]; 4] = [
    [0, 1, 0, 0, 0, 0, 0, 0], // 12.5%
    [0, 1, 1, 0, 0, 0, 0, 0], // 25%
    [0, 1, 1, 1, 1, 0, 0, 0], // 50%
    [1, 0, 0, 1, 1, 1, 1, 1], // 25% negated
];

#[derive(Default)]
pub struct SquareWave {
    enabled: bool,
    duty_cycle: u8,
    volume: u8,
    constant_volume: bool,
    length_counter_halt: bool,
    envelope_divider: u8,
    envelope_decay_level: u8,
    envelope_start: bool,
    
    timer: u16,
    timer_period: u16,
    sequence_position: u8,
    
    length_counter: u8,
    
    // Sweep unit
    sweep_enabled: bool,
    sweep_negate: bool,
    sweep_shift: u8,
    sweep_period: u8,
    sweep_divider: u8,
    sweep_reload: bool,
    channel_number: u8, // 0 for square1, 1 for square2 (affects ones' complement)
}

impl SquareWave {
    pub fn new(channel_number: u8) -> Self {
        Self {
            channel_number,
            ..Default::default()
        }
    }
    
    pub fn write_control(&mut self, value: u8) {
        // $4000/$4004: DDLC VVVV
        self.duty_cycle = (value >> 6) & 0x03;
        self.length_counter_halt = (value & 0x20) != 0;
        self.constant_volume = (value & 0x10) != 0;
        self.volume = value & 0x0F;
        
        if self.constant_volume {
            self.envelope_decay_level = self.volume;
        }
        self.envelope_start = true;
    }
    
    pub fn write_sweep(&mut self, value: u8) {
        // $4001/$4005: EPPP NSSS
        self.sweep_enabled = (value & 0x80) != 0;
        self.sweep_period = (value >> 4) & 0x07;
        self.sweep_negate = (value & 0x08) != 0;
        self.sweep_shift = value & 0x07;
        self.sweep_reload = true;
    }
    
    pub fn write_timer_low(&mut self, value: u8) {
        // $4002/$4006: TTTT TTTT
        self.timer_period = (self.timer_period & 0xFF00) | value as u16;
    }
    
    pub fn write_timer_high(&mut self, value: u8) {
        // $4003/$4007: LLLL LTTT
        self.timer_period = (self.timer_period & 0x00FF) | ((value as u16 & 0x07) << 8);
        
        // Length counter load
        if self.enabled {
            const LENGTH_TABLE: [u8; 32] = [
                10, 254, 20, 2, 40, 4, 80, 6, 160, 8, 60, 10, 14, 12, 26, 14,
                12, 16, 24, 18, 48, 20, 96, 22, 192, 24, 72, 26, 16, 28, 32, 30
            ];
            self.length_counter = LENGTH_TABLE[(value >> 3) as usize];
        }
        
        self.sequence_position = 0;
        self.envelope_start = true;
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
            self.sequence_position = (self.sequence_position + 1) % 8;
        } else {
            self.timer = self.timer.saturating_sub(1);
        }
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
        if !self.enabled || self.length_counter == 0 || self.timer_period < 8 {
            return 0;
        }
        
        // Sweep unit can mute the channel
        if !self.is_sweep_target_valid() {
            return 0;
        }
        
        let duty_value = DUTY_CYCLES[self.duty_cycle as usize][self.sequence_position as usize];
        if duty_value == 0 {
            return 0;
        }
        
        if self.constant_volume {
            self.volume
        } else {
            self.envelope_decay_level
        }
    }

    pub fn length_counter(&self) -> u8 {
        self.length_counter
    }

    fn calculate_target_period(&self) -> u16 {
        let shift_amount = self.timer_period >> self.sweep_shift;
        if self.sweep_negate {
            // Two's complement for channel 1, ones' complement for channel 2
            if self.channel_number == 0 {
                // Square 1: two's complement
                self.timer_period.saturating_sub(shift_amount + 1)
            } else {
                // Square 2: ones' complement
                self.timer_period.saturating_sub(shift_amount)
            }
        } else {
            self.timer_period.saturating_add(shift_amount)
        }
    }

    fn is_sweep_target_valid(&self) -> bool {
        let target = self.calculate_target_period();
        // Target period must be >= 8 and <= 0x7FF
        target >= 8 && target <= 0x7FF
    }

    pub fn tick_sweep(&mut self) {
        if self.sweep_divider == 0 && self.sweep_enabled && self.is_sweep_target_valid() {
            self.timer_period = self.calculate_target_period();
        }

        if self.sweep_divider == 0 || self.sweep_reload {
            self.sweep_divider = self.sweep_period;
            self.sweep_reload = false;
        } else {
            self.sweep_divider -= 1;
        }
    }
}