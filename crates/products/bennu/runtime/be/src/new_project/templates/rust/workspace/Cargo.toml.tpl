[workspace]
resolver = "{{resolver}}"
members = ["crates/*"]

[workspace.package]
version = "0.1.0"
edition = "{{edition}}"
{{#if license}}
license = "{{license}}"
{{/if}}

[workspace.dependencies]
{{name}}-core = { path = "crates/{{name}}-core" }
