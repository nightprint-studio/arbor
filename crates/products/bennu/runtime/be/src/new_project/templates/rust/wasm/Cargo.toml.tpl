[package]
name = "{{name}}"
version = "0.1.0"
edition = "{{edition}}"
{{#if license}}
license = "{{license}}"
{{/if}}

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
wasm-bindgen = "0.2"

[profile.release]
opt-level = "s"
lto = true
