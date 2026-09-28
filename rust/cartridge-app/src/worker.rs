//! Bounded, cancellation-aware worker transport shared by both interfaces.
use crate::{Error, Result};
use cartridge_core::service::Request;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{
        mpsc::{self, Receiver, SyncSender},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
};

const LINE_LIMIT: u64 = 1024 * 1024;

#[derive(Debug, Clone)]
pub enum Event {
    Directory(PathBuf),
    Progress(String),
    Finished(Result<Value>),
}

pub fn binary(name: &str) -> Result<PathBuf> {
    let variable = match name {
        "cartridge-worker" => "CARTRIDGE_STUDIO_WORKER",
        "cartridge" => "CARTRIDGE_STUDIO_CLI",
        _ => "CARTRIDGE_STUDIO_FRONTEND",
    };
    if let Some(path) = std::env::var_os(variable) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Ok(path);
        }
        return Err(Error::new(
            "PROGRAM_NOT_FOUND",
            format!("{variable} points to a missing program."),
            "Correct the environment override or remove it, then restart Cartridge Studio.",
        ));
    }
    let path = std::env::current_exe()?.with_file_name(name);
    if path.is_file() {
        return Ok(path);
    }
    #[cfg(target_os = "macos")]
    {
        let installed = if name == "cartridge-worker" {
            PathBuf::from("/usr/local/libexec/cartridge-studio/cartridge-worker")
        } else {
            PathBuf::from("/usr/local/bin").join(name)
        };
        if installed.is_file() {
            return Ok(installed);
        }
    }
    let (code, action) = match name {
        "cartridge-tui" => ("TUI_NOT_INSTALLED", missing_component_action("TUI")),
        "cartridge" => ("CLI_NOT_INSTALLED", missing_component_action("CLI")),
        _ => (
            "PROGRAM_NOT_FOUND",
            "Reinstall your selected Cartridge Studio interfaces to restore the shared cartridge engine. Keep their executables together."
                .into(),
        ),
    };
    Err(Error::new(
        code,
        format!("The application is missing {name}."),
        &action,
    ))
}

#[cfg(target_os = "linux")]
fn missing_component_action(component: &str) -> String {
    format!("Rerun the installer and include {component} in your selection, or install the matching cartridge-studio package for your distribution.")
}
#[cfg(target_os = "macos")]
fn missing_component_action(component: &str) -> String {
    format!("Rerun Cartridge Studio.pkg and include {component} in your component selection.")
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn missing_component_action(component: &str) -> String {
    format!("Rerun the installer and include {component} in your component selection.")
}

pub struct Job {
    child: Arc<Mutex<Child>>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
    pub stopping: bool,
}

impl Job {
    pub fn start(request: &Request) -> Result<Self> {
        Self::with_command(Command::new(binary("cartridge-worker")?), request)
    }
    pub fn with_command(mut command: Command, request: &Request) -> Result<Self> {
        let encoded = serde_json::to_vec(request)?;
        let mut child = command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| {
                Error::new(
                    "WORKER_START_FAILED",
                    "The cartridge service could not start.",
                    "Reinstall the application, or check executable permissions.",
                )
                .details(json!({"reason":e.to_string()}))
            })?;
        let mut input = child.stdin.take().unwrap();
        if let Err(e) = input
            .write_all(&encoded)
            .and_then(|_| input.write_all(b"\n"))
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e.into());
        }
        drop(input);
        let output = child.stdout.take().unwrap();
        let errors = child.stderr.take().unwrap();
        let child = Arc::new(Mutex::new(child));
        let owned = child.clone();
        let (send, events) = mpsc::sync_channel(128);
        let thread = thread::spawn(move || {
            let stderr = thread::spawn(move || {
                let mut tail = VecDeque::with_capacity(65536);
                let mut reader = errors;
                let mut bytes = [0; 4096];
                while let Ok(n) = reader.read(&mut bytes) {
                    if n == 0 {
                        break;
                    }
                    tail.extend(&bytes[..n]);
                    while tail.len() > 65536 {
                        tail.pop_front();
                    }
                }
                String::from_utf8_lossy(&tail.into_iter().collect::<Vec<_>>()).into_owned()
            });
            let parsed = receive(BufReader::new(output), &send);
            // Malformed/oversized output must never leave a programming worker orphaned.
            if parsed.is_err() {
                signal(&owned);
            }
            let status = loop {
                let result = owned.lock().unwrap().try_wait();
                match result {
                    Ok(Some(status)) => break Ok(status),
                    Err(error) => break Err(error),
                    Ok(None) => thread::sleep(std::time::Duration::from_millis(10)),
                }
            };
            let stderr = stderr.join().unwrap_or_default();
            let result = match (parsed, status) {
                (Ok(result), Ok(status)) if status.success() || result.is_err() => result,
                (Err(error), _) => Err(error),
                (_, status) => Err(Error::new("WORKER_EXITED", "The cartridge service stopped before completing successfully.", "Keep the operation folder. If erase or writing had started, restore the retained source ROM after resolving the error.").details(json!({"status":format!("{status:?}"),"stderr":stderr}))),
            };
            // Success is observable only after exit and USB cleanup, never on a result line alone.
            let _ = send.send(Event::Finished(result));
        });
        Ok(Self {
            child,
            events,
            thread: Some(thread),
            stopping: false,
        })
    }
    pub fn poll(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
    pub fn stop(&mut self) {
        self.stopping = true;
        signal(&self.child);
    }
}

fn signal(child: &Arc<Mutex<Child>>) {
    if let Ok(mut child) = child.lock() {
        if matches!(child.try_wait(), Ok(None)) {
            // Cartridge Studio's worker handles SIGTERM cooperatively and journals before releasing USB.
            unsafe {
                libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        signal(&self.child);
        // Drain the bounded channel while joining so a full progress queue cannot deadlock cleanup.
        if let Some(handle) = self.thread.take() {
            while !handle.is_finished() {
                let _ = self
                    .events
                    .recv_timeout(std::time::Duration::from_millis(20));
            }
            let _ = handle.join();
        }
    }
}

fn receive(mut output: impl BufRead, send: &SyncSender<Event>) -> Result<Result<Value>> {
    let mut terminal = None;
    loop {
        let mut line = Vec::new();
        let n = output
            .by_ref()
            .take(LINE_LIMIT + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            break;
        }
        if line.len() as u64 > LINE_LIMIT {
            return Err(protocol("An operation event exceeded the size limit."));
        }
        let value: Value = serde_json::from_slice(&line)
            .map_err(|_| protocol("The cartridge service sent an invalid event."))?;
        if terminal.is_some() {
            return Err(protocol(
                "The cartridge service sent data after its final result.",
            ));
        }
        match value["event"].as_str() {
            Some("directory") => {
                let path = value["path"]
                    .as_str()
                    .ok_or_else(|| protocol("The operation folder was missing."))?;
                let _ = send.send(Event::Directory(path.into()));
            }
            Some("progress") => {
                let text = value["message"]
                    .as_str()
                    .ok_or_else(|| protocol("Progress text was missing."))?;
                let _ = send.try_send(Event::Progress(text.into()));
            }
            Some("result") if value.get("result").is_some() => {
                terminal = Some(Ok(value["result"].clone()))
            }
            Some("error") => {
                terminal = Some(Err(Error::new(
                    value["error"].as_str().unwrap_or("WORKER_ERROR"),
                    value["message"].as_str().unwrap_or("The operation failed."),
                    value["action"].as_str().unwrap_or(
                        "Keep the operation folder and retry after checking the reader.",
                    ),
                )
                .details(value["details"].clone())))
            }
            _ => return Err(protocol("The cartridge service sent an unknown event.")),
        }
    }
    terminal.ok_or_else(|| protocol("The cartridge service exited without a final result."))
}
fn protocol(message: &str) -> Error {
    Error::new("WORKER_PROTOCOL", message, "Reinstall matching application and worker versions. Retain the operation folder; a partial write may need restoring.")
}
