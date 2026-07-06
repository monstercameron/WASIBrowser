//! System log + crash reporting.
//!
//! - `logs/system.log` — append-only, timestamped, flushed per line so it can
//!   be tailed live (`Get-Content logs\system.log -Wait -Tail 20`) while the
//!   renderer runs.
//! - `logs/crash-<ts>.log` — written by the panic hook: panic message,
//!   backtrace, and the last [`RING_CAP`] system-log lines as a black box.
//! - `logs/session.running` — marker present while a session is alive; if it
//!   already exists at startup the previous session died hard (native crash,
//!   kill, power loss) and that gets logged.

use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const RING_CAP: usize = 400;

struct SystemLog {
    file: Mutex<BufWriter<File>>,
    ring: Mutex<VecDeque<String>>,
    dir: PathBuf,
}

static LOG: OnceLock<SystemLog> = OnceLock::new();

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string()
}

/// Initialize the logger. Returns the system log path.
pub fn init() -> anyhow::Result<PathBuf> {
    let dir = std::env::current_dir()?.join("logs");
    fs::create_dir_all(&dir)?;
    let path = dir.join("system.log");
    let file = OpenOptions::new().create(true).append(true).open(&path)?;
    let unclean = dir.join("session.running").exists();

    LOG.set(SystemLog {
        file: Mutex::new(BufWriter::new(file)),
        ring: Mutex::new(VecDeque::new()),
        dir: dir.clone(),
    })
    .map_err(|_| anyhow::anyhow!("logger initialized twice"))?;

    log(
        "sys",
        &format!(
            "==== session start pid={} v{} ====",
            std::process::id(),
            env!("CARGO_PKG_VERSION")
        ),
    );
    if unclean {
        log(
            "crash",
            "previous session did NOT exit cleanly (native crash, kill, or power loss) - \
             see the log tail above this session marker",
        );
    }
    fs::write(dir.join("session.running"), std::process::id().to_string())?;
    Ok(path)
}

/// Append one timestamped line to the system log (and the crash ring buffer).
pub fn log(category: &str, message: &str) {
    let Some(l) = LOG.get() else { return };
    let line = format!("{} [{:<9}] {}", timestamp(), category, message);
    {
        let mut f = l.file.lock().unwrap();
        let _ = writeln!(f, "{line}");
        let _ = f.flush();
    }
    {
        let mut r = l.ring.lock().unwrap();
        r.push_back(line);
        if r.len() > RING_CAP {
            r.pop_front();
        }
    }
}

/// Install a panic hook that writes `logs/crash-<ts>.log` (message, backtrace,
/// black-box log tail) before delegating to the default hook.
pub fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_string();
        let backtrace = std::backtrace::Backtrace::force_capture();
        let msg = format!("panic on thread '{thread}': {info}");
        log("crash", &msg);

        if let Some(l) = LOG.get() {
            let ts = chrono::Local::now().format("%Y%m%d-%H%M%S").to_string();
            let path = l.dir.join(format!("crash-{ts}.log"));
            let mut body = format!("{msg}\n\nbacktrace:\n{backtrace}\n\nlast log lines:\n");
            for line in l.ring.lock().unwrap().iter() {
                body.push_str(line);
                body.push('\n');
            }
            if fs::write(&path, body).is_ok() {
                log("crash", &format!("crash report: {}", path.display()));
            }
        }
        prev(info);
    }));
}

/// Remove the session marker and stamp a clean shutdown.
pub fn mark_clean_exit() {
    log("sys", "==== session end (clean) ====");
    if let Some(l) = LOG.get() {
        let _ = fs::remove_file(l.dir.join("session.running"));
    }
}
