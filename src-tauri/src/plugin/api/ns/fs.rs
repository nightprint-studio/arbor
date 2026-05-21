//! `arbor.fs` — sandboxed filesystem ops.
//!
//! Calling convention (Phase 1+2):
//!   · I/O / parse failures return `(nil, err_string)`; success returns
//!     `(value, nil)` where the value is whatever the op produces (string,
//!     table, true, …). Callers that don't care about errors can simply
//!     read the first return value.
//!   · Permission denied (`fs = "..."` not declared) raises a Lua error
//!     — that's a programming error fixed in plugin.toml.
//!   · Multi-arg structured-edit ops (`move`, `glob`, `*_set`) take a
//!     single config table.

use std::path::PathBuf;

use mlua::{Lua, LuaSerdeExt, Table};

use crate::error::{AppError, Result};
use crate::plugin::api::ctx::ApiCtx;
use crate::plugin::api::helpers::convert::strip_utf8_bom;
use crate::plugin::api::helpers::fs_perm::{check_fs_read, check_fs_write, FsPerm};
use crate::plugin::api::helpers::glob::walk_glob;
use crate::plugin::api::helpers::json_patch::set_json_at_path;
use crate::plugin::api::helpers::tuple::{LuaTuple, err2, ok2};
use crate::plugin::api::helpers::xml_patch::apply_xml_set;

pub(crate) fn install(ctx: &ApiCtx, lua: &Lua, arbor: &Table) -> Result<()> {
    let fs_table = lua.create_table().map_err(|e| AppError::Plugin(e.to_string()))?;
    let fp: FsPerm = (ctx.fs_perm, ctx.fs_scope.clone());

    install_read_ops(lua, &fs_table, &fp)?;
    install_write_ops(lua, &fs_table, &fp)?;
    install_structured_edit_ops(lua, &fs_table, &fp)?;
    install_join(lua, &fs_table)?;

    arbor.set("fs", fs_table).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Read-only ops (exists, is_file, is_dir, read, list, glob)
// ─────────────────────────────────────────────────────────────────────────
fn install_read_ops(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    // exists(path) → boolean   (read perm — never fails)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, path: String| {
            let p = PathBuf::from(&path);
            check_fs_read(lua_ctx, &p, &fp)?;
            Ok(p.exists())
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("exists", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // is_file(path) → boolean
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, path: String| {
            let p = PathBuf::from(&path);
            check_fs_read(lua_ctx, &p, &fp)?;
            Ok(p.is_file())
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("is_file", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // is_dir(path) → boolean
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, path: String| {
            let p = PathBuf::from(&path);
            check_fs_read(lua_ctx, &p, &fp)?;
            Ok(p.is_dir())
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("is_dir", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // read(path) → content | (nil, err)
    //
    // Strips a leading UTF-8 BOM before handing the string to Lua — no
    // plugin author has ever wanted `\u{FEFF}` at the start of their
    // processing string, and tools like serde_json / Lua's own regex
    // choke on it.
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, path: String| -> LuaTuple {
            let p = PathBuf::from(&path);
            check_fs_read(lua_ctx, &p, &fp)?;
            match std::fs::read_to_string(&p) {
                Ok(s)  => ok2(lua_ctx, strip_utf8_bom(&s).to_string()),
                Err(e) => err2(lua_ctx, format!("fs.read {path}: {e}")),
            }
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("read", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // list(dir) → (entries[], nil) | (nil, err)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, dir: String| -> LuaTuple {
            let p = PathBuf::from(&dir);
            check_fs_read(lua_ctx, &p, &fp)?;
            let entries = match std::fs::read_dir(&p) {
                Ok(it) => it,
                Err(e) => return err2(lua_ctx, format!("fs.list {dir}: {e}")),
            };
            let result = lua_ctx.create_table()?;
            let mut i = 1usize;
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let meta = entry.metadata().ok();
                let is_file = meta.as_ref().map(|m| m.is_file()).unwrap_or(false);
                let is_dir  = meta.as_ref().map(|m| m.is_dir()).unwrap_or(false);
                let row = lua_ctx.create_table()?;
                row.set("name",    name)?;
                row.set("is_file", is_file)?;
                row.set("is_dir",  is_dir)?;
                result.set(i, row)?;
                i += 1;
            }
            ok2(lua_ctx, result)
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("list", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // glob{root, pattern, include_dirs?, max_depth?} → (paths[], nil) | (nil, err)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            let root: String = cfg.get("root").map_err(|_|
                mlua::Error::RuntimeError("arbor.fs.glob: 'root' is required".into()))?;
            let pattern: String = cfg.get("pattern").map_err(|_|
                mlua::Error::RuntimeError("arbor.fs.glob: 'pattern' is required".into()))?;
            let include_dirs: bool = cfg.get::<Option<bool>>("include_dirs").unwrap_or(None).unwrap_or(false);
            let max_depth: i64 = cfg.get::<Option<i64>>("max_depth").unwrap_or(None).unwrap_or(-1);

            let root_p = PathBuf::from(&root);
            check_fs_read(lua_ctx, &root_p, &fp)?;
            let mut out: Vec<String> = Vec::new();
            if root_p.exists() {
                walk_glob(&root_p, &pattern, 0, max_depth, include_dirs, &mut out);
            }
            let arr = lua_ctx.create_table()?;
            for (i, p) in out.iter().enumerate() {
                arr.set(i + 1, p.as_str())?;
            }
            ok2(lua_ctx, arr)
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("glob", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Mutating ops (write, append, touch, move, copy, delete)
// ─────────────────────────────────────────────────────────────────────────
fn install_write_ops(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    // write(path, content) → (true, nil) | (nil, err)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, (path, content): (String, String)| -> LuaTuple {
            let p = PathBuf::from(&path);
            check_fs_write(lua_ctx, &p, &fp)?;
            if let Some(parent) = p.parent() { let _ = std::fs::create_dir_all(parent); }
            match std::fs::write(&p, &content) {
                Ok(_)  => ok2(lua_ctx, true),
                Err(e) => err2(lua_ctx, format!("fs.write {path}: {e}")),
            }
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("write", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // append(path, content) → (true, nil) | (nil, err)
    //
    // Append raw bytes to the end of the file (creating it if missing).
    // Parent directories are created. Writes are always BOM-free.
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, (path, content): (String, String)| -> LuaTuple {
            use std::io::Write;
            let p = PathBuf::from(&path);
            check_fs_write(lua_ctx, &p, &fp)?;
            if let Some(parent) = p.parent() { let _ = std::fs::create_dir_all(parent); }
            let mut f = match std::fs::OpenOptions::new().create(true).append(true).open(&p) {
                Ok(f)  => f,
                Err(e) => return err2(lua_ctx, format!("fs.append open {path}: {e}")),
            };
            if let Err(e) = f.write_all(content.as_bytes()) {
                return err2(lua_ctx, format!("fs.append write {path}: {e}"));
            }
            ok2(lua_ctx, true)
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("append", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // touch(path) → (true, nil) | (nil, err)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, path: String| -> LuaTuple {
            let p = PathBuf::from(&path);
            check_fs_write(lua_ctx, &p, &fp)?;
            if let Some(parent) = p.parent() { let _ = std::fs::create_dir_all(parent); }
            if p.exists() {
                if let Err(e) = std::fs::OpenOptions::new().append(true).open(&p) {
                    return err2(lua_ctx, format!("fs.touch open {path}: {e}"));
                }
            } else if let Err(e) = std::fs::write(&p, b"") {
                return err2(lua_ctx, format!("fs.touch create {path}: {e}"));
            }
            ok2(lua_ctx, true)
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("touch", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // move{src, dest, overwrite?} → (true, nil) | (nil, err)
    //
    // Rename a file or directory. When src and dest are on the same volume
    // this is atomic. When `overwrite = true`, removes an existing dest
    // beforehand (lets Windows' `rename` succeed). Parent of dest is created.
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
            let src: String = cfg.get("src").map_err(|_|
                mlua::Error::RuntimeError("arbor.fs.move: 'src' is required".into()))?;
            let dest: String = cfg.get("dest").map_err(|_|
                mlua::Error::RuntimeError("arbor.fs.move: 'dest' is required".into()))?;
            let overwrite: bool = cfg.get::<Option<bool>>("overwrite").unwrap_or(None).unwrap_or(false);

            let s = PathBuf::from(&src);
            let d = PathBuf::from(&dest);
            check_fs_read(lua_ctx, &s, &fp)?;
            check_fs_write(lua_ctx, &d, &fp)?;
            if let Some(parent) = d.parent() { let _ = std::fs::create_dir_all(parent); }
            if overwrite && d.exists() {
                let removed = if d.is_dir() {
                    std::fs::remove_dir_all(&d)
                } else {
                    std::fs::remove_file(&d)
                };
                if let Err(e) = removed {
                    return err2(lua_ctx, format!("fs.move overwrite {dest}: {e}"));
                }
            }
            match std::fs::rename(&s, &d) {
                Ok(_)  => ok2(lua_ctx, true),
                Err(e) => err2(lua_ctx, format!("fs.move {src} → {dest}: {e}")),
            }
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("move", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // delete(path) → (true, nil) | (nil, err)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, path: String| -> LuaTuple {
            let p = PathBuf::from(&path);
            check_fs_write(lua_ctx, &p, &fp)?;
            let res = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
            match res {
                Ok(_)  => ok2(lua_ctx, true),
                Err(e) => err2(lua_ctx, format!("fs.delete {path}: {e}")),
            }
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("delete", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    // copy(src, dst) → (true, nil) | (nil, err)
    {
        let fp = fp.clone();
        let fn_ = lua.create_function(move |lua_ctx, (src, dst): (String, String)| -> LuaTuple {
            let src_p = PathBuf::from(&src);
            let mut dst_p = PathBuf::from(&dst);
            check_fs_read(lua_ctx, &src_p, &fp)?;
            check_fs_write(lua_ctx, &dst_p, &fp)?;
            if dst_p.is_dir() {
                if let Some(fname) = src_p.file_name() { dst_p = dst_p.join(fname); }
            }
            if let Some(parent) = dst_p.parent() { let _ = std::fs::create_dir_all(parent); }
            match std::fs::copy(&src_p, &dst_p) {
                Ok(_)  => ok2(lua_ctx, true),
                Err(e) => err2(lua_ctx, format!("fs.copy {src} → {dst}: {e}")),
            }
        }).map_err(|e| AppError::Plugin(e.to_string()))?;
        fs_table.set("copy", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Structured-edit ops (json_set / yaml_set / toml_set / xml_set)
// ─────────────────────────────────────────────────────────────────────────
fn install_structured_edit_ops(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    install_json_set(lua, fs_table, fp)?;
    install_yaml_set(lua, fs_table, fp)?;
    install_toml_set(lua, fs_table, fp)?;
    install_xml_set(lua, fs_table, fp)?;
    Ok(())
}

fn install_json_set(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    let fp = fp.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        let path: String = cfg.get("path").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.json_set: 'path' is required".into()))?;
        let jpath: String = cfg.get("jpath").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.json_set: 'jpath' is required".into()))?;
        let value: mlua::Value = cfg.get("value").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.json_set: 'value' is required".into()))?;
        let pretty: bool = cfg.get::<Option<bool>>("pretty").unwrap_or(None).unwrap_or(true);

        let p = PathBuf::from(&path);
        check_fs_read(lua_ctx, &p, &fp)?;
        check_fs_write(lua_ctx, &p, &fp)?;
        let raw = match std::fs::read_to_string(&p) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.json_set read {path}: {e}")),
        };
        let raw = strip_utf8_bom(&raw);
        let mut doc: serde_json::Value = if raw.trim().is_empty() {
            serde_json::Value::Object(Default::default())
        } else {
            match serde_json::from_str(raw) {
                Ok(v)  => v,
                Err(e) => return err2(lua_ctx, format!("fs.json_set parse {path}: {e}")),
            }
        };
        let new_val: serde_json::Value = lua_ctx.from_value(value)
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.fs.json_set: value → json: {e}")))?;
        if let Err(e) = set_json_at_path(&mut doc, &jpath, new_val) {
            return err2(lua_ctx, format!("fs.json_set: {e}"));
        }
        let out = if pretty { serde_json::to_string_pretty(&doc) } else { serde_json::to_string(&doc) };
        let out = match out {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.json_set encode: {e}")),
        };
        match std::fs::write(&p, out.as_bytes()) {
            Ok(_)  => ok2(lua_ctx, true),
            Err(e) => err2(lua_ctx, format!("fs.json_set write {path}: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    fs_table.set("json_set", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_yaml_set(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    let fp = fp.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        let path: String = cfg.get("path").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.yaml_set: 'path' is required".into()))?;
        let ypath: String = cfg.get("ypath").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.yaml_set: 'ypath' is required".into()))?;
        let value: mlua::Value = cfg.get("value").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.yaml_set: 'value' is required".into()))?;

        let p = PathBuf::from(&path);
        check_fs_read(lua_ctx, &p, &fp)?;
        check_fs_write(lua_ctx, &p, &fp)?;
        let raw = match std::fs::read_to_string(&p) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.yaml_set read {path}: {e}")),
        };
        let raw = strip_utf8_bom(&raw);
        let mut doc: serde_json::Value = if raw.trim().is_empty() {
            serde_json::Value::Object(Default::default())
        } else {
            match serde_yaml_ng::from_str::<serde_json::Value>(raw) {
                Ok(v)  => v,
                Err(e) => return err2(lua_ctx, format!("fs.yaml_set parse {path}: {e}")),
            }
        };
        let new_val: serde_json::Value = lua_ctx.from_value(value)
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.fs.yaml_set: value → yaml: {e}")))?;
        if let Err(e) = set_json_at_path(&mut doc, &ypath, new_val) {
            return err2(lua_ctx, format!("fs.yaml_set: {e}"));
        }
        let out = match serde_yaml_ng::to_string(&doc) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.yaml_set encode: {e}")),
        };
        match std::fs::write(&p, out.as_bytes()) {
            Ok(_)  => ok2(lua_ctx, true),
            Err(e) => err2(lua_ctx, format!("fs.yaml_set write {path}: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    fs_table.set("yaml_set", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_toml_set(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    let fp = fp.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        let path: String = cfg.get("path").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.toml_set: 'path' is required".into()))?;
        let tpath: String = cfg.get("tpath").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.toml_set: 'tpath' is required".into()))?;
        let value: mlua::Value = cfg.get("value").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.toml_set: 'value' is required".into()))?;

        let p = PathBuf::from(&path);
        check_fs_read(lua_ctx, &p, &fp)?;
        check_fs_write(lua_ctx, &p, &fp)?;
        let raw = match std::fs::read_to_string(&p) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.toml_set read {path}: {e}")),
        };
        let raw = strip_utf8_bom(&raw);
        let toml_val: toml::Value = if raw.trim().is_empty() {
            toml::Value::Table(Default::default())
        } else {
            match raw.parse::<toml::Value>() {
                Ok(v)  => v,
                Err(e) => return err2(lua_ctx, format!("fs.toml_set parse {path}: {e}")),
            }
        };
        let mut doc: serde_json::Value = match serde_json::to_value(&toml_val) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("fs.toml_set toml→json: {e}")),
        };
        let new_val: serde_json::Value = lua_ctx.from_value(value)
            .map_err(|e| mlua::Error::RuntimeError(format!("arbor.fs.toml_set: value → toml: {e}")))?;
        if let Err(e) = set_json_at_path(&mut doc, &tpath, new_val) {
            return err2(lua_ctx, format!("fs.toml_set: {e}"));
        }
        let toml_back: toml::Value = match serde_json::from_value(doc) {
            Ok(v)  => v,
            Err(e) => return err2(lua_ctx, format!("fs.toml_set json→toml: {e}")),
        };
        let out = match toml::to_string_pretty(&toml_back) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.toml_set encode: {e}")),
        };
        match std::fs::write(&p, out.as_bytes()) {
            Ok(_)  => ok2(lua_ctx, true),
            Err(e) => err2(lua_ctx, format!("fs.toml_set write {path}: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    fs_table.set("toml_set", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

fn install_xml_set(lua: &Lua, fs_table: &Table, fp: &FsPerm) -> Result<()> {
    let fp = fp.clone();
    let fn_ = lua.create_function(move |lua_ctx, cfg: mlua::Table| -> LuaTuple {
        let path: String = cfg.get("path").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.xml_set: 'path' is required".into()))?;
        let xpath: String = cfg.get("xpath").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.xml_set: 'xpath' is required".into()))?;
        let value: String = cfg.get("value").map_err(|_|
            mlua::Error::RuntimeError("arbor.fs.xml_set: 'value' is required (string)".into()))?;

        let p = PathBuf::from(&path);
        check_fs_read(lua_ctx, &p, &fp)?;
        check_fs_write(lua_ctx, &p, &fp)?;
        let raw = match std::fs::read_to_string(&p) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.xml_set read {path}: {e}")),
        };
        let raw = strip_utf8_bom(&raw);
        let out = match apply_xml_set(raw, &xpath, &value) {
            Ok(s)  => s,
            Err(e) => return err2(lua_ctx, format!("fs.xml_set: {e}")),
        };
        match std::fs::write(&p, out.as_bytes()) {
            Ok(_)  => ok2(lua_ctx, true),
            Err(e) => err2(lua_ctx, format!("fs.xml_set write {path}: {e}")),
        }
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    fs_table.set("xml_set", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────
// Pure compute (join)
// ─────────────────────────────────────────────────────────────────────────
fn install_join(lua: &Lua, fs_table: &Table) -> Result<()> {
    let fn_ = lua.create_function(|lua_ctx, parts: mlua::Variadic<String>| {
        let mut path = PathBuf::new();
        for part in parts.iter() { path.push(part); }
        let mut s = path.to_string_lossy().into_owned();
        if cfg!(target_os = "windows") {
            s = s.replace('/', "\\");
        }
        Ok(lua_ctx.create_string(s.as_bytes())?)
    }).map_err(|e| AppError::Plugin(e.to_string()))?;
    fs_table.set("join", fn_).map_err(|e| AppError::Plugin(e.to_string()))?;
    Ok(())
}
