use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::AtomicUsize,
    time::Instant,
};

use ringbuf::{
    HeapRb,
    traits::{Consumer, Split},
};

const WAIT: Duration = Duration::from_secs(10);

/// One silent MPEG-1 Layer III frame: 128 kbps, 44.1 kHz, stereo.
fn silent_mp3_frame() -> Vec<u8> {
    let mut frame = vec![0u8; 417];
    frame[..4].copy_from_slice(&[0xFF, 0xFB, 0x90, 0x00]);
    frame
}

/// A stand-in radio stream: serves endless silent MP3 to every client and
/// counts how many are connected right now and how many ever connected.
struct FakeStream {
    url: String,
    open: Arc<AtomicUsize>,
    accepted: Arc<AtomicUsize>,
}

fn spawn_fake_stream() -> FakeStream {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/radio.mp3", listener.local_addr().unwrap());
    let open = Arc::new(AtomicUsize::new(0));
    let accepted = Arc::new(AtomicUsize::new(0));
    let (thread_open, thread_accepted) = (Arc::clone(&open), Arc::clone(&accepted));
    thread::spawn(move || {
        for socket in listener.incoming() {
            let Ok(mut socket) = socket else { return };
            let open = Arc::clone(&thread_open);
            thread_accepted.fetch_add(1, Ordering::SeqCst);
            open.fetch_add(1, Ordering::SeqCst);
            thread::spawn(move || {
                let mut request = [0u8; 1024];
                let _ = socket.read(&mut request);
                let _ = socket
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nConnection: close\r\n\r\n");
                let frame = silent_mp3_frame();
                // Writing fails once the client hangs up.
                while socket.write_all(&frame).is_ok() {
                    thread::sleep(Duration::from_millis(1));
                }
                open.fetch_sub(1, Ordering::SeqCst);
            });
        }
    });
    FakeStream {
        url,
        open,
        accepted,
    }
}

fn wait_until(what: &str, condition: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting until {what}");
        thread::sleep(Duration::from_millis(5));
    }
}

struct Harness {
    stream: FakeStream,
    muted: Arc<AtomicBool>,
    native_source_selected: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Runs the decoder thread against a fake stream, with a thread standing in
/// for the output callback (it drains the queue, as the callback does even
/// while silenced).
fn start_decoder(muted: bool) -> Harness {
    let stream = spawn_fake_stream();
    let spec = AudioSpec {
        sample_rate: 44_100,
        channels: 2,
    };
    let (queue_tx, mut queue_rx) = HeapRb::<f32>::new(44_100 * 2 * 2).split();
    let muted = Arc::new(AtomicBool::new(muted));
    let native_source_selected = Arc::new(AtomicBool::new(true));
    let stop = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);

    spawn_decoder_thread(
        Arc::new(Mutex::new(stream.url.clone())),
        Arc::new(AtomicU64::new(0)),
        Arc::new(AtomicU64::new(0)),
        Arc::new(AtomicBool::new(true)),
        Arc::clone(&native_source_selected),
        Arc::clone(&muted),
        queue_tx,
        spec,
        44_100,
        Arc::clone(&stop),
        ready_tx,
        0,
    );
    ready_rx.recv().unwrap().unwrap();

    let drain_stop = Arc::clone(&stop);
    thread::spawn(move || {
        while !drain_stop.load(Ordering::Relaxed) {
            while queue_rx.try_pop().is_some() {}
            thread::sleep(Duration::from_millis(1));
        }
    });

    Harness {
        stream,
        muted,
        native_source_selected,
        stop,
    }
}

#[test]
fn muting_closes_the_stream_and_unmuting_reopens_it() {
    let harness = start_decoder(false);
    let open = &harness.stream.open;
    wait_until("the stream is open", || open.load(Ordering::SeqCst) == 1);

    harness.muted.store(true, Ordering::Relaxed);
    wait_until("mute closed the stream", || {
        open.load(Ordering::SeqCst) == 0
    });

    let before = harness.stream.accepted.load(Ordering::SeqCst);
    harness.muted.store(false, Ordering::Relaxed);
    wait_until("unmute reopened the stream", || {
        open.load(Ordering::SeqCst) == 1
    });
    assert_eq!(
        harness.stream.accepted.load(Ordering::SeqCst),
        before + 1,
        "exactly one new connection"
    );
}

#[test]
fn leaving_the_native_source_closes_the_stream() {
    let harness = start_decoder(false);
    let open = &harness.stream.open;
    wait_until("the stream is open", || open.load(Ordering::SeqCst) == 1);

    // YouTube selected: the CLI has nothing native to play.
    harness
        .native_source_selected
        .store(false, Ordering::Relaxed);
    wait_until("the source change closed the stream", || {
        open.load(Ordering::SeqCst) == 0
    });

    harness.native_source_selected.store(true, Ordering::Relaxed);
    wait_until("returning to radio reopened the stream", || {
        open.load(Ordering::SeqCst) == 1
    });
}
