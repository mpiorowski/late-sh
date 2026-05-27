use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use anyhow::Context;
use late_core::MutexRecover;
use nes::{
    cartridge::Cartridge,
    frame::{NTSC_HEIGHT, NTSC_WIDTH, RenderFrame},
    joypad::{Joypad, JoypadButton, JoypadEvent},
    nes::{HostEvent, HostPlatform, Nes},
};

const PRESS_RELEASED_AFTER: Duration = Duration::from_millis(250);
const TICKS_PER_EMU_LOOP: usize = 12_000;
const EMU_LOOP_SLEEP: Duration = Duration::from_millis(8);

pub const ROMS: [RomInfo; 3] = [
    RomInfo {
        title: "Nova the Squirrel",
        subtitle: "Open-source platformer",
        bytes: include_bytes!("../../../../assets/nes/Nova_the_Squirrel.nes"),
    },
    RomInfo {
        title: "2048",
        subtitle: "NES tile puzzle",
        bytes: include_bytes!("../../../../assets/nes/2048.nes"),
    },
    RomInfo {
        title: "Life",
        subtitle: "Cellular automaton toy",
        bytes: include_bytes!("../../../../assets/nes/life.nes"),
    },
];

#[derive(Clone, Copy)]
pub struct RomInfo {
    pub title: &'static str,
    pub subtitle: &'static str,
    pub bytes: &'static [u8],
}

pub struct State {
    selected_rom: usize,
    host: SharedHostState,
    desired_rom: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    last_error: Option<String>,
    zoomed: bool,
    pan_x: usize,
    pan_y: usize,
}

impl State {
    pub fn new() -> anyhow::Result<Self> {
        for (idx, rom) in ROMS.iter().enumerate() {
            load_cartridge(idx).with_context(|| format!("failed to validate {}", rom.title))?;
        }
        let host = SharedHostState::default();
        let desired_rom = Arc::new(AtomicUsize::new(0));
        let shutdown = Arc::new(AtomicBool::new(false));
        let thread = Some(spawn_emulator_thread(
            host.clone(),
            desired_rom.clone(),
            shutdown.clone(),
        )?);
        Ok(Self {
            selected_rom: 0,
            host,
            desired_rom,
            shutdown,
            thread,
            last_error: None,
            zoomed: false,
            pan_x: 0,
            pan_y: 0,
        })
    }

    pub fn selected_rom(&self) -> usize {
        self.selected_rom
    }

    pub fn rom(&self) -> RomInfo {
        ROMS[self.selected_rom]
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn zoomed(&self) -> bool {
        self.zoomed
    }

    pub fn pan(&self) -> (usize, usize) {
        (self.pan_x, self.pan_y)
    }

    pub fn frame(&self) -> Vec<u8> {
        self.host.frame()
    }

    pub fn tick(&mut self) {
        self.selected_rom = self.desired_rom.load(Ordering::Relaxed).min(ROMS.len() - 1);
    }

    pub fn press(&mut self, button: JoypadButton) {
        self.host.press(button);
    }

    pub fn reset(&mut self) {
        self.host.request_reset();
    }

    pub fn toggle_zoom(&mut self) {
        self.zoomed = !self.zoomed;
    }

    pub fn pan_zoom(&mut self, dx: isize, dy: isize) {
        let step_x = 16isize;
        let step_y = 16isize;
        let x = self.pan_x as isize + dx * step_x;
        let y = self.pan_y as isize + dy * step_y;
        self.pan_x = x.clamp(0, (NTSC_WIDTH - 1) as isize) as usize;
        self.pan_y = y.clamp(0, (NTSC_HEIGHT - 1) as isize) as usize;
    }

    pub fn next_rom(&mut self) {
        self.load_rom((self.selected_rom + 1) % ROMS.len());
    }

    pub fn prev_rom(&mut self) {
        self.load_rom((self.selected_rom + ROMS.len() - 1) % ROMS.len());
    }

    fn load_rom(&mut self, selected_rom: usize) {
        self.selected_rom = selected_rom;
        self.last_error = None;
        self.desired_rom.store(selected_rom, Ordering::Relaxed);
        self.host.clear_frame();
        self.pan_x = 0;
        self.pan_y = 0;
    }
}

impl Drop for State {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone, Default)]
struct SharedHostState {
    inner: Arc<Mutex<HostState>>,
}

impl SharedHostState {
    fn frame(&self) -> Vec<u8> {
        self.inner.lock_recover().frame.clone()
    }

    fn press(&self, button: JoypadButton) {
        self.inner
            .lock_recover()
            .pressed
            .insert(button, Instant::now());
    }

    fn request_reset(&self) {
        self.inner.lock_recover().reset_requested = true;
    }

    fn clear_frame(&self) {
        self.inner.lock_recover().frame.fill(0);
    }
}

struct HostState {
    frame: Vec<u8>,
    pressed: HashMap<JoypadButton, Instant>,
    sent: HashSet<JoypadButton>,
    reset_requested: bool,
    started_at: Instant,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            frame: vec![0; NTSC_WIDTH * NTSC_HEIGHT * 3],
            pressed: HashMap::new(),
            sent: HashSet::new(),
            reset_requested: false,
            started_at: Instant::now(),
        }
    }
}

pub struct CabinetHost {
    state: SharedHostState,
}

impl CabinetHost {
    fn new(state: SharedHostState) -> Self {
        Self { state }
    }
}

impl HostPlatform for CabinetHost {
    fn render(&mut self, frame: &RenderFrame) {
        self.state.inner.lock_recover().frame = frame.pixels_ntsc().collect();
    }

    fn poll_events(&mut self, joypad: &mut Joypad) -> HostEvent {
        let mut state = self.state.inner.lock_recover();
        if state.reset_requested {
            state.reset_requested = false;
            state.pressed.clear();
            state.sent.clear();
            return HostEvent::Reset;
        }

        let now = Instant::now();
        let expired: Vec<JoypadButton> = state
            .pressed
            .iter()
            .filter_map(|(button, pressed_at)| {
                (now.duration_since(*pressed_at) >= PRESS_RELEASED_AFTER).then_some(*button)
            })
            .collect();
        for button in expired {
            state.pressed.remove(&button);
        }

        let active: HashSet<JoypadButton> = state.pressed.keys().copied().collect();
        let to_release: Vec<JoypadButton> = state.sent.difference(&active).copied().collect();
        let to_press: Vec<JoypadButton> = active.difference(&state.sent).copied().collect();

        for button in to_release {
            joypad.on_event(JoypadEvent::Release(button));
            state.sent.remove(&button);
        }
        for button in to_press {
            joypad.on_event(JoypadEvent::Press(button));
            state.sent.insert(button);
        }

        HostEvent::Nothing
    }

    fn elapsed_millis(&self) -> usize {
        self.state
            .inner
            .lock_recover()
            .started_at
            .elapsed()
            .as_millis() as usize
    }
}

fn spawn_emulator_thread(
    host: SharedHostState,
    desired_rom: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("nes-cabinet".to_string())
        .spawn(move || run_emulator(host, desired_rom, shutdown))
        .context("failed to spawn NES emulator thread")
}

fn run_emulator(host: SharedHostState, desired_rom: Arc<AtomicUsize>, shutdown: Arc<AtomicBool>) {
    let mut loaded_rom = usize::MAX;
    let mut nes: Option<Nes<CabinetHost>> = None;

    while !shutdown.load(Ordering::Relaxed) {
        let requested_rom = desired_rom.load(Ordering::Relaxed).min(ROMS.len() - 1);
        if requested_rom != loaded_rom {
            nes = load_nes(requested_rom, host.clone()).ok();
            loaded_rom = requested_rom;
        }

        if let Some(nes) = nes.as_mut() {
            for _ in 0..TICKS_PER_EMU_LOOP {
                nes.tick();
            }
        }

        thread::sleep(EMU_LOOP_SLEEP);
    }
}

fn load_nes(selected_rom: usize, host: SharedHostState) -> anyhow::Result<Nes<CabinetHost>> {
    let cartridge = load_cartridge(selected_rom)?;
    let mut nes = Nes::insert(cartridge, CabinetHost::new(host));
    nes.fps_max(60);
    Ok(nes)
}

fn load_cartridge(selected_rom: usize) -> anyhow::Result<Cartridge> {
    Cartridge::blow_dust_no_heap(ROMS[selected_rom].bytes)
        .map_err(|err| anyhow::anyhow!("{err}"))
        .with_context(|| format!("failed to load NES ROM {}", ROMS[selected_rom].title))
}
