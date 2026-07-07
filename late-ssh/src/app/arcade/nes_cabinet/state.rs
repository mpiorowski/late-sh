use std::{
    any::Any,
    collections::{HashMap, HashSet},
    panic::{self, AssertUnwindSafe},
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
const INACTIVE_EMU_SLEEP: Duration = Duration::from_millis(50);

pub const ROMS: [RomInfo; 10] = [
    RomInfo {
        title: "Squirrel Domino",
        subtitle: "Domino-clearing puzzle duel",
        bytes: include_bytes!("../../../../assets/nes/squirrel_domino.nes"),
    },
    RomInfo {
        title: "Thwaite",
        subtitle: "Town-defense missile arcade",
        bytes: include_bytes!("../../../../assets/nes/thwaite128.nes"),
    },
    RomInfo {
        title: "DABG",
        subtitle: "Double Action Blaster Guys",
        bytes: include_bytes!("../../../../assets/nes/dabg.nes"),
    },
    RomInfo {
        title: "Falling",
        subtitle: "Dodge-and-collect score chase",
        bytes: include_bytes!("../../../../assets/nes/falling.nes"),
    },
    RomInfo {
        title: "Brick Breaker",
        subtitle: "Breakout-style brick smashing",
        bytes: include_bytes!("../../../../assets/nes/brickbreaker.nes"),
    },
    RomInfo {
        title: "Escape from Pong",
        subtitle: "Pong-from-the-ball puzzle",
        bytes: include_bytes!("../../../../assets/nes/escape_from_pong.nes"),
    },
    RomInfo {
        title: "RHDE",
        subtitle: "Furniture-fight strategy oddity",
        bytes: include_bytes!("../../../../assets/nes/rhde.nes"),
    },
    RomInfo {
        title: "Concentration Room",
        subtitle: "Two-player memory card game",
        bytes: include_bytes!("../../../../assets/nes/concentration_room.nes"),
    },
    RomInfo {
        title: "Zap Ruder",
        subtitle: "Air-hockey and Zapper test toy",
        bytes: include_bytes!("../../../../assets/nes/zap_ruder.nes"),
    },
    RomInfo {
        title: "2048",
        subtitle: "NES tile puzzle",
        bytes: include_bytes!("../../../../assets/nes/2048.nes"),
    },
];

pub const ROM_SQUIRREL_DOMINO: usize = 0;
pub const ROM_THWAITE: usize = 1;
pub const ROM_DABG: usize = 2;
pub const ROM_FALLING: usize = 3;
pub const ROM_BRICK_BREAKER: usize = 4;
pub const ROM_ESCAPE_FROM_PONG: usize = 5;
pub const ROM_RHDE: usize = 6;
pub const ROM_CONCENTRATION_ROOM: usize = 7;
pub const ROM_ZAP_RUDER: usize = 8;
pub const ROM_2048: usize = 9;

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
    active: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
    last_error: Option<String>,
    zoomed: bool,
    pan_x: usize,
    pan_y: usize,
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        let host = SharedHostState::default();
        let desired_rom = Arc::new(AtomicUsize::new(0));
        let active = Arc::new(AtomicBool::new(false));
        let shutdown = Arc::new(AtomicBool::new(false));
        Self {
            selected_rom: 0,
            host,
            desired_rom,
            active,
            shutdown,
            thread: None,
            last_error: None,
            zoomed: false,
            pan_x: 0,
            pan_y: 0,
        }
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
        self.reap_finished_thread();
        if let Some(error) = self.host.error() {
            self.last_error = Some(error);
            self.active.store(false, Ordering::Relaxed);
        }

        self.selected_rom = self.desired_rom.load(Ordering::Relaxed).min(ROMS.len() - 1);
        if self.active.load(Ordering::Relaxed) && self.thread.is_none() {
            self.start_emulator();
        }
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

    pub fn select_rom(&mut self, selected_rom: usize) {
        self.load_rom(selected_rom.min(ROMS.len() - 1));
        self.activate();
    }

    pub fn activate(&mut self) {
        self.active.store(true, Ordering::Relaxed);
        self.start_emulator();
    }

    pub fn deactivate(&mut self) {
        self.active.store(false, Ordering::Relaxed);
        self.host.release_inputs();
    }

    fn load_rom(&mut self, selected_rom: usize) {
        self.selected_rom = selected_rom;
        self.last_error = None;
        self.desired_rom.store(selected_rom, Ordering::Relaxed);
        self.host.clear_frame();
        self.host.clear_error();
        self.pan_x = 0;
        self.pan_y = 0;
    }

    fn start_emulator(&mut self) {
        self.reap_finished_thread();
        if self.thread.is_some() {
            return;
        }

        match spawn_emulator_thread(
            self.host.clone(),
            self.desired_rom.clone(),
            self.active.clone(),
            self.shutdown.clone(),
        ) {
            Ok(thread) => {
                self.last_error = None;
                self.thread = Some(thread);
            }
            Err(err) => {
                self.active.store(false, Ordering::Relaxed);
                self.last_error = Some(err.to_string());
            }
        }
    }

    fn reap_finished_thread(&mut self) {
        if !self
            .thread
            .as_ref()
            .is_some_and(thread::JoinHandle::is_finished)
        {
            return;
        }

        if let Some(thread) = self.thread.take()
            && let Err(err) = thread.join()
        {
            self.last_error = Some(format!(
                "NES emulator crashed: {}",
                panic_payload_message(err.as_ref())
            ));
            self.active.store(false, Ordering::Relaxed);
        }
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

    fn error(&self) -> Option<String> {
        self.inner.lock_recover().error.clone()
    }

    fn set_error(&self, error: String) {
        self.inner.lock_recover().error = Some(error);
    }

    fn clear_error(&self) {
        self.inner.lock_recover().error = None;
    }

    fn release_inputs(&self) {
        let mut state = self.inner.lock_recover();
        state.pressed.clear();
        state.sent.clear();
    }
}

struct HostState {
    frame: Vec<u8>,
    pressed: HashMap<JoypadButton, Instant>,
    sent: HashSet<JoypadButton>,
    reset_requested: bool,
    started_at: Instant,
    error: Option<String>,
}

impl Default for HostState {
    fn default() -> Self {
        Self {
            frame: vec![0; NTSC_WIDTH * NTSC_HEIGHT * 3],
            pressed: HashMap::new(),
            sent: HashSet::new(),
            reset_requested: false,
            started_at: Instant::now(),
            error: None,
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
        let mut state = self.state.inner.lock_recover();
        state.frame.clear();
        state.frame.extend(frame.pixels_ntsc());
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

    fn delay(&self, duration: Duration) {
        thread::sleep(duration);
    }
}

fn spawn_emulator_thread(
    host: SharedHostState,
    desired_rom: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) -> anyhow::Result<thread::JoinHandle<()>> {
    let error_host = host.clone();
    thread::Builder::new()
        .name("nes-cabinet".to_string())
        .spawn(move || {
            if let Err(err) = panic::catch_unwind(AssertUnwindSafe(|| {
                run_emulator(host, desired_rom, active, shutdown);
            })) {
                error_host.set_error(format!(
                    "NES emulator crashed: {}",
                    panic_payload_message(err.as_ref())
                ));
            }
        })
        .context("failed to spawn NES emulator thread")
}

fn run_emulator(
    host: SharedHostState,
    desired_rom: Arc<AtomicUsize>,
    active: Arc<AtomicBool>,
    shutdown: Arc<AtomicBool>,
) {
    let mut loaded_rom = usize::MAX;
    let mut nes: Option<Nes<CabinetHost>> = None;

    while !shutdown.load(Ordering::Relaxed) {
        if !active.load(Ordering::Relaxed) {
            thread::sleep(INACTIVE_EMU_SLEEP);
            continue;
        }

        let requested_rom = desired_rom.load(Ordering::Relaxed).min(ROMS.len() - 1);
        if requested_rom != loaded_rom {
            match load_nes(requested_rom, host.clone()) {
                Ok(loaded_nes) => {
                    host.clear_error();
                    nes = Some(loaded_nes);
                }
                Err(err) => {
                    host.set_error(err.to_string());
                    active.store(false, Ordering::Relaxed);
                    nes = None;
                }
            }
            loaded_rom = requested_rom;
        }

        if let Some(nes) = nes.as_mut() {
            nes.tick();
        } else {
            thread::sleep(INACTIVE_EMU_SLEEP);
        }
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

fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}
