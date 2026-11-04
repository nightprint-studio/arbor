# arbor-bevy-remote-derive

`#[derive(RemoteInspect)]` for [`arbor-bevy-remote`](../bevy-remote): field attributes that tell Arbor's Bevy Remote
inspector to draw a colour picker, a slider, a date or an angle instead of a bare number.

You do not depend on this crate directly: `arbor-bevy-remote` re-exports the derive in its prelude, next to the
`RemoteInspect` trait it implements (the same split as `serde`'s `Serialize`).

```rust
use arbor_bevy_remote::prelude::*;
use bevy::prelude::*;

#[derive(Component, Reflect, RemoteInspect)]
struct Lamp {
    #[inspect(color)]
    tint: [f32; 3],
    #[inspect(range = 0.0..=1.0, step = 0.01)]
    intensity: f32,
    #[inspect(angle)]
    beam: f32,
    #[inspect(date)]
    installed: f64,
    #[inspect(label = "speed", unit = "m/s", readonly)]
    measured: f32,
}

fn main() {
    App::new()
        // … DefaultPlugins, with_arbor_methods(RemotePlugin::default()), ArborRemotePlugin::default() …
        .register_inspect::<Lamp>()
        .run();
}
```

## Options

| Key | Meaning |
|---|---|
| `color` | a colour: hex string, `[r, g, b(, a)]` or a `{red, green, blue}` struct — swatch, picker, hex field |
| `range = a..=b` | a slider over `a..=b` (write it inclusive) |
| `step = x` | the step of the slider and of drag-to-scrub |
| `date` | seconds since the Unix epoch, drawn and edited as a date |
| `angle` | radians, drawn and edited in degrees |
| `readonly` | shown, not editable |
| `label = "…"` | the name shown instead of the field's |
| `unit = "…"` | a suffix, such as `m/s` |
| `description = "…"` | a sentence shown under the label and on hover; defaults to the field's `///` doc comment |

`color`, `date`, `angle` and `range` each say how to *draw* the field, so a field takes at most one; the rest combine.
A `///` doc comment on a field is its default description (`description = "…"` wins), and one on the struct — or `#[inspect(description = "…")]` there, the only option a struct takes — describes the type. Options can be spread over several `#[inspect(...)]` attributes. Only structs are supported; a field's hints are
keyed by its name (`speed`) or, in a tuple struct, its index (`0`). Numbers in `range` and `step` are ordinary
expressions, so `range = 0.0..=MAX_HP as f64` works.

## Layout

| File | Does |
|---|---|
| `src/parse.rs` | `#[inspect(...)]` → a plain model, with errors that say what was expected |
| `src/expand.rs` | the model → the `impl` (text-tested) |
| `src/lib.rs` | the `proc_macro_derive` entry point |

Both modules are pure `proc_macro2`/`syn`, so `cargo test` covers parsing and expansion without a compiler plugin.
A proc-macro crate can only export macros, so there is no `prelude` here: the companion crate's prelude is the
public entry point.

## Dependencies

`syn` 2 (`full`), `quote` 1 and `proc-macro2` 1 — the ones Arbor's own `arbor-rpc-macros` already uses.

## Status

Not compiled yet (written without running `cargo`); the unit tests are the first thing to run.
