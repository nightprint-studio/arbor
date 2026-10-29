[package]
name = "{{name}}-app"
version.workspace = true
edition.workspace = true
{{#if license}}
license.workspace = true
{{/if}}

[dependencies]
{{name}}-core.workspace = true
