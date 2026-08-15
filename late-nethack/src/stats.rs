// The stats session: a log-streaming SSH session for late-ssh's ingestion
// task. late-ssh connects with the reserved `late_stats` username (inside the
// reserved `late_*` handle namespace, so no player can ever claim it) and,
// instead of a nethack child on a PTY, gets a tail of the append-only log
// files nethack writes into VAR_PLAYGROUND: `xlogfile` (one line per finished
// game; XLOGFILE is compiled in unconditionally on 5.0.0) and `livelog`
// (live mid-run achievement events; needs the LIVELOG compile chain plus the
// sysconf `LIVELOG=` mask, both asserted/set fail-closed in
// docker/doors/nethack.Dockerfile).
//
// Protocol, deliberately dumb (the late-dcss twin, `late-dcss/src/stats.rs`):
// - The client sends its per-file byte offsets in env requests before the
//   shell ([`CURSORS_ENV_VAR`], value `xlogfile:123,livelog:456`; a large
//   cursor set arrives split across several requests, concatenated by
//   [`append_cursors`]; a missing file starts at 0, so a fresh cursor
//   ingests the whole history already on the PVC).
// - The host streams one frame per complete log line,
//   `<file-id>\t<offset>\t<line>\n`, where `<offset>` is the byte offset
//   AFTER the line in the source file — exactly the next cursor, so the
//   client can persist it per line. A trailing partial line waits for its
//   newline. (NetHack's own fields are tab-separated too; the client splits
//   the frame with `splitn(3, '\t')`, so embedded tabs survive.)
// - The host follows the files as they grow (tail -f semantics, coarse poll).
//   It stays stateless: no cursor storage, no parsing, no DB. All parsing
//   lives in late-ssh, so parser fixes never need a door redeploy.
//
// The env var name and frame shape are duplicated in late-ssh's ingestion
// client (`late-ssh/src/app/door/ingest/stream.rs`), the same cross-crate
// contract style as `rc.rs`/`identity.rs`; keep the copies in sync.

use std::collections::HashMap;
use std::time::Duration;

use russh::ChannelId;
use russh::server::Handle;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::{mpsc, watch};

/// The reserved SSH username that opens a stats session instead of a game.
pub(crate) const STATS_USERNAME: &str = "late_stats";

/// Env request carrying the client's per-file byte offsets.
pub(crate) const CURSORS_ENV_VAR: &str = "LATE_DOOR_STATS_CURSORS";

/// The files this host streams, by stable frame id, relative to
/// VAR_PLAYGROUND (the writable playground the game itself writes them into;
/// SCOREPREFIX in NetHack terms).
const FILES: &[(&str, &str)] = &[("xlogfile", "xlogfile"), ("livelog", "livelog")];

/// How often to look for new lines once caught up. Coarse on purpose: the
/// consumer is a leaderboard/badge pipeline, seconds of latency are fine.
const POLL_INTERVAL: Duration = Duration::from_secs(2);

/// Read chunk size while catching up. Backfill of a large xlogfile streams in
/// these increments; `Handle::data` awaits SSH window credit, so a slow
/// client naturally backpressures the reads.
const READ_CHUNK: usize = 64 * 1024;

/// Parse the client's cursor env value (`xlogfile:123,livelog:456`).
/// Unknown ids are ignored; malformed entries fall back to 0 via the lookup
/// default (re-ingest is idempotent client-side, so starting over is safe).
pub(crate) fn parse_cursors(value: &str) -> HashMap<String, u64> {
    value
        .split(',')
        .filter_map(|part| {
            let (id, offset) = part.split_once(':')?;
            Some((id.trim().to_string(), offset.trim().parse().ok()?))
        })
        .collect()
}

/// Merge one cursor env request into the accumulated value. The client splits
/// a large cursor set across several requests (entries never split across a
/// boundary); the values concatenate with the same `,` the entry list already
/// uses, so [`parse_cursors`] reads the merged whole.
pub(crate) fn append_cursors(current: Option<String>, value: &str) -> String {
    match current {
        Some(mut merged) if !merged.is_empty() => {
            merged.push(',');
            merged.push_str(value);
            merged
        }
        _ => value.to_string(),
    }
}

/// Frame every complete line in `bytes` (read starting at `offset` in the
/// source file). Returns the framed output and how many source bytes were
/// consumed; a trailing partial line is left unconsumed so it is re-read once
/// its newline lands.
fn frame_lines(file_id: &str, offset: u64, bytes: &[u8]) -> (Vec<u8>, u64) {
    let mut out = Vec::new();
    let mut consumed = 0usize;
    while let Some(nl) = bytes[consumed..].iter().position(|&b| b == b'\n') {
        let line = &bytes[consumed..consumed + nl];
        consumed += nl + 1;
        let next_cursor = offset + consumed as u64;
        out.extend_from_slice(file_id.as_bytes());
        out.push(b'\t');
        out.extend_from_slice(next_cursor.to_string().as_bytes());
        out.push(b'\t');
        out.extend_from_slice(line);
        out.push(b'\n');
    }
    (out, consumed as u64)
}

/// Per-session host for one stats stream. Owns a detached background task
/// that tails the log files into the SSH channel; dropping the host (client
/// EOF/close) drops `_stop_tx`, which the task observes and exits on.
pub(crate) struct StatsHost {
    _stop_tx: mpsc::Sender<()>,
}

impl StatsHost {
    pub(crate) fn spawn(
        var_dir: String,
        cursors_env: Option<String>,
        handle: Handle,
        channel: ChannelId,
        shutdown_rx: watch::Receiver<bool>,
    ) -> Self {
        let cursors = parse_cursors(cursors_env.as_deref().unwrap_or(""));
        let (stop_tx, stop_rx) = mpsc::channel::<()>(1);
        tokio::spawn(async move {
            run_stream(var_dir, cursors, handle, channel, stop_rx, shutdown_rx).await;
        });
        Self { _stop_tx: stop_tx }
    }
}

struct FileTail {
    id: &'static str,
    path: String,
    offset: u64,
}

async fn run_stream(
    var_dir: String,
    cursors: HashMap<String, u64>,
    handle: Handle,
    channel: ChannelId,
    mut stop_rx: mpsc::Receiver<()>,
    mut shutdown_rx: watch::Receiver<bool>,
) {
    let base = var_dir.trim_end_matches('/');
    let mut tails: Vec<FileTail> = FILES
        .iter()
        .map(|(id, rel)| FileTail {
            id,
            path: format!("{base}/{rel}"),
            offset: cursors.get(*id).copied().unwrap_or(0),
        })
        .collect();
    tracing::info!(
        cursors = ?tails.iter().map(|t| (t.id, t.offset)).collect::<Vec<_>>(),
        "stats session streaming"
    );

    loop {
        for tail in &mut tails {
            if pump(tail, &handle, channel).await.is_err() {
                // SSH channel gone (client disconnect): nothing to clean up,
                // the client owns all state.
                return;
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(POLL_INTERVAL) => {}
            // StatsHost dropped (client EOF/close).
            _ = stop_rx.recv() => break,
            // Host SIGTERM: close so the client reconnects to the new pod.
            res = shutdown_rx.changed() => match res {
                Ok(()) if *shutdown_rx.borrow() => break,
                Ok(()) => {}
                Err(_) => break,
            },
        }
    }
    let _ = handle.eof(channel).await;
    let _ = handle.close(channel).await;
}

/// Stream every complete line the file has grown by since `tail.offset`.
/// Returns Err only when the SSH channel is gone.
async fn pump(tail: &mut FileTail, handle: &Handle, channel: ChannelId) -> Result<(), ()> {
    // Missing file is normal: nethack append-creates xlogfile/livelog on
    // first use.
    let Ok(meta) = tokio::fs::metadata(&tail.path).await else {
        return Ok(());
    };
    if meta.len() < tail.offset {
        // Append-only by contract, so a shrunk file means the playground was
        // rebuilt (disaster recovery). Start over; the client's idempotent
        // inserts make the re-ingest a no-op for anything it already has.
        tracing::warn!(
            file = tail.id,
            offset = tail.offset,
            len = meta.len(),
            "log file shrank below cursor; restarting from 0"
        );
        tail.offset = 0;
    }

    loop {
        let mut file = match tokio::fs::File::open(&tail.path).await {
            Ok(file) => file,
            Err(e) => {
                tracing::warn!(file = tail.id, error = ?e, "failed to open log file");
                return Ok(());
            }
        };
        if file
            .seek(std::io::SeekFrom::Start(tail.offset))
            .await
            .is_err()
        {
            return Ok(());
        }
        let mut buf = vec![0u8; READ_CHUNK];
        let mut filled = 0usize;
        // Fill up to a chunk so a partial line at a chunk boundary is framed
        // in one pass instead of dropped.
        while filled < buf.len() {
            match file.read(&mut buf[filled..]).await {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) => {
                    tracing::warn!(file = tail.id, error = ?e, "failed to read log file");
                    return Ok(());
                }
            }
        }
        if filled == 0 {
            return Ok(());
        }
        let (frames, consumed) = frame_lines(tail.id, tail.offset, &buf[..filled]);
        if consumed == 0 {
            // Only a partial line so far; wait for its newline.
            return Ok(());
        }
        tail.offset += consumed;
        if !frames.is_empty() && handle.data(channel, frames).await.is_err() {
            return Err(());
        }
        if filled < READ_CHUNK {
            // Caught up to EOF.
            return Ok(());
        }
    }
}

#[cfg(test)]
#[path = "stats_test.rs"]
mod stats_test;
