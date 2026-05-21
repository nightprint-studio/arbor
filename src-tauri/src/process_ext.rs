/// Extension trait that suppresses the console window popup on Windows.
/// On non-Windows platforms this is a no-op.
///
/// Call `.no_window()` on any `std::process::Command` before `.spawn()` or
/// `.output()` to prevent a visible CMD/shell window from appearing when the
/// process is created from within the Tauri GUI process.
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
