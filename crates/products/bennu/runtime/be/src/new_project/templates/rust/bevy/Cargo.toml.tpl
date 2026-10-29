[package]
name = "{{name}}"
version = "0.1.0"
edition = "{{edition}}"
{{#if license}}
license = "{{license}}"
{{/if}}

[dependencies]
bevy = "{{bevy_version}}"
{{#if ron}}
serde = { version = "1", features = ["derive"] }
ron = "0.8"
{{/if}}
{{#if dynamic}}

[features]
# `cargo run --features dev` links Bevy dynamically for much faster rebuilds.
# Never ship a build made with it.
dev = ["bevy/dynamic_linking"]
{{/if}}

# Your own code compiles lightly, dependencies (Bevy) heavily: the usual compromise
# between rebuild time and frame rate while developing.
[profile.dev]
opt-level = 1

[profile.dev.package."*"]
opt-level = 3

[profile.release]
codegen-units = 1
lto = "thin"
