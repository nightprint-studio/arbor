//! The platform's process oddities, in one place.
//!
//! Two of them so far, and they have nothing in common except that every caller meets them and each
//! one works them out again if they are not here: suppressing Windows' console-window popup, and
//! finding an executable when the app's `PATH` is not the shell's ([`locate`]).
//!
//! `NoWindowExt` — extension trait that suppresses the console window popup on
//! Windows. On non-Windows platforms this is a no-op.
//!
//! Call `.no_window()` on any `std::process::Command` before `.spawn()` or
//! `.output()` to prevent a visible CMD/shell window from appearing when the
//! process is created from within a GUI process (e.g. Tauri WebView).

pub mod locate;

pub mod prelude;

pub trait NoWindowExt {
    fn no_window(&mut self) -> &mut Self;
}

impl NoWindowExt for std::process::Command {
    fn no_window(&mut self) -> &mut Self {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            self.creation_flags(CREATE_NO_WINDOW);
        }
        self
    }
}
