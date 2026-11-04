// Give the executable its icon and its Windows file properties. The icon is `icon.ico`
// beside this file; regenerate it with `scripts/gen-exe-icons.ps1`.
//
// A no-op on every target that is not Windows, so there is no `cfg` to keep in step here.
fn main() {
    arbor_exe_icon::prelude::embed();
}
