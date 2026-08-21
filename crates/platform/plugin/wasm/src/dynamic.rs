//! Calling an extension without knowing what it does.
//!
//! ## Why this exists, and what it replaces
//!
//! The typed path — `bindgen!` against a `.wit` in Arbor's tree — makes Arbor learn the
//! interface. That is fine for the three in `host.wit`, because those are **Arbor's**: they
//! are the capabilities it grants, and the whole point is that it decides what they mean.
//!
//! It is wrong for everything a guest *exports*. An extension whose interface Arbor has to
//! carry is not an extension: shipping it means editing Arbor, generating bindings, adding
//! endpoints and a UI, and at that point the wasm file is an implementation detail of a
//! built-in feature. The test is simple — **if the host has to learn something, it is not a
//! plugin.**
//!
//! So the export side is dynamic. A component's own type information says what it exports and
//! what shapes those functions take; this module reads that, coerces JSON into the component
//! model's values and back, and lets the plugin decide what to call.
//!
//! The split is the security-relevant one, not an arbitrary one: **imports are typed and
//! gated, exports are opaque.** Arbor still decides whether a guest may reach a keychain. It
//! has no opinion about what the guest computes.
//!
//! ## Positional arguments
//!
//! A component's type information carries parameter *types* but not their *names*, so the
//! argument list is positional. It reads better than it sounds, because the shapes inside are
//! named: a record is a JSON object keyed by its fields, so a two-parameter call is a short
//! array of readable objects rather than a wall of positional values.
//!
//! ## What does not cross
//!
//! **Resources.** A `resource` is a handle into one store's memory, and there is nothing to
//! turn it into on the other side of a JSON boundary. An interface built on a resource — the
//! studio-format one, for instance — needs the typed path or a redesign around whole calls.
//! Refused by name rather than silently dropped.
//!
//! **`option<option<T>>`.** `none` maps to JSON `null`, which makes `some(none)` and `none`
//! the same document. Nesting an option in an option is rare and the alternative — wrapping
//! every ordinary option in `{"some": …}` — makes the common case worse to read and to write.
//! Refused with a message that says why, instead of picking one of the two silently.

#![cfg(feature = "runtime")]

use serde_json::{json, Value as Json};
use wasmtime::component::{
    types::{ComponentItem, Type},
    Component, Instance, Val,
};
use wasmtime::Store;

use crate::caps::GuestCaps;
use crate::engine::{EngineError, WasmHost};
use crate::guest::GuestState;
use crate::services::Services;

/// One exported function, as discovery reports it.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct FuncSig {
    pub name: String,
    /// How many positional arguments `call` expects.
    pub params: usize,
    /// 0 or 1 — a WIT function returns at most one value.
    pub results: usize,
}

/// One exported interface and its functions.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct InterfaceSurface {
    /// The full export name, as it appears in the component: `arbor:extensions/mesh-source@1.0.0`.
    pub name: String,
    pub funcs: Vec<FuncSig>,
}

/// A live extension, addressed by name rather than by type.
pub struct DynGuest {
    store:     Store<GuestState>,
    instance:  Instance,
    component: Component,
}

impl WasmHost {
    /// Bring up an extension without binding it to a world.
    ///
    /// The linker is the same one every guest gets, so the host interfaces it may import are
    /// exactly the ones Arbor grants — the dynamic side is the exports, never the imports.
    pub fn open_dynamic(
        &self,
        module: &std::path::Path,
        caps: GuestCaps,
        services: Services,
    ) -> Result<DynGuest, EngineError> {
        let component = self.component(module)?;
        let linker = self.linker()?;
        let mut store = self.store(caps, services);
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(EngineError::wasm(format!("instantiating {}", module.display())))?;
        Ok(DynGuest { store, instance, component })
    }
}

impl DynGuest {
    /// Every interface this extension exports, and the functions in each.
    ///
    /// Read from the component's own type information rather than from a manifest: a package
    /// that claimed an interface it does not export would be discovered here, by the thing
    /// that has to call it, instead of at the first call.
    pub fn surface(&self, engine: &wasmtime::Engine) -> Vec<InterfaceSurface> {
        let mut out = Vec::new();
        for (name, item) in self.component.component_type().exports(engine) {
            let ComponentItem::ComponentInstance(iface) = item else {
                // A bare function export at the top level is legal but is not how any of
                // these contracts are shaped; listing it as an interface with no name would
                // be more confusing than leaving it out.
                continue;
            };
            let funcs = iface
                .exports(engine)
                .filter_map(|(fname, fitem)| match fitem {
                    ComponentItem::ComponentFunc(f) => Some(FuncSig {
                        name:    fname.to_string(),
                        params:  f.params().len(),
                        results: f.results().len(),
                    }),
                    _ => None,
                })
                .collect();
            out.push(InterfaceSurface { name: name.to_string(), funcs });
        }
        out
    }

    /// Call one exported function.
    ///
    /// `interface` is the full export name; `None` looks the function up at the top level.
    /// `args` is positional and must match the function's arity — a mismatch is refused here
    /// rather than passed to a guest that would trap on it.
    ///
    /// A function whose single result is a `result<_, E>` is unwrapped: an `err` becomes this
    /// call's error. That is the shape every interface in practice uses to report failure, and
    /// leaving it wrapped would make every caller unwrap it identically.
    pub fn call(
        &mut self,
        interface: Option<&str>,
        func: &str,
        args: &[Json],
    ) -> Result<Json, String> {
        let idx = match interface {
            Some(iface) => {
                let parent = self
                    .instance
                    .get_export(&mut self.store, None, iface)
                    .ok_or_else(|| format!("this extension exports no interface '{iface}'"))?;
                self.instance
                    .get_export(&mut self.store, Some(&parent), func)
                    .ok_or_else(|| format!("'{iface}' has no function '{func}'"))?
            }
            None => self
                .instance
                .get_export(&mut self.store, None, func)
                .ok_or_else(|| format!("this extension exports no function '{func}'"))?,
        };
        let f = self
            .instance
            .get_func(&mut self.store, &idx)
            .ok_or_else(|| format!("'{func}' is not a function"))?;

        let param_types = f.params(&self.store);
        if param_types.len() != args.len() {
            return Err(format!(
                "'{func}' takes {} argument(s), got {}",
                param_types.len(),
                args.len()
            ));
        }
        let params: Vec<Val> = param_types
            .iter()
            .zip(args)
            .enumerate()
            .map(|(i, (ty, arg))| {
                json_to_val(ty, arg).map_err(|e| format!("'{func}' argument {i}: {e}"))
            })
            .collect::<Result<_, _>>()?;

        let result_types = f.results(&self.store);
        let mut results = vec![Val::Bool(false); result_types.len()];
        f.call(&mut self.store, &params, &mut results)
            .map_err(|e| format!("'{func}' trapped: {e}"))?;
        // Required before the next call on this instance; skipping it leaves the guest unable
        // to run again, which shows up as an unrelated failure much later.
        f.post_return(&mut self.store)
            .map_err(|e| format!("'{func}' post-return: {e}"))?;

        match results.into_iter().next() {
            None => Ok(Json::Null),
            Some(Val::Result(Ok(v))) => Ok(v.map(|b| val_to_json(&b)).unwrap_or(Json::Null)),
            Some(Val::Result(Err(e))) => Err(match e {
                Some(b) => describe_error(&val_to_json(&b)),
                None => format!("'{func}' failed"),
            }),
            Some(v) => Ok(val_to_json(&v)),
        }
    }
}

/// Render a guest's error payload as a line a person reads.
///
/// A `variant` error arrives as `{"not-found": "…"}`, and showing that JSON to somebody who
/// mistyped a key is showing them the interface instead of the problem. A plain string passes
/// through untouched.
fn describe_error(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        Json::Object(map) if map.len() == 1 => {
            let (case, payload) = map.iter().next().expect("len checked");
            match payload {
                Json::String(s) => format!("{case}: {s}"),
                Json::Null => case.clone(),
                other => format!("{case}: {other}"),
            }
        }
        other => other.to_string(),
    }
}

// ── JSON → Val ──────────────────────────────────────────────────────────────────

/// Coerce a JSON document into the component-model value a parameter's type demands.
///
/// Every failure names the type it wanted, because the caller writing this JSON is a plugin
/// author who cannot see the WIT from where they are standing.
pub fn json_to_val(ty: &Type, v: &Json) -> Result<Val, String> {
    match ty {
        Type::Bool => v.as_bool().map(Val::Bool).ok_or_else(|| want("a boolean", v)),
        Type::S8   => int(v, i8::MIN as i64, i8::MAX as i64).map(|n| Val::S8(n as i8)),
        Type::U8   => int(v, 0, u8::MAX as i64).map(|n| Val::U8(n as u8)),
        Type::S16  => int(v, i16::MIN as i64, i16::MAX as i64).map(|n| Val::S16(n as i16)),
        Type::U16  => int(v, 0, u16::MAX as i64).map(|n| Val::U16(n as u16)),
        Type::S32  => int(v, i32::MIN as i64, i32::MAX as i64).map(|n| Val::S32(n as i32)),
        Type::U32  => int(v, 0, u32::MAX as i64).map(|n| Val::U32(n as u32)),
        Type::S64  => int(v, i64::MIN, i64::MAX).map(Val::S64),
        Type::U64  => v
            .as_u64()
            .map(Val::U64)
            .ok_or_else(|| want("a non-negative whole number", v)),
        Type::Float32 => v
            .as_f64()
            .map(|n| Val::Float32(n as f32))
            .ok_or_else(|| want("a number", v)),
        Type::Float64 => v.as_f64().map(Val::Float64).ok_or_else(|| want("a number", v)),
        Type::Char => {
            let s = v.as_str().ok_or_else(|| want("a one-character string", v))?;
            let mut it = s.chars();
            match (it.next(), it.next()) {
                (Some(c), None) => Ok(Val::Char(c)),
                _ => Err(want("a one-character string", v)),
            }
        }
        Type::String => v
            .as_str()
            .map(|s| Val::String(s.to_string()))
            .ok_or_else(|| want("a string", v)),

        Type::List(l) => {
            let arr = v.as_array().ok_or_else(|| want("an array", v))?;
            let inner = l.ty();
            arr.iter()
                .enumerate()
                .map(|(i, item)| json_to_val(&inner, item).map_err(|e| format!("[{i}] {e}")))
                .collect::<Result<Vec<_>, _>>()
                .map(Val::List)
        }

        Type::Record(r) => {
            let obj = v.as_object().ok_or_else(|| want("an object", v))?;
            let mut fields = Vec::new();
            for f in r.fields() {
                // Kebab-case is what WIT spells; a plugin author writing JSON reaches for it
                // too, so both are accepted rather than making them remember which.
                let value = obj
                    .get(f.name)
                    .or_else(|| obj.get(&f.name.replace('-', "_")))
                    .ok_or_else(|| format!("missing field '{}'", f.name))?;
                fields.push((
                    f.name.to_string(),
                    json_to_val(&f.ty, value).map_err(|e| format!("field '{}': {e}", f.name))?,
                ));
            }
            Ok(Val::Record(fields))
        }

        Type::Tuple(t) => {
            let arr = v.as_array().ok_or_else(|| want("an array", v))?;
            let types: Vec<Type> = t.types().collect();
            if arr.len() != types.len() {
                return Err(format!("expected {} tuple elements, got {}", types.len(), arr.len()));
            }
            types
                .iter()
                .zip(arr)
                .enumerate()
                .map(|(i, (ty, item))| json_to_val(ty, item).map_err(|e| format!("[{i}] {e}")))
                .collect::<Result<Vec<_>, _>>()
                .map(Val::Tuple)
        }

        Type::Enum(e) => {
            let s = v.as_str().ok_or_else(|| want("a string naming a case", v))?;
            let names: Vec<&str> = e.names().collect();
            if names.contains(&s) {
                Ok(Val::Enum(s.to_string()))
            } else {
                Err(format!("'{s}' is not one of: {}", names.join(", ")))
            }
        }

        Type::Variant(var) => {
            let cases: Vec<_> = var.cases().collect();
            // A payload-less case may be written bare, which is what it looks like anyway.
            if let Some(s) = v.as_str() {
                return match cases.iter().find(|c| c.name == s) {
                    Some(c) if c.ty.is_none() => Ok(Val::Variant(s.to_string(), None)),
                    Some(_) => Err(format!("case '{s}' carries a payload — write {{\"{s}\": …}}")),
                    None => Err(format!(
                        "'{s}' is not one of: {}",
                        cases.iter().map(|c| c.name).collect::<Vec<_>>().join(", ")
                    )),
                };
            }
            let obj = v.as_object().ok_or_else(|| want("an object with one case", v))?;
            if obj.len() != 1 {
                return Err(format!("expected exactly one case, got {}", obj.len()));
            }
            let (name, payload) = obj.iter().next().expect("len checked");
            let case = cases
                .iter()
                .find(|c| c.name == name)
                .ok_or_else(|| format!("'{name}' is not a case of this variant"))?;
            match &case.ty {
                Some(t) => Ok(Val::Variant(
                    name.clone(),
                    Some(Box::new(json_to_val(t, payload)?)),
                )),
                None => Ok(Val::Variant(name.clone(), None)),
            }
        }

        Type::Option(o) => {
            let inner = o.ty();
            if matches!(inner, Type::Option(_)) {
                return Err(
                    "an option inside an option cannot be written as JSON — `null` would mean \
                     both `none` and `some(none)`"
                        .into(),
                );
            }
            match v {
                Json::Null => Ok(Val::Option(None)),
                other => Ok(Val::Option(Some(Box::new(json_to_val(&inner, other)?)))),
            }
        }

        Type::Result(r) => {
            let obj = v
                .as_object()
                .ok_or_else(|| want(r#"an object shaped {"ok": …} or {"err": …}"#, v))?;
            if let Some(ok) = obj.get("ok") {
                return Ok(Val::Result(Ok(match r.ok() {
                    Some(t) => Some(Box::new(json_to_val(&t, ok)?)),
                    None => None,
                })));
            }
            if let Some(err) = obj.get("err") {
                return Ok(Val::Result(Err(match r.err() {
                    Some(t) => Some(Box::new(json_to_val(&t, err)?)),
                    None => None,
                })));
            }
            Err(want(r#"an object shaped {"ok": …} or {"err": …}"#, v))
        }

        Type::Flags(f) => {
            let arr = v.as_array().ok_or_else(|| want("an array of flag names", v))?;
            let known: Vec<&str> = f.names().collect();
            let mut set = Vec::new();
            for item in arr {
                let s = item.as_str().ok_or_else(|| want("a flag name", item))?;
                if !known.contains(&s) {
                    return Err(format!("'{s}' is not one of: {}", known.join(", ")));
                }
                set.push(s.to_string());
            }
            Ok(Val::Flags(set))
        }

        Type::Own(_) | Type::Borrow(_) => Err(
            "this function takes a resource handle, which cannot be written as JSON — an \
             interface built on resources needs whole-call functions instead"
                .into(),
        ),
    }
}

fn want(what: &str, got: &Json) -> String {
    format!("expected {what}, got {got}")
}

fn int(v: &Json, lo: i64, hi: i64) -> Result<i64, String> {
    let n = v.as_i64().ok_or_else(|| want("a whole number", v))?;
    // Range-checked rather than truncated: a `u8` handed 300 is a mistake in the caller, and
    // silently becoming 44 hides it somewhere much less obvious.
    if n < lo || n > hi {
        return Err(format!("{n} is outside {lo}..={hi}"));
    }
    Ok(n)
}

// ── Val → JSON ──────────────────────────────────────────────────────────────────

/// Render a component-model value as JSON, in the same spelling `json_to_val` accepts.
pub fn val_to_json(v: &Val) -> Json {
    match v {
        Val::Bool(b) => json!(b),
        Val::S8(n) => json!(n),
        Val::U8(n) => json!(n),
        Val::S16(n) => json!(n),
        Val::U16(n) => json!(n),
        Val::S32(n) => json!(n),
        Val::U32(n) => json!(n),
        Val::S64(n) => json!(n),
        Val::U64(n) => json!(n),
        Val::Float32(n) => json!(n),
        Val::Float64(n) => json!(n),
        Val::Char(c) => json!(c.to_string()),
        Val::String(s) => json!(s),
        Val::List(items) => Json::Array(items.iter().map(val_to_json).collect()),
        // Both spellings of every field name, and the reason is symmetry with the way in.
        //
        // WIT identifiers are kebab-case, so a record field is `params-schema`. Going IN, this
        // module already accepts either spelling — see the lookup in `json_to_val`, which
        // tries the hyphen and then the underscore, because a plugin author writing a Lua
        // table reaches for `params_schema`. Coming OUT it emitted the hyphen only, so that
        // same author read `entry.params_schema`, got `nil`, and had no way to find out why:
        // in Lua a missing key is not an error, it is an absent value, and a schema that never
        // arrived is indistinguishable from a schema that says nothing.
        //
        // That cost a mesh package every one of its parameters. The controls simply were not
        // there, on every package, and the panel looked exactly like a picker for shapes that
        // have no knobs.
        //
        // The alias cannot collide: WIT forbids `_` in an identifier, so a field spelled with
        // one cannot also exist.
        Val::Record(fields) => Json::Object(
            fields
                .iter()
                .flat_map(|(k, v)| {
                    let json = val_to_json(v);
                    let mut out = vec![(k.clone(), json.clone())];
                    if k.contains('-') {
                        out.push((k.replace('-', "_"), json));
                    }
                    out
                })
                .collect(),
        ),
        Val::Tuple(items) => Json::Array(items.iter().map(val_to_json).collect()),
        Val::Variant(case, payload) => match payload {
            Some(p) => json!({ case.as_str(): val_to_json(p) }),
            // Bare string for a payload-less case, which is how it may be written going in.
            None => json!(case),
        },
        Val::Enum(name) => json!(name),
        Val::Option(inner) => inner.as_ref().map(|b| val_to_json(b)).unwrap_or(Json::Null),
        Val::Result(r) => match r {
            Ok(v) => json!({ "ok": v.as_ref().map(|b| val_to_json(b)).unwrap_or(Json::Null) }),
            Err(e) => json!({ "err": e.as_ref().map(|b| val_to_json(b)).unwrap_or(Json::Null) }),
        },
        Val::Flags(names) => json!(names),
        // A handle is meaningless outside the store that owns it, so it renders as what it is
        // rather than as a number somebody might try to send back.
        Val::Resource(_) => json!("<resource>"),
    }
}

#[cfg(test)]
mod tests {
    /// Un campo kebab arriva anche in snake_case.
    ///
    /// La regressione che questo fissa e' costata a un pacchetto mesh tutti i suoi parametri:
    /// `params-schema` usciva col trattino soltanto, il plugin leggeva `params_schema`, e in
    /// Lua una chiave assente non e' un errore — e' `nil`, cioe' esattamente ciò che si vede
    /// quando una forma non ha knob. Nessuno dei due lati poteva accorgersene.
    #[test]
    fn un_campo_kebab_esce_in_entrambe_le_grafie() {
        let v = Val::Record(vec![
            ("params-schema".to_string(), Val::String("{}".into())),
            ("id".to_string(), Val::String("prism".into())),
        ]);
        let json = val_to_json(&v);
        let obj = json.as_object().expect("un record e' un oggetto");
        assert_eq!(obj.get("params-schema").and_then(|v| v.as_str()), Some("{}"));
        assert_eq!(obj.get("params_schema").and_then(|v| v.as_str()), Some("{}"));
        // Un campo senza trattino resta uno solo: l'alias non e' rumore su ogni chiave.
        assert_eq!(obj.len(), 3);
    }

    use super::*;

    #[test]
    fn scalars_coerce_from_the_json_a_plugin_would_write() {
        assert_eq!(json_to_val(&Type::Bool, &json!(true)).unwrap(), Val::Bool(true));
        assert_eq!(json_to_val(&Type::S32, &json!(-5)).unwrap(), Val::S32(-5));
        assert_eq!(json_to_val(&Type::String, &json!("hi")).unwrap(), Val::String("hi".into()));
        assert_eq!(json_to_val(&Type::Char, &json!("x")).unwrap(), Val::Char('x'));
        match json_to_val(&Type::Float32, &json!(0.5)).unwrap() {
            Val::Float32(f) => assert_eq!(f, 0.5),
            other => panic!("got {other:?}"),
        }
    }

    #[test]
    fn a_number_outside_the_range_is_refused_rather_than_truncated() {
        // 300 into a u8 is a caller's mistake; silently becoming 44 hides it until the guest
        // does something inexplicable with it.
        let e = json_to_val(&Type::U8, &json!(300)).unwrap_err();
        assert!(e.contains("0..=255"), "{e}");
        assert!(json_to_val(&Type::S8, &json!(-200)).is_err());
        assert!(json_to_val(&Type::U8, &json!(-1)).is_err());
    }

    #[test]
    fn a_wrong_shape_says_what_it_wanted() {
        // The person reading this is writing Lua and cannot see the WIT from there.
        let e = json_to_val(&Type::String, &json!(42)).unwrap_err();
        assert!(e.contains("expected a string"), "{e}");
        let e = json_to_val(&Type::Char, &json!("too long")).unwrap_err();
        assert!(e.contains("one-character"), "{e}");
    }

    #[test]
    fn values_render_in_the_spelling_they_are_read_back_in() {
        // Round-tripping matters because a plugin passes one call's output to the next call.
        assert_eq!(val_to_json(&Val::Enum("gcs".into())), json!("gcs"));
        assert_eq!(val_to_json(&Val::Variant("not-found".into(), None)), json!("not-found"));
        assert_eq!(
            val_to_json(&Val::Variant("parse".into(), Some(Box::new(Val::String("bad".into()))))),
            json!({ "parse": "bad" })
        );
        assert_eq!(val_to_json(&Val::Option(None)), Json::Null);
        assert_eq!(
            val_to_json(&Val::Option(Some(Box::new(Val::U32(3))))),
            json!(3)
        );
        assert_eq!(
            val_to_json(&Val::Record(vec![("id".into(), Val::String("a".into()))])),
            json!({ "id": "a" })
        );
        assert_eq!(val_to_json(&Val::Flags(vec!["read".into()])), json!(["read"]));
    }

    #[test]
    fn a_resource_renders_as_what_it_is_and_not_as_a_number() {
        // A handle sent back in would address whatever now sits at that index.
        let rendered = val_to_json(&Val::Result(Ok(None)));
        assert_eq!(rendered, json!({ "ok": Json::Null }));
    }

    #[test]
    fn a_guest_error_reads_as_a_sentence() {
        assert_eq!(describe_error(&json!("plain")), "plain");
        assert_eq!(describe_error(&json!({ "not-found": "a/b.txt" })), "not-found: a/b.txt");
        assert_eq!(describe_error(&json!({ "denied": Json::Null })), "denied");
    }
}
