//! The generators, one module each. A module holds the generator's `Options`, its defaults and its
//! `generate`, plus the tests of its output.

pub mod bevy_skeleton;
pub mod ratatui_app;
pub mod ratatui_dialog;
pub mod ratatui_keymap;
pub mod ratatui_list;
pub mod ratatui_widget;
pub mod rust_http_client;
pub mod rust_support;
pub mod soap_call;
pub mod builder;
pub mod cxf_rest;
pub mod converter;
pub mod dto_mapper;
pub mod http_client;
pub mod jackson;
pub mod security;
pub mod tauri_command;
pub mod test_skeleton;
pub mod value_members;
