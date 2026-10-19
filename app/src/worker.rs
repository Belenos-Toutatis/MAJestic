//! Elevated helper.
//!
//! The window creates a named pipe with a random name, starts
//! `MAJestic.exe --worker <pipe> <window pid>` through UAC, and talks to it
//! with JSON lines. Both ends check who is on the other side (pipe client /
//! server process id), so no other process can feed the elevated helper.
//! The helper only accepts `JobRequest`s, whose commands are built from fixed
//! templates with validated ids.

use std::fs::File;
use std::hash::{BuildHasher, Hasher};
use std::io::{BufRead, BufReader, Write};
use std::os::windows::io::{AsRawHandle, FromRawHandle};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use majestic_core::jobs;
use majestic_core::model::{FailureKind, JobEvent, JobRequest, JobState};
use majestic_core::system::elevation::{self, ElevatedProcess, ElevationError};
use serde::{Deserialize, Serialize};
use windows::core::HSTRING;
use windows::Win32::Foundation::{ERROR_PIPE_CONNECTED, HANDLE};
use windows::Win32::Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_DUPLEX};
use windows::Win32::System::Pipes::{
    ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId, GetNamedPipeServerProcessId,
    PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT,
};

const PIPE_PREFIX: &str = r"\\.\pipe\majestic-";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ToWorker {
    Run { job: JobRequest },
    Quit,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum FromWorker {
    Event { event: JobEvent },
    Done { state: JobState },
}

pub enum StartError {
    Refused,
    Other(String),
}

fn send<T: Serialize>(w: &mut File, msg: &T) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg).map_err(std::io::Error::other)?;
    line.push('\n');
    w.write_all(line.as_bytes())?;
    w.flush()
}

/// Window side of the channel.
pub struct Worker {
    reader: BufReader<File>,
    writer: File,
    process: ElevatedProcess,
    alive: bool,
}

impl Worker {
    /// Starts the elevated helper (shows the UAC prompt) and connects to it.
    pub fn start() -> Result<Worker, StartError> {
        let exe = std::env::current_exe().map_err(|e| StartError::Other(e.to_string()))?;
        let random = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let name = format!("{PIPE_PREFIX}{}-{random:016x}", std::process::id());

        // SAFETY: plain FFI call with a valid, NUL-terminated name.
        let pipe = unsafe {
            CreateNamedPipeW(
                &HSTRING::from(name.as_str()),
                PIPE_ACCESS_DUPLEX | FILE_FLAG_FIRST_PIPE_INSTANCE,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                1,
                64 * 1024,
                64 * 1024,
                0,
                None,
            )
        };
        if pipe.is_invalid() {
            return Err(StartError::Other(format!("cannot create pipe: {}", std::io::Error::last_os_error())));
        }
        // From here the File owns the handle and closes it on every path.
        // SAFETY: `pipe` is a valid handle we own.
        let file = unsafe { File::from_raw_handle(pipe.0 as _) };

        let args = format!("--worker {name} {}", std::process::id());
        let process = elevation::spawn_elevated(&exe, &args).map_err(|e| match e {
            ElevationError::Refused => StartError::Refused,
            ElevationError::Other(e) => StartError::Other(e),
        })?;

        // ConnectNamedPipe blocks: run it on a thread, watch the helper meanwhile.
        let raw = file.as_raw_handle() as isize;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            // SAFETY: the handle stays open (owned by `file`) until this
            // call returns: on the failure path we connect a dummy client to
            // release it before dropping `file`.
            let r = unsafe { ConnectNamedPipe(HANDLE(raw as _), None) };
            let ok = match r {
                Ok(()) => true,
                Err(e) => e.code() == ERROR_PIPE_CONNECTED.to_hresult(),
            };
            let _ = tx.send(ok);
        });

        let started = Instant::now();
        let connected = loop {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(ok) => break ok,
                Err(mpsc::RecvTimeoutError::Disconnected) => break false,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !process.is_running() || started.elapsed() > CONNECT_TIMEOUT {
                        // Unblock the connecting thread with a dummy client.
                        let _ = std::fs::OpenOptions::new().read(true).write(true).open(&name);
                        let _ = rx.recv_timeout(Duration::from_secs(2));
                        return Err(StartError::Other("the elevated helper did not connect".into()));
                    }
                }
            }
        };
        if !connected {
            return Err(StartError::Other("pipe connection failed".into()));
        }

        let mut client_pid = 0u32;
        // SAFETY: valid pipe handle, valid out pointer.
        let ok = unsafe { GetNamedPipeClientProcessId(HANDLE(raw as _), &mut client_pid) }.is_ok();
        if !ok || client_pid != process.pid {
            return Err(StartError::Other(format!("unexpected pipe client {client_pid}")));
        }

        let writer = file.try_clone().map_err(|e| StartError::Other(e.to_string()))?;
        log::info!("elevated helper connected (pid {})", process.pid);
        Ok(Worker { reader: BufReader::new(file), writer, process, alive: true })
    }

    pub fn is_alive(&self) -> bool {
        self.alive
    }

    /// Runs one job in the helper, forwarding its events.
    pub fn run(&mut self, job: &JobRequest, on_event: &mut dyn FnMut(JobEvent)) -> JobState {
        let lost = |detail: String| JobState::Failed {
            code: None,
            kind: FailureKind::ElevationRequired,
            log_excerpt: format!("The elevated helper stopped: {detail}"),
        };
        if let Err(e) = send(&mut self.writer, &ToWorker::Run { job: job.clone() }) {
            self.alive = false;
            return lost(e.to_string());
        }
        let mut line = String::new();
        loop {
            line.clear();
            match self.reader.read_line(&mut line) {
                Ok(0) => {
                    self.alive = false;
                    return lost("connection closed".into());
                }
                Ok(_) => match serde_json::from_str::<FromWorker>(line.trim()) {
                    Ok(FromWorker::Event { event }) => on_event(event),
                    Ok(FromWorker::Done { state }) => return state,
                    Err(e) => log::warn!("worker: bad message: {e}"),
                },
                Err(e) => {
                    self.alive = false;
                    return lost(e.to_string());
                }
            }
        }
    }

    pub fn quit(mut self) {
        let _ = send(&mut self.writer, &ToWorker::Quit);
        let _ = self.process.wait(Duration::from_secs(5));
    }
}

/// Helper side: `--worker <pipe> <parent pid>`. Returns the exit code.
pub fn run_worker(args: &[String]) -> i32 {
    let (Some(name), Some(parent)) = (args.first(), args.get(1).and_then(|p| p.parse::<u32>().ok())) else {
        log::error!("worker: bad arguments");
        return 2;
    };
    if !name.starts_with(PIPE_PREFIX) {
        log::error!("worker: bad pipe name");
        return 2;
    }
    log::info!("worker started (elevated: {})", elevation::is_elevated());

    let mut file = None;
    for _ in 0..40 {
        match std::fs::OpenOptions::new().read(true).write(true).open(name) {
            Ok(f) => {
                file = Some(f);
                break;
            }
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
    let Some(file) = file else {
        log::error!("worker: cannot open pipe");
        return 3;
    };

    let mut server_pid = 0u32;
    // SAFETY: valid pipe handle, valid out pointer.
    let ok = unsafe { GetNamedPipeServerProcessId(HANDLE(file.as_raw_handle() as _), &mut server_pid) }.is_ok();
    if !ok || server_pid != parent {
        log::error!("worker: unexpected pipe server {server_pid} (expected {parent})");
        return 4;
    }

    let Ok(mut writer) = file.try_clone() else { return 5 };
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        match serde_json::from_str::<ToWorker>(line.trim()) {
            Ok(ToWorker::Run { job }) => {
                let mut emit = |event: JobEvent| {
                    let _ = send(&mut writer, &FromWorker::Event { event });
                };
                let state = jobs::run_job(&job, &mut emit);
                if send(&mut writer, &FromWorker::Done { state }).is_err() {
                    break;
                }
            }
            Ok(ToWorker::Quit) => break,
            Err(e) => log::warn!("worker: bad message: {e}"),
        }
    }
    log::info!("worker exiting");
    0
}
