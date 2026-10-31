//! The glue of the field-by-field mapping: finds the two classes in the index, turns their members into the plain
//! data `bennu_flc::mapper` matches, and answers with the next statement. See that module for the rules.

use bennu_flc::prelude::{next_mapping, read_context, render_mapping, MappingProp, MappingStyle, mapping_stem};
use bennu_java::prelude::{extract_symbols, parse_java, visible_bindings, ClassMembers, Member, TypeRef, Visibility};

use crate::index_service::IndexService;

/// The next mapping statement and how sure it is.
pub(crate) struct Mapped {
    pub text: String,
    pub confidence: f32,
}

fn dotted(binary: &str) -> String {
    binary.replace(['/', '$'], ".")
}

/// `java.util.List<java.lang.String>`, `int[]`: a type as one comparable string.
fn render_ty(t: &TypeRef) -> String {
    let args = if t.type_args.is_empty() { String::new() } else { format!("<{}>", t.type_args.iter().map(render_ty).collect::<Vec<_>>().join(",")) };
    format!("{}{args}{}", dotted(&t.binary_name), "[]".repeat(t.dims as usize))
}

fn simple(binary: &str) -> &str {
    binary.rsplit(['/', '$', '.']).next().unwrap_or(binary)
}

fn public_instance(m: &&Member) -> bool {
    m.visibility == Visibility::Public && !m.is_static && !m.name.starts_with('<')
}

/// The `import`/`package` lines: all a name is resolved against.
fn imports_of(source: &str) -> String {
    source.lines().map(str::trim_start).filter(|l| l.starts_with("import ") || l.starts_with("package ")).collect::<Vec<_>>().join("\n")
}

fn tables(file: &str, imports: &str, name: &str) -> Option<(String, Vec<std::sync::Arc<ClassMembers>>)> {
    IndexService::global().header_type(file, imports, name)
}

/// The names of the target's fields in declaration order — the order its properties are mapped in.
fn field_order(tables: &[std::sync::Arc<ClassMembers>]) -> Vec<String> {
    tables.first().map(|t| t.fields.iter().filter(|f| !f.is_static).map(|f| mapping_stem(&f.name)).collect()).unwrap_or_default()
}

fn in_field_order(mut props: Vec<MappingProp>, order: &[String]) -> Vec<MappingProp> {
    props.sort_by_key(|p| order.iter().position(|f| *f == mapping_stem(&p.name)).unwrap_or(usize::MAX));
    props
}

/// `setX(T)` of the target and its supertypes.
fn setters(tables: &[std::sync::Arc<ClassMembers>]) -> Vec<MappingProp> {
    let mut out: Vec<MappingProp> = Vec::new();
    for m in tables.iter().flat_map(|t| t.methods.iter()).filter(public_instance) {
        let Some(prop) = m.name.strip_prefix("set").filter(|p| p.starts_with(|c: char| c.is_uppercase())) else { continue };
        if m.params.len() == 1 && !out.iter().any(|o| o.method == m.name) {
            out.push(MappingProp { name: prop.to_string(), method: m.name.clone(), ty: render_ty(&m.params[0]) });
        }
    }
    in_field_order(out, &field_order(tables))
}

/// The one-argument methods of the target's builder that return the builder.
fn builder_methods(file: &str, imports: &str, target: &[std::sync::Arc<ClassMembers>]) -> Vec<MappingProp> {
    let Some(factory) = target.iter().flat_map(|t| t.methods.iter()).find(|m| m.name == "builder" && m.is_static && m.params.is_empty()) else { return Vec::new() };
    let builder = dotted(&factory.return_type.binary_name);
    let Some((binary, builder_tables)) = tables(file, imports, &builder) else { return Vec::new() };
    let mut out: Vec<MappingProp> = Vec::new();
    for m in builder_tables.iter().flat_map(|t| t.methods.iter()).filter(public_instance) {
        if m.params.len() == 1 && m.name != "build" && m.return_type.binary_name == binary && !out.iter().any(|o| o.method == m.name) {
            out.push(MappingProp { name: m.name.clone(), method: m.name.clone(), ty: render_ty(&m.params[0]) });
        }
    }
    in_field_order(out, &field_order(target))
}

/// `getX()` / `isX()` of the source, and — for a record, or a class whose accessors are named like its fields — `x()`.
fn getters(tables: &[std::sync::Arc<ClassMembers>]) -> Vec<MappingProp> {
    let fields: Vec<&str> = tables.iter().flat_map(|t| t.fields.iter()).map(|f| f.name.as_str()).collect();
    let mut out: Vec<MappingProp> = Vec::new();
    for m in tables.iter().flat_map(|t| t.methods.iter()).filter(public_instance) {
        if !m.params.is_empty() || m.return_type.binary_name == "void" {
            continue;
        }
        let name = if let Some(p) = m.name.strip_prefix("get").filter(|p| p.starts_with(|c: char| c.is_uppercase())) {
            Some(p)
        } else if let Some(p) = m.name.strip_prefix("is").filter(|p| p.starts_with(|c: char| c.is_uppercase())) {
            Some(p)
        } else {
            fields.contains(&m.name.as_str()).then_some(m.name.as_str())
        };
        if let Some(name) = name {
            if !out.iter().any(|o| o.method == m.name) {
                out.push(MappingProp { name: name.to_string(), method: m.name.clone(), ty: render_ty(&m.return_type) });
            }
        }
    }
    out
}

/// The next `target.setX(source.getX());` (or `.x(source.getX())`) for the empty line at `offset`.
pub(crate) fn suggest(file: &str, source: &str, offset: usize) -> Option<Mapped> {
    if offset > source.len() || !source.is_char_boundary(offset) {
        return None;
    }
    let line_start = source[..offset].rfind('\n').map_or(0, |p| p + 1);
    let line_end = source[offset..].find('\n').map_or(source.len(), |p| offset + p);
    if !source[line_start..line_end].trim().is_empty() {
        return None;
    }
    let ctx = read_context(&source[..line_start])?;

    // the variables' types, from the scope the caret is in
    let resolver = IndexService::global().caret_resolver_for(file)?;
    let tree = parse_java(source)?;
    let symbols = extract_symbols(source);
    let bindings = visible_bindings(&tree.root_node(), source, &symbols, offset, &*resolver);
    let type_of = |name: &str| bindings.iter().find(|b| b.name == name).and_then(|b| b.ty.clone());

    let target_type: String = match (&ctx.target_type, ctx.target_var.as_deref().and_then(|v| type_of(v))) {
        (Some(written), _) => written.clone(),
        (None, Some(t)) => dotted(&t.binary_name),
        (None, None) => return None,
    };
    let source_var = match &ctx.source_var {
        Some(v) => v.clone(),
        None => {
            // nothing above reads a source yet: the one parameter that is a class other than the target
            let target_simple = simple(&target_type).to_string();
            let mut candidates = bindings.iter().filter(|b| b.is_parameter).filter(|b| b.ty.as_ref().is_some_and(|t| t.dims == 0 && t.binary_name.contains('/') && !t.binary_name.starts_with("java/lang/") && simple(&t.binary_name) != target_simple));
            let only = candidates.next()?;
            if candidates.next().is_some() {
                return None;
            }
            only.name.clone()
        }
    };
    let source_type = dotted(&type_of(&source_var)?.binary_name);

    let imports = imports_of(source);
    let (_, target_tables) = tables(file, &imports, &target_type)?;
    let (_, source_tables) = tables(file, &imports, &source_type)?;
    let target_props = match ctx.style {
        MappingStyle::Setter => setters(&target_tables),
        MappingStyle::Builder => builder_methods(file, &imports, &target_tables),
    };
    let source_props = getters(&source_tables);
    let pick = next_mapping(&target_props, &source_props, &ctx.assigned, simple(&target_type), simple(&source_type))?;
    let target_var = ctx.target_var.as_deref().unwrap_or("");
    if ctx.style == MappingStyle::Setter && target_var.is_empty() {
        return None;
    }
    Some(Mapped { text: render_mapping(ctx.style, target_var, &source_var, &target_props[pick.target], &source_props[pick.source]), confidence: pick.confidence })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arbor_ipc::prelude::EventSink;
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    struct Quiet;
    impl EventSink for Quiet {
        fn emit(&self, _topic: &str, _payload: serde_json::Value) {}
    }

    const ENTITY: &str = "package z;\npublic class Customer {\n    private Long id;\n    private String name;\n    private int age;\n    private boolean active;\n    private String secret;\n    public Long getId() { return id; }\n    public String getName() { return name; }\n    public int getAge() { return age; }\n    public boolean isActive() { return active; }\n    public String getSecret() { return secret; }\n}\n";
    const DTO: &str = "package z;\npublic class CustomerDto {\n    private Long id;\n    private String name;\n    private Integer age;\n    private boolean active;\n    private String nickname;\n    public void setId(Long v) { }\n    public void setName(String v) { }\n    public void setAge(Integer v) { }\n    public void setActive(boolean v) { }\n    public void setNickname(String v) { }\n    public static CustomerDtoBuilder builder() { return null; }\n    public static class CustomerDtoBuilder {\n        public CustomerDtoBuilder id(Long v) { return this; }\n        public CustomerDtoBuilder name(String v) { return this; }\n        public CustomerDtoBuilder age(Integer v) { return this; }\n        public CustomerDto build() { return null; }\n    }\n}\n";

    /// Opens a throw-away project with the two classes and the converter under test, and asks at the `|` in `body`.
    fn ask(body: &str) -> Option<Mapped> {
        // one project on disk and one index open for all of them: the tests take turns
        static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _turn = TURN.lock().unwrap_or_else(|p| p.into_inner());
        let dir = std::env::temp_dir().join(format!("arbor-mapper-{}", std::process::id()));
        let java = dir.join("src/main/java/z");
        std::fs::create_dir_all(&java).unwrap();
        let use_src = format!("package z;\npublic class Convert {{\n    CustomerDto toDto(Customer c) {{\n{body}\n    }}\n}}\n");
        let caret = use_src.find('|').unwrap();
        let use_src = use_src.replace('|', "");
        for (name, text) in [("Customer.java", ENTITY), ("CustomerDto.java", DTO), ("Convert.java", use_src.as_str())] {
            std::fs::write(java.join(name), text).unwrap();
        }
        let root = dir.to_string_lossy().into_owned();
        let svc = IndexService::global();
        if !svc.has_resolver(&root) {
            svc.open(&root, "21", "UTF-8", Arc::new(Quiet));
            let started = Instant::now();
            while !svc.has_resolver(&root) {
                assert!(started.elapsed() < Duration::from_secs(120), "the index did not come up");
                std::thread::sleep(Duration::from_millis(200));
            }
        }
        suggest(&java.join("Convert.java").to_string_lossy(), &use_src, caret)
    }

    #[test]
    fn the_next_setter_follows_the_ones_written() {
        let m = ask("        CustomerDto d = new CustomerDto();\n        d.setId(c.getId());\n|").expect("a mapping");
        assert_eq!(m.text, "d.setName(c.getName());");
        assert_eq!(m.confidence, 0.9);
    }

    #[test]
    fn right_after_the_construction_the_first_property_is_proposed() {
        let m = ask("        CustomerDto d = new CustomerDto();\n|").expect("a mapping");
        assert_eq!(m.text, "d.setId(c.getId());");
    }

    #[test]
    fn a_boxed_type_is_mapped_with_less_confidence_and_a_property_without_a_source_is_skipped() {
        let m = ask("        CustomerDto d = new CustomerDto();\n        d.setId(c.getId());\n        d.setName(c.getName());\n|").expect("a mapping");
        assert_eq!((m.text.as_str(), m.confidence), ("d.setAge(c.getAge());", 0.75));
        // age done: `active` comes through `isActive`, and `nickname` (nothing in the customer) is never proposed
        let m = ask("        CustomerDto d = new CustomerDto();\n        d.setAge(c.getAge());\n        d.setId(c.getId());\n        d.setName(c.getName());\n|").expect("a mapping");
        assert_eq!(m.text, "d.setActive(c.isActive());");
        assert!(ask("        CustomerDto d = new CustomerDto();\n        d.setAge(c.getAge());\n        d.setId(c.getId());\n        d.setName(c.getName());\n        d.setActive(c.isActive());\n|").is_none());
    }

    #[test]
    fn a_builder_chain_continues_in_its_own_style() {
        let m = ask("        return CustomerDto.builder()\n            .id(c.getId())\n|").expect("a mapping");
        assert_eq!(m.text, ".name(c.getName())");
    }

    #[test]
    fn nothing_is_offered_mid_line_or_away_from_a_conversion() {
        assert!(ask("        int total = 1;\n|").is_none());
        assert!(ask("        CustomerDto d = new CustomerDto();\n        d.set|").is_none());
    }
}
