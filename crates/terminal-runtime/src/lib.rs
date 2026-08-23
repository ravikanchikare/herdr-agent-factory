//! Rust-owned PTY sessions with bounded, cursor-addressable output.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use base64::Engine;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const DEFAULT_OUTPUT_CAPACITY: usize = 1024 * 1024;
pub const MAX_READ_BYTES: usize = 256 * 1024;
pub const MAX_TERMINALS: usize = 32;

#[derive(Clone, Debug)]
pub struct CreateTerminal {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalCreated {
    pub terminal_id: Uuid,
    pub process_id: Option<u32>,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalRead {
    pub terminal_id: Uuid,
    pub data_base64: String,
    pub start_cursor: u64,
    pub next_cursor: u64,
    pub truncated: bool,
    pub reader_closed: bool,
    pub exit_status: Option<TerminalExit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TerminalExit {
    pub code: u32,
    pub signal: Option<String>,
}

pub struct TerminalManager {
    sessions: HashMap<Uuid, TerminalSession>,
    output_capacity: usize,
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new(DEFAULT_OUTPUT_CAPACITY)
    }
}

impl TerminalManager {
    pub fn new(output_capacity: usize) -> Self {
        assert!(output_capacity > 0, "output capacity must be positive");
        Self {
            sessions: HashMap::new(),
            output_capacity,
        }
    }

    pub fn create(&mut self, request: CreateTerminal) -> Result<TerminalCreated, TerminalError> {
        self.create_with_capacity(request, self.output_capacity)
    }

    pub fn create_with_capacity(
        &mut self,
        request: CreateTerminal,
        output_capacity: usize,
    ) -> Result<TerminalCreated, TerminalError> {
        if self.sessions.len() >= MAX_TERMINALS {
            return Err(TerminalError::CapacityReached);
        }
        if output_capacity == 0 || output_capacity > DEFAULT_OUTPUT_CAPACITY {
            return Err(TerminalError::InvalidOutputCapacity(output_capacity));
        }
        validate_size(request.cols, request.rows)?;
        if !request.executable.is_absolute() || !request.executable.is_file() {
            return Err(TerminalError::InvalidExecutable(request.executable));
        }
        if !request.cwd.is_absolute() || !request.cwd.is_dir() {
            return Err(TerminalError::InvalidWorkingDirectory(request.cwd));
        }

        let pair = native_pty_system()
            .openpty(PtySize {
                rows: request.rows,
                cols: request.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(TerminalError::Pty)?;
        let mut command = CommandBuilder::new(&request.executable);
        command.args(&request.args);
        command.cwd(&request.cwd);
        command.env_clear();
        for (name, value) in &request.environment {
            command.env(name, value);
        }
        if !request.environment.contains_key("TERM") {
            command.env("TERM", "xterm-256color");
        }

        let child = pair
            .slave
            .spawn_command(command)
            .map_err(TerminalError::Pty)?;
        drop(pair.slave);
        let process_id = child.process_id();
        #[cfg(unix)]
        let process_group = pair.master.process_group_leader();
        let mut reader = pair.master.try_clone_reader().map_err(TerminalError::Pty)?;
        let writer = pair.master.take_writer().map_err(TerminalError::Pty)?;
        let output = Arc::new(Mutex::new(OutputRing::new(output_capacity)));
        let reader_output = Arc::clone(&output);
        let reader_thread = std::thread::Builder::new()
            .name(format!("terminal-reader-{process_id:?}"))
            .spawn(move || {
                let mut buffer = [0_u8; 8192];
                loop {
                    match std::io::Read::read(&mut reader, &mut buffer) {
                        Ok(0) | Err(_) => {
                            reader_output
                                .lock()
                                .expect("terminal output lock poisoned")
                                .reader_closed = true;
                            break;
                        }
                        Ok(length) => reader_output
                            .lock()
                            .expect("terminal output lock poisoned")
                            .push(&buffer[..length]),
                    }
                }
            })
            .map_err(TerminalError::ReaderThread)?;

        let terminal_id = Uuid::new_v4();
        self.sessions.insert(
            terminal_id,
            TerminalSession {
                master: pair.master,
                writer,
                child,
                #[cfg(unix)]
                process_group,
                output,
                reader_thread: Some(reader_thread),
                exit_status: None,
            },
        );

        Ok(TerminalCreated {
            terminal_id,
            process_id,
            cols: request.cols,
            rows: request.rows,
        })
    }

    pub fn write(&mut self, terminal_id: Uuid, data: &[u8]) -> Result<(), TerminalError> {
        let session = self.session_mut(terminal_id)?;
        poll_exit(session)?;
        if session.exit_status.is_some() {
            return Err(TerminalError::Exited(terminal_id));
        }
        session.writer.write_all(data).map_err(TerminalError::Io)?;
        session.writer.flush().map_err(TerminalError::Io)
    }

    pub fn resize(&mut self, terminal_id: Uuid, cols: u16, rows: u16) -> Result<(), TerminalError> {
        validate_size(cols, rows)?;
        let session = self.session_mut(terminal_id)?;
        session
            .master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(TerminalError::Pty)
    }

    pub fn size(&self, terminal_id: Uuid) -> Result<PtySize, TerminalError> {
        self.sessions
            .get(&terminal_id)
            .ok_or(TerminalError::Unknown(terminal_id))?
            .master
            .get_size()
            .map_err(TerminalError::Pty)
    }

    pub fn read(
        &mut self,
        terminal_id: Uuid,
        cursor: u64,
        max_bytes: usize,
    ) -> Result<TerminalRead, TerminalError> {
        if max_bytes == 0 || max_bytes > MAX_READ_BYTES {
            return Err(TerminalError::InvalidReadLimit(max_bytes));
        }
        let session = self.session_mut(terminal_id)?;
        poll_exit(session)?;
        let output = session
            .output
            .lock()
            .expect("terminal output lock poisoned");
        let chunk = output.read(cursor, max_bytes);
        Ok(TerminalRead {
            terminal_id,
            data_base64: base64::engine::general_purpose::STANDARD.encode(chunk.data),
            start_cursor: chunk.start_cursor,
            next_cursor: chunk.next_cursor,
            truncated: chunk.truncated,
            reader_closed: output.reader_closed,
            exit_status: session.exit_status.clone(),
        })
    }

    pub fn kill(&mut self, terminal_id: Uuid) -> Result<TerminalExit, TerminalError> {
        let session = self.session_mut(terminal_id)?;
        poll_exit(session)?;
        if let Some(status) = &session.exit_status {
            return Ok(status.clone());
        }
        terminate(session, Duration::from_millis(500))?;
        Ok(session
            .exit_status
            .clone()
            .expect("terminate records exit status"))
    }

    pub fn status(&mut self, terminal_id: Uuid) -> Result<Option<TerminalExit>, TerminalError> {
        let session = self.session_mut(terminal_id)?;
        poll_exit(session)?;
        Ok(session.exit_status.clone())
    }

    pub fn release(&mut self, terminal_id: Uuid) -> Result<(), TerminalError> {
        let mut session = self
            .sessions
            .remove(&terminal_id)
            .ok_or(TerminalError::Unknown(terminal_id))?;
        terminate(&mut session, Duration::from_millis(500))?;
        drop(session.writer);
        drop(session.master);
        if let Some(thread) = session.reader_thread.take() {
            let _ = thread.join();
        }
        Ok(())
    }

    fn session_mut(&mut self, terminal_id: Uuid) -> Result<&mut TerminalSession, TerminalError> {
        self.sessions
            .get_mut(&terminal_id)
            .ok_or(TerminalError::Unknown(terminal_id))
    }
}

impl Drop for TerminalManager {
    fn drop(&mut self) {
        for session in self.sessions.values_mut() {
            let _ = terminate(session, Duration::from_millis(100));
        }
        for (_, mut session) in self.sessions.drain() {
            drop(session.writer);
            drop(session.master);
            if let Some(thread) = session.reader_thread.take() {
                let _ = thread.join();
            }
        }
    }
}

struct TerminalSession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
    #[cfg(unix)]
    process_group: Option<libc::pid_t>,
    output: Arc<Mutex<OutputRing>>,
    reader_thread: Option<JoinHandle<()>>,
    exit_status: Option<TerminalExit>,
}

fn poll_exit(session: &mut TerminalSession) -> Result<(), TerminalError> {
    if session.exit_status.is_none()
        && let Some(status) = session.child.try_wait().map_err(TerminalError::Io)?
    {
        session.exit_status = Some(TerminalExit {
            code: status.exit_code(),
            signal: status.signal().map(str::to_owned),
        });
    }
    Ok(())
}

fn terminate(session: &mut TerminalSession, timeout: Duration) -> Result<(), TerminalError> {
    poll_exit(session)?;
    if session.exit_status.is_some() {
        return Ok(());
    }

    #[cfg(unix)]
    let signalled_group = session.process_group.is_some_and(|group| {
        // SAFETY: the group id is reported by portable-pty for this PTY's
        // controlling process group.
        unsafe { libc::kill(-group, libc::SIGTERM) == 0 }
    });
    #[cfg(not(unix))]
    let signalled_group = false;
    if !signalled_group {
        session.child.kill().map_err(TerminalError::Io)?;
    }

    let deadline = Instant::now() + timeout;
    loop {
        poll_exit(session)?;
        if session.exit_status.is_some() {
            return Ok(());
        }
        if Instant::now() >= deadline {
            #[cfg(unix)]
            if let Some(group) = session.process_group {
                // SAFETY: same portable-pty process-group invariant as above.
                unsafe {
                    libc::kill(-group, libc::SIGKILL);
                }
            } else {
                session.child.kill().map_err(TerminalError::Io)?;
            }
            #[cfg(not(unix))]
            session.child.kill().map_err(TerminalError::Io)?;
            let status = session.child.wait().map_err(TerminalError::Io)?;
            session.exit_status = Some(TerminalExit {
                code: status.exit_code(),
                signal: status.signal().map(str::to_owned),
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn validate_size(cols: u16, rows: u16) -> Result<(), TerminalError> {
    if cols == 0 || rows == 0 || cols > 1000 || rows > 1000 {
        return Err(TerminalError::InvalidSize { cols, rows });
    }
    Ok(())
}

struct OutputRing {
    bytes: VecDeque<u8>,
    capacity: usize,
    start_cursor: u64,
    next_cursor: u64,
    reader_closed: bool,
}

impl OutputRing {
    fn new(capacity: usize) -> Self {
        Self {
            bytes: VecDeque::with_capacity(capacity),
            capacity,
            start_cursor: 0,
            next_cursor: 0,
            reader_closed: false,
        }
    }

    fn push(&mut self, bytes: &[u8]) {
        for byte in bytes {
            if self.bytes.len() == self.capacity {
                self.bytes.pop_front();
                self.start_cursor = self.start_cursor.saturating_add(1);
            }
            self.bytes.push_back(*byte);
            self.next_cursor = self.next_cursor.saturating_add(1);
        }
    }

    fn read(&self, cursor: u64, max_bytes: usize) -> OutputChunk {
        let truncated = cursor < self.start_cursor;
        let actual_cursor = cursor.clamp(self.start_cursor, self.next_cursor);
        let offset = actual_cursor.saturating_sub(self.start_cursor) as usize;
        let data = self
            .bytes
            .iter()
            .skip(offset)
            .take(max_bytes)
            .copied()
            .collect::<Vec<_>>();
        OutputChunk {
            start_cursor: actual_cursor,
            next_cursor: actual_cursor.saturating_add(data.len() as u64),
            truncated,
            data,
        }
    }
}

struct OutputChunk {
    start_cursor: u64,
    next_cursor: u64,
    truncated: bool,
    data: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum TerminalError {
    #[error("terminal capacity of {MAX_TERMINALS} has been reached")]
    CapacityReached,
    #[error("terminal executable is not an absolute regular file: {0}")]
    InvalidExecutable(PathBuf),
    #[error("terminal working directory is not an absolute directory: {0}")]
    InvalidWorkingDirectory(PathBuf),
    #[error("terminal size {cols}x{rows} is outside 1..=1000")]
    InvalidSize { cols: u16, rows: u16 },
    #[error("terminal read limit {0} is outside 1..={MAX_READ_BYTES}")]
    InvalidReadLimit(usize),
    #[error("terminal output capacity {0} is outside 1..={DEFAULT_OUTPUT_CAPACITY}")]
    InvalidOutputCapacity(usize),
    #[error("unknown terminal {0}")]
    Unknown(Uuid),
    #[error("terminal {0} has already exited")]
    Exited(Uuid),
    #[error("PTY operation failed: {0}")]
    Pty(anyhow::Error),
    #[error("terminal I/O failed: {0}")]
    Io(std::io::Error),
    #[error("failed to start terminal reader thread: {0}")]
    ReaderThread(std::io::Error),
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn create(executable: &str, args: &[&str], capacity: usize) -> (TerminalManager, Uuid) {
        let mut manager = TerminalManager::new(capacity);
        let created = manager
            .create(CreateTerminal {
                executable: PathBuf::from(executable),
                args: args.iter().map(|value| (*value).into()).collect(),
                cwd: std::env::temp_dir(),
                environment: BTreeMap::new(),
                cols: 80,
                rows: 24,
            })
            .unwrap();
        (manager, created.terminal_id)
    }

    fn wait_for_output(manager: &mut TerminalManager, id: Uuid) -> TerminalRead {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let read = manager.read(id, 0, MAX_READ_BYTES).unwrap();
            if !read.data_base64.is_empty() || read.exit_status.is_some() {
                return read;
            }
            assert!(Instant::now() < deadline, "terminal produced no output");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn preserves_unicode_output() {
        let (mut manager, id) = create("/usr/bin/printf", &["hello 🌍"], 1024);
        let read = wait_for_output(&mut manager, id);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(read.data_base64)
            .unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), "hello 🌍");
    }

    #[test]
    fn resizes_the_pty() {
        let (mut manager, id) = create("/bin/cat", &[], 1024);
        manager.resize(id, 120, 40).unwrap();
        let size = manager.size(id).unwrap();
        assert_eq!((size.cols, size.rows), (120, 40));
        manager.kill(id).unwrap();
    }

    #[test]
    fn writes_to_the_pty_and_reports_exit() {
        let (mut manager, id) = create("/bin/cat", &[], 1024);
        manager.write(id, "typed 🌍\n".as_bytes()).unwrap();
        let read = wait_for_output(&mut manager, id);
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(read.data_base64)
            .unwrap();
        assert!(String::from_utf8(bytes).unwrap().contains("typed 🌍"));
        manager.kill(id).unwrap();
    }

    #[test]
    fn captures_a_nonzero_exit_status() {
        let (mut manager, id) = create("/usr/bin/false", &[], 1024);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let read = manager.read(id, 0, 1024).unwrap();
            if let Some(status) = read.exit_status {
                assert_ne!(status.code, 0);
                break;
            }
            assert!(Instant::now() < deadline, "terminal did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn kills_a_running_process() {
        let (mut manager, id) = create("/bin/sleep", &["10"], 1024);
        let status = manager.kill(id).unwrap();
        assert_ne!(status.code, 0);
        assert!(manager.read(id, 0, 1024).unwrap().exit_status.is_some());
    }

    #[test]
    fn release_cleans_up_the_terminal() {
        let (mut manager, id) = create("/bin/sleep", &["10"], 1024);
        manager.release(id).unwrap();
        assert!(matches!(manager.status(id), Err(TerminalError::Unknown(_))));
    }

    #[test]
    fn bounded_ring_reports_lost_output() {
        let (mut manager, id) = create("/usr/bin/yes", &["output"], 128);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let read = manager.read(id, 0, 128).unwrap();
            if read.truncated {
                assert_eq!(
                    base64::engine::general_purpose::STANDARD
                        .decode(read.data_base64)
                        .unwrap()
                        .len(),
                    128
                );
                break;
            }
            assert!(Instant::now() < deadline, "output ring did not fill");
            std::thread::sleep(Duration::from_millis(10));
        }
        manager.kill(id).unwrap();
    }

    #[test]
    fn output_ring_cursor_replay_is_deterministic() {
        let mut ring = OutputRing::new(5);
        ring.push(b"abcdef");
        let first = ring.read(0, 2);
        assert!(first.truncated);
        assert_eq!(first.start_cursor, 1);
        assert_eq!(first.data, b"bc");
        let second = ring.read(first.next_cursor, 10);
        assert_eq!(second.data, b"def");
    }

    #[test]
    fn executable_must_be_absolute() {
        let mut manager = TerminalManager::default();
        let result = manager.create(CreateTerminal {
            executable: Path::new("sh").into(),
            args: vec![],
            cwd: std::env::temp_dir(),
            environment: BTreeMap::new(),
            cols: 80,
            rows: 24,
        });
        assert!(matches!(result, Err(TerminalError::InvalidExecutable(_))));
    }
}
