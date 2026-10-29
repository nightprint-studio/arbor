[package]
name = "{{name}}"
version = "0.1.0"
edition = "{{edition}}"
{{#if license}}
license = "{{license}}"
{{/if}}

[dependencies]
clap = { version = "4", features = ["derive"] }
