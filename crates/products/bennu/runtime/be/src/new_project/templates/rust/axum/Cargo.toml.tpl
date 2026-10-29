[package]
name = "{{name}}"
version = "0.1.0"
edition = "{{edition}}"
{{#if license}}
license = "{{license}}"
{{/if}}

[dependencies]
axum = "0.8"
tokio = { version = "1", features = ["full"] }
