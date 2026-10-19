# arbor-exe-icon

The one line a backend's build script needs to stop shipping a blank executable.

```rust
// crates/products/corvus/be/build.rs
fn main() {
    arbor_exe_icon::prelude::embed();
}
```

`embed()` picks up `icon.ico` from the calling crate's root and writes it, plus the crate's
name, version and description, into the binary's Windows resource. On macOS and Linux it does
nothing and reports success — a `.ico` is a Win32 resource, and those platforms take an icon
from a bundle or a `.desktop` file instead.

## Why a crate

Seven products ship a headless backend (`corvus-be`, `sitta-be`, `merula-be`, `picus-be`,
`tyto-be`, `garrulus-be`, `bennu-be`), and each is an ordinary `[[bin]]`. On Windows that means
the generic executable glyph in Explorer, in the taskbar, in Task Manager, and in the prompts
the user is asked to recognise something in. Seven copies of the same resource-compiler
incantation is seven places to drift, and the eighth backend copies whichever it found first.

So the mechanism is here and the artwork stays with its product: each backend crate holds its
own `icon.ico`, beside the `Cargo.toml` that names it. A crate that moves takes its icon along.

## Where the icons come from

They are committed artefacts, re-encoded from `src-tauri/icons/products/<product>.png` — the
same artwork the launcher draws. Regenerate them with:

```
powershell -ExecutionPolicy Bypass -File scripts/gen-exe-icons.ps1
```

Each file holds 16/32/48 as 32-bit bitmaps and 256 as an embedded PNG: the sizes Windows
actually asks for. A lone 256 entry leaves it to downscale to 16px itself, which it does badly.

## What ends up in Properties → Details

`FileDescription` is the crate's own `description`, `FileVersion` and `ProductVersion` its
version, `ProductName` is Arbor, `CompanyName` the crate's authors, and `OriginalFilename` the
binary's name. Signed builds and SmartScreen read that block; an executable with nothing in it
is the one that looks like it came from nowhere.

## Failure is never fatal

A missing `icon.ico`, or a machine without the Windows SDK's `rc.exe`, produces a
`cargo:warning` and a plain binary. A backend that cannot be built because an artwork file was
not committed yet would be the worse trade.
