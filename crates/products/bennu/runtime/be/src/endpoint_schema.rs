//! What an endpoint takes and returns, as a schema Hirundo can complete against.
//!
//! Hirundo asks, for a detected endpoint, the shape of its `@RequestBody` and of its return type: the fields,
//! their types, nested objects, lists, enums, with the recursion cut at a few levels (a `Customer` that holds a
//! `List<Order>` that holds a `Customer` stops there). The same index that answers the generators answers this,
//! so the three agree about what a DTO is. Validation annotations are not in the index (it holds members, not
//! their annotations), so `required` means what the type says: a primitive is never null.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use bennu_core::prelude::BennuState;
use serde::Serialize;

use crate::code_generate::{endpoint_infos, member_fields};

/// Levels of nesting followed before a type is left as its name.
const MAX_DEPTH: usize = 4;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Schema {
    /// The type as written (`List<OrderDto>`).
    pub type_text: String,
    /// `object` | `array` | `map` | `enum` | `scalar`.
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<SchemaField>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub constants: Vec<String>,
    /// The element of an `array`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub item: Option<Box<Schema>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchemaField {
    pub name: String,
    pub type_text: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Box<Schema>>,
}

#[derive(Debug, Serialize)]
pub struct EndpointSchema {
    pub request: Option<Schema>,
    pub response: Option<Schema>,
}

/// How a Java type text is built: a wrapper that is looked through, a collection, a map, or a type of its own.
#[derive(Debug, PartialEq, Eq)]
pub enum Shape {
    Scalar,
    Array(String),
    Map,
    /// `ResponseEntity<T>`, `Optional<T>`, `Mono<T>`…: the answer is the `T`.
    Wrapper(String),
    Named(String),
    Nothing,
}

/// `Name<A, B>` → (`Name`, [`A`, `B`]); a type without arguments has none.
pub fn split_generic(text: &str) -> (String, Vec<String>) {
    let t = text.trim();
    let Some(open) = t.find('<') else { return (t.to_string(), Vec::new()) };
    let name = t[..open].trim().to_string();
    let inner = t[open + 1..].trim_end().strip_suffix('>').unwrap_or(&t[open + 1..]);
    let (mut args, mut depth, mut start) = (Vec::new(), 0i32, 0usize);
    for (i, c) in inner.char_indices() {
        match c {
            '<' => depth += 1,
            '>' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inner[start..i].trim().to_string());
                start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inner[start..].trim().to_string());
    (name, args.into_iter().filter(|a| !a.is_empty()).collect())
}

const SCALARS: &[&str] = &[
    "String", "CharSequence", "char", "Character", "boolean", "Boolean", "byte", "Byte", "short", "Short", "int", "Integer", "long", "Long",
    "float", "Float", "double", "Double", "BigDecimal", "BigInteger", "Number", "UUID", "Object", "LocalDate", "LocalDateTime", "LocalTime",
    "Instant", "OffsetDateTime", "ZonedDateTime", "Date", "Timestamp", "Duration", "Period", "URI", "URL", "Locale", "Currency", "Resource",
    "byte[]", "MultipartFile",
];

pub fn classify(type_text: &str) -> Shape {
    let (name, args) = split_generic(type_text);
    let simple = name.rsplit('.').next().unwrap_or(&name).to_string();
    if simple == "void" || simple == "Void" || simple.is_empty() {
        return Shape::Nothing;
    }
    if let Some(element) = simple.strip_suffix("[]") {
        return if simple == "byte[]" { Shape::Scalar } else { Shape::Array(element.to_string()) };
    }
    match simple.as_str() {
        "ResponseEntity" | "HttpEntity" | "Optional" | "Mono" | "CompletableFuture" | "Callable" | "DeferredResult" | "WebAsyncTask" | "Future" => {
            args.into_iter().next().map_or(Shape::Nothing, Shape::Wrapper)
        }
        "List" | "Set" | "Collection" | "Iterable" | "Flux" | "Page" | "Slice" | "Stream" | "ArrayList" | "LinkedList" | "HashSet" | "TreeSet" | "LinkedHashSet" | "SortedSet" | "Queue" | "Deque" => {
            args.into_iter().next().map_or(Shape::Scalar, Shape::Array)
        }
        "Map" | "HashMap" | "TreeMap" | "LinkedHashMap" | "SortedMap" | "MultiValueMap" | "Properties" => Shape::Map,
        s if SCALARS.contains(&s) => Shape::Scalar,
        _ => Shape::Named(type_text.trim().to_string()),
    }
}

fn is_primitive(type_text: &str) -> bool {
    matches!(type_text.trim(), "boolean" | "byte" | "short" | "int" | "long" | "float" | "double" | "char")
}

/// The schema of `type_text`, resolved against the types of `file` (its imports decide what a bare name means).
fn schema_of(file: &str, type_text: &str, depth: usize, seen: &mut BTreeSet<String>) -> Option<Schema> {
    let text = type_text.trim().to_string();
    let scalar = |text: String| Schema { type_text: text, kind: "scalar", fields: Vec::new(), constants: Vec::new(), item: None };
    match classify(&text) {
        Shape::Nothing => None,
        Shape::Scalar => Some(scalar(text)),
        Shape::Map => Some(Schema { type_text: text, kind: "map", fields: Vec::new(), constants: Vec::new(), item: None }),
        Shape::Wrapper(inner) => schema_of(file, &inner, depth, seen),
        Shape::Array(element) => {
            let item = schema_of(file, &element, depth + 1, seen).map(Box::new);
            Some(Schema { type_text: text, kind: "array", fields: Vec::new(), constants: Vec::new(), item })
        }
        Shape::Named(name) => {
            let (simple, _) = split_generic(&name);
            if depth >= MAX_DEPTH || !seen.insert(simple.clone()) {
                return Some(Schema { type_text: text, kind: "object", fields: Vec::new(), constants: Vec::new(), item: None });
            }
            let result = match member_fields(file, &simple) {
                None => scalar(text),
                Some((true, constants, _)) => Schema { type_text: text, kind: "enum", fields: Vec::new(), constants, item: None },
                Some((false, _, fields)) => {
                    let fields = fields
                        .into_iter()
                        .map(|(name, ty)| SchemaField {
                            required: is_primitive(&ty),
                            schema: schema_of(file, &ty, depth + 1, seen).filter(|s| s.kind != "scalar").map(Box::new),
                            name,
                            type_text: ty,
                        })
                        .collect();
                    Schema { type_text: text, kind: "object", fields, constants: Vec::new(), item: None }
                }
            };
            seen.remove(&simple);
            Some(result)
        }
    }
}

/// The request body and the answer of the endpoint `id` of the project at `root`.
#[arbor_rpc::handler]
fn endpoint_schema(_ctx: &BennuState, root: String, id: String) -> Result<EndpointSchema, String> {
    let (info, file) = endpoint_infos(&root).into_iter().find(|(e, _)| e.id == id).ok_or_else(|| format!("no endpoint '{id}' in this project"))?;
    let body = info.params.iter().find(|p| p.binding == "body").map(|p| p.type_text.clone());
    Ok(EndpointSchema {
        request: body.and_then(|t| schema_of(&file, &t, 0, &mut BTreeSet::new())),
        response: schema_of(&file, &info.return_type, 0, &mut BTreeSet::new()),
    })
}

// ── Constants of the project, read from source ────────────────────────────────────────────────────────

/// Finds the sources of the classes an annotation names (`Api` in `Api.JSON_V1`): beside the controller first,
/// then anywhere under the project. Each class is searched for once.
pub struct SourceReader<'a> {
    root: &'a str,
    controller: &'a str,
    cache: std::cell::RefCell<std::collections::HashMap<String, Option<String>>>,
}

impl<'a> SourceReader<'a> {
    pub fn new(root: &'a str, controller: &'a str) -> Self {
        SourceReader { root, controller, cache: Default::default() }
    }

    /// `None` is the controller itself.
    pub fn read(&self, class: Option<&str>) -> Option<String> {
        let Some(class) = class else { return fs::read_to_string(self.controller).ok() };
        self.cache.borrow_mut().entry(class.to_string()).or_insert_with(|| self.find(class)).clone()
    }

    fn find(&self, class: &str) -> Option<String> {
        let name = format!("{class}.java");
        if let Some(beside) = Path::new(self.controller).parent().map(|d| d.join(&name)) {
            if let Ok(text) = fs::read_to_string(beside) {
                return Some(text);
            }
        }
        find_file(Path::new(self.root), &name, 0).and_then(|p| fs::read_to_string(p).ok())
    }
}

fn find_file(dir: &Path, name: &str, depth: usize) -> Option<PathBuf> {
    if depth > 10 {
        return None;
    }
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name = file_name.to_string_lossy();
        if path.is_dir() {
            if !file_name.starts_with('.') && !matches!(file_name.as_ref(), "target" | "node_modules" | "build" | "out") {
                subdirs.push(path);
            }
        } else if file_name == name {
            return Some(path);
        }
    }
    subdirs.into_iter().find_map(|d| find_file(&d, name, depth + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_generic_type_is_split_at_the_top_level() {
        assert_eq!(split_generic("Map<String, List<Order>>"), ("Map".to_string(), vec!["String".to_string(), "List<Order>".to_string()]));
        assert_eq!(split_generic("Order"), ("Order".to_string(), Vec::<String>::new()));
    }

    #[test]
    fn wrappers_collections_maps_and_scalars_are_told_apart() {
        assert_eq!(classify("ResponseEntity<OrderDto>"), Shape::Wrapper("OrderDto".into()));
        assert_eq!(classify("List<OrderDto>"), Shape::Array("OrderDto".into()));
        assert_eq!(classify("Flux<OrderDto>"), Shape::Array("OrderDto".into()));
        assert_eq!(classify("OrderDto[]"), Shape::Array("OrderDto".into()));
        assert_eq!(classify("Map<String, Object>"), Shape::Map);
        assert_eq!(classify("java.util.UUID"), Shape::Scalar);
        assert_eq!(classify("long"), Shape::Scalar);
        assert_eq!(classify("void"), Shape::Nothing);
        assert_eq!(classify("OrderDto"), Shape::Named("OrderDto".into()));
    }

    #[test]
    fn a_wrapper_around_a_list_is_followed_to_the_element() {
        let Shape::Wrapper(inner) = classify("ResponseEntity<List<OrderDto>>") else { panic!("a wrapper") };
        assert_eq!(classify(&inner), Shape::Array("OrderDto".into()));
    }

    #[test]
    fn a_scalar_has_a_schema_without_the_index() {
        let s = schema_of("/nowhere/A.java", "String", 0, &mut BTreeSet::new()).unwrap();
        assert_eq!((s.kind, s.type_text.as_str()), ("scalar", "String"));
        assert!(schema_of("/nowhere/A.java", "void", 0, &mut BTreeSet::new()).is_none());
    }

    #[test]
    fn the_source_reader_follows_a_class_beside_the_controller() {
        let dir = std::env::temp_dir().join(format!("hirundo-reader-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("Api.java"), "class Api { static final String V1 = \"application/x\"; }").unwrap();
        fs::write(dir.join("C.java"), "class C {}").unwrap();
        let controller = dir.join("C.java").to_string_lossy().to_string();
        let root = dir.to_string_lossy().to_string();
        let reader = SourceReader::new(&root, &controller);
        assert!(reader.read(Some("Api")).unwrap().contains("V1"));
        assert!(reader.read(Some("Nope")).is_none());
        assert_eq!(reader.read(None).unwrap(), "class C {}");
        let _ = fs::remove_dir_all(&dir);
    }
}
