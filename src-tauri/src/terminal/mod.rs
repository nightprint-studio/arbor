use std::collections::HashMap;
use std::io::{Read, Write};
use base64::{engine::general_purpose, Engine};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde::{Deserialize, Serialize};
use tauri::Emitter;

use crate::error::{AppError, Result};
use crate::process_ext::NoWindowExt;

pub mod registry;
pub use registry::{
    DetectedShell, BUILTIN_SHELLS,
    detect_available_shells, platform_default, resolve_shell,
};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalInfo {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub title: String,
    pub cols: u16,
    pub rows: u16,
}

// ---------------------------------------------------------------------------
// Internal instance
// ---------------------------------------------------------------------------

struct TerminalInstance {
    pub id: String,
    pub shell: String,
    pub cwd: String,
    pub cols: u16,
    pub rows: u16,
    /// Writer to the PTY master (sends data to the shell).
    pub writer: Box<dyn Write + Send>,
    /// The PTY master itself — kept alive for resize operations.
    pub master: Box<dyn portable_pty::MasterPty>,
}

// Safety: TerminalInstance is always accessed under Mutex<TerminalManager>.
// No two threads ever touch the same instance concurrently.
// The concrete MasterPty implementations (ConPTY on Windows, UnixPty on Unix)
// are internally Send — only the trait object declaration omits the bound.
unsafe impl Send for TerminalInstance {}

// ---------------------------------------------------------------------------
// Manager
// ---------------------------------------------------------------------------

pub struct TerminalManager {
    terminals: HashMap<String, TerminalInstance>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self { terminals: HashMap::new() }
    }

    /// Spawn a new PTY process and return its unique ID.
    /// Output is streamed via Tauri events: `terminal:output:<id>` (base64 payload).
    /// A `terminal:closed:<id>` event is emitted when the process exits.
    pub fn create(
        &mut self,
        shell: String,
        args: Vec<String>,
        display_name: String,
        cwd: String,
        cols: u16,
        rows: u16,
        app_handle: tauri::AppHandle,
    ) -> Result<TerminalInfo> {
        let id = uuid::Uuid::new_v4().to_string();

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| AppError::Other(format!("PTY open failed: {e}")))?;

        let mut cmd = CommandBuilder::new(&shell);
        for a in &args { cmd.arg(a); }
        cmd.cwd(&cwd);
        // Ensure proper ANSI colour support
        cmd.env("TERM", "xterm-256color");
        #[cfg(not(target_os = "windows"))]
        cmd.env("COLORTERM", "truecolor");
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| AppError::Other(format!("spawn failed: {e}")))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| AppError::Other(format!("take_writer failed: {e}")))?;

        let mut reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| AppError::Other(format!("clone_reader failed: {e}")))?;

        // Spawn a background thread to relay PTY output → Tauri events.
        let id_clone = id.clone();
        let app_handle_reader = app_handle.clone();
        std::thread::Builder::new()
            .name(format!("arbor-pty-reader-{id}"))
            .spawn(move || {
                let mut buf = [0u8; 4096];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            let encoded = general_purpose::STANDARD.encode(&buf[..n]);
                            let _ = app_handle_reader.emit(
                                &format!("terminal:output:{}", id_clone),
                                encoded,
                            );
                        }
                    }
                }
                // Reader EOF also signals close (Unix / non-ConPTY paths).
                let _ = app_handle_reader.emit(
                    &format!("terminal:closed:{}", id_clone),
                    (),
                );
            })
            .map_err(|e| AppError::Other(format!("thread spawn failed: {e}")))?;

        // Spawn a child-watcher thread that blocks on wait() and emits
        // terminal:closed when the process actually exits.  This is the
        // *reliable* path on Windows/ConPTY where the PTY reader may never
        // see EOF even after the shell process terminates.
        let id_watcher = id.clone();
        std::thread::Builder::new()
            .name(format!("arbor-pty-watcher-{id}"))
            .spawn(move || {
                let _ = child.wait();
                let _ = app_handle.emit(
                    &format!("terminal:closed:{}", id_watcher),
                    (),
                );
            })
            .map_err(|e| AppError::Other(format!("watcher thread spawn failed: {e}")))?;

        let info = TerminalInfo {
            id: id.clone(),
            shell: display_name.clone(),
            cwd: cwd.clone(),
            title: display_name.clone(),
            cols,
            rows,
        };

        self.terminals.insert(id, TerminalInstance {
            id:     info.id.clone(),
            shell:  display_name,
            cwd,
            cols,
            rows,
            writer,
            master: pair.master,
        });

        Ok(info)
    }

    /// Write raw input bytes to the PTY (keystrokes from xterm.js).
    pub fn write(&mut self, id: &str, data: &[u8]) -> Result<()> {
        let term = self
            .terminals
            .get_mut(id)
            .ok_or_else(|| AppError::Other(format!("terminal '{id}' not found")))?;
        term.writer
            .write_all(data)
            .map_err(|e| AppError::Other(format!("write failed: {e}")))
    }

    /// Resize the PTY window.
    pub fn resize(&mut self, id: &str, cols: u16, rows: u16) -> Result<()> {
        let term = self
            .terminals
            .get_mut(id)
            .ok_or_else(|| AppError::Other(format!("terminal '{id}' not found")))?;
        term.master
            .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| AppError::Other(format!("resize failed: {e}")))?;
        term.cols = cols;
        term.rows = rows;
        Ok(())
    }

    /// Update the terminal title (cosmetic only).
    pub fn set_title(&mut self, id: &str, title: String) -> Result<()> {
        // Title is stored in the frontend store only; nothing to do here at the
        // moment.  Kept as a hook for future OSC 2 title-change sequences.
        let _ = (id, title);
        Ok(())
    }

    /// Close a terminal — drops the PTY master (sends SIGHUP/kills the child).
    pub fn close(&mut self, id: &str) -> Result<()> {
        self.terminals
            .remove(id)
            .ok_or_else(|| AppError::Other(format!("terminal '{id}' not found")))?;
        Ok(())
    }

    /// List all currently open terminals.
    pub fn list(&self) -> Vec<TerminalInfo> {
        self.terminals
            .values()
            .map(|t| TerminalInfo {
                id:    t.id.clone(),
                shell: t.shell.clone(),
                cwd:   t.cwd.clone(),
                title: t.shell.clone(),
                cols:  t.cols,
                rows:  t.rows,
            })
            .collect()
    }

    /// Execute a command in a throwaway process and capture its output.
    /// Used by the plugin `arbor.terminal.exec` API.
    /// Returns `(exit_code, stdout, stderr)`.
    pub fn exec_command(
        command: &str,
        cwd: Option<&str>,
    ) -> Result<(i32, String, String)> {
        let mut parts = command.split_whitespace();
        let prog = parts
            .next()
            .ok_or_else(|| AppError::Other("empty command".to_string()))?;

        let mut cmd = std::process::Command::new(prog);
        cmd.args(parts);
        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        }

        let output = cmd
            .no_window()
            .output()
            .map_err(|e| AppError::Other(format!("exec failed: {e}")))?;

        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok((exit_code, stdout, stderr))
    }
}
