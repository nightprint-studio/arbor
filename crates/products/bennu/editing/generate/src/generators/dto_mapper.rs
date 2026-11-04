//! A DTO and its mapper from the entity at the caret: a record, Lombok class or plain class for the
//! DTO, and a MapStruct interface or a hand-written mapper (static or Spring component) for the mapping.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::code::JavaFile;
use crate::context::{ClassInfo, FieldInfo, GenContext};
use crate::names::pascal;
use crate::output::{parse_options, require_class_name, require_package, GenFile, Output, SourceRoot};
use crate::registry::{Generator, GeneratorMeta, Target};

pub struct DtoMapper;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DtoOptions {
    pub dto_name: String,
    pub mapper_name: String,
    pub package: String,
    /// Package of the entity, for the import; empty when it shares the DTO's.
    pub entity_package: String,
    pub entity_name: String,
    /// Fields copied to the DTO; empty means all.
    pub fields: Vec<String>,
    /// `auto` | `record` | `lombok` | `class`
    pub dto_kind: String,
    /// `auto` | `mapstruct` | `manual` | `component` | `none`
    pub mapper: String,
    /// Also map back from the DTO to the entity.
    pub to_entity: bool,
    pub list_methods: bool,
}

impl Default for DtoOptions {
    fn default() -> Self {
        DtoOptions {
            dto_name: String::new(),
            mapper_name: String::new(),
            package: String::new(),
            entity_package: String::new(),
            entity_name: String::new(),
            fields: Vec::new(),
            dto_kind: "auto".into(),
            mapper: "auto".into(),
            to_entity: true,
            list_methods: true,
        }
    }
}

impl Generator for DtoMapper {
    fn meta(&self) -> GeneratorMeta {
        GeneratorMeta {
            id: "dto-mapper",
            title: "Generate DTO and mapper from entity",
            description: "A DTO (record, Lombok or plain class) for the class at the caret and a MapStruct or hand-written mapper.",
            icon: "Shuffle",
            group: "Class",
            language: "java",
            targets: &[Target::Class, Target::Java],
            related: &["mapstruct", "lombok"],
        }
    }

    fn defaults(&self, ctx: &GenContext) -> Value {
        let mut o = DtoOptions { package: ctx.package_for("dto"), ..DtoOptions::default() };
        if let Some(c) = &ctx.class {
            o.entity_name = c.name.clone();
            o.entity_package = c.package.clone();
            o.dto_name = format!("{}Dto", c.name);
            o.mapper_name = format!("{}Mapper", c.name);
            o.fields = c.fields.iter().map(|f| f.name.clone()).collect();
        }
        serde_json::to_value(o).unwrap_or(Value::Null)
    }

    fn generate(&self, options: &Value, ctx: &GenContext) -> Result<Output, String> {
        let o: DtoOptions = parse_options(options)?;
        let entity = ctx.class.as_ref().filter(|c| o.entity_name.is_empty() || c.name == o.entity_name).ok_or("Place the caret in the entity to map")?;
        require_package(&o.package)?;
        let dto_name = if o.dto_name.trim().is_empty() { format!("{}Dto", entity.name) } else { o.dto_name.trim().to_string() };
        let mapper_name = if o.mapper_name.trim().is_empty() { format!("{}Mapper", entity.name) } else { o.mapper_name.trim().to_string() };
        require_class_name(&dto_name, "DTO name")?;
        require_class_name(&mapper_name, "Mapper name")?;
        let fields: Vec<&FieldInfo> = entity.fields.iter().filter(|f| o.fields.is_empty() || o.fields.contains(&f.name)).collect();
        if fields.is_empty() {
            return Err("Choose at least one field".into());
        }
        let kind = match o.dto_kind.as_str() {
            "auto" => {
                if ctx.project.records() { "record" } else if ctx.project.lombok() { "lombok" } else { "class" }
            }
            other => other,
        };
        if kind == "record" && !ctx.project.records() {
            return Err("Records need Java 16 — choose a class".into());
        }
        let mapper = match o.mapper.as_str() {
            "auto" => {
                if ctx.project.has("mapstruct") { "mapstruct" } else { "component" }
            }
            other => other,
        };

        let mut out = Output::file(GenFile::java(SourceRoot::Main, &o.package, &dto_name, dto_text(&o, &dto_name, kind, &fields)));
        if mapper != "none" {
            let entity_import = (!entity.package.is_empty() && entity.package != o.package).then(|| entity.fqn());
            let text = mapper_text(&o, &mapper_name, &dto_name, entity, &fields, kind, mapper, entity_import);
            out.files.push(GenFile::java(SourceRoot::Main, &o.package, &mapper_name, text));
        }
        if mapper == "mapstruct" && ctx.project.resolved && !ctx.project.has("mapstruct-processor") {
            out = out.note("MapStruct needs its annotation processor (`org.mapstruct:mapstruct-processor`) configured in the build.");
        }
        if fields.len() < entity.fields.len() {
            out = out.note("Fields left out of the DTO stay unmapped: MapStruct reports them as unmapped target properties when mapping back.");
        }
        Ok(out.needs(ctx.project.missing(if mapper == "mapstruct" { &["org.mapstruct:mapstruct"][..] } else { &[][..] })))
    }
}

fn dto_text(o: &DtoOptions, name: &str, kind: &str, fields: &[&FieldInfo]) -> String {
    let mut f = JavaFile::new(&o.package);
    match kind {
        "record" => {
            let components: Vec<String> = fields.iter().map(|x| format!("{} {}", x.type_name, x.name)).collect();
            f.body.line(format!("public record {name}({}) {{}}", components.join(", ")));
        }
        "lombok" => {
            f.imports.extend(["lombok.AllArgsConstructor", "lombok.Data", "lombok.NoArgsConstructor"]);
            f.body.line("@Data").line("@NoArgsConstructor").line("@AllArgsConstructor");
            f.body.block(format!("public class {name}"), |l| {
                for x in fields {
                    l.line(format!("private {} {};", x.type_name, x.name));
                }
            });
        }
        _ => {
            f.body.block(format!("public class {name}"), |l| {
                for x in fields {
                    l.line(format!("private {} {};", x.type_name, x.name));
                }
                for x in fields {
                    l.blank();
                    l.block(format!("public {} get{}()", x.type_name, pascal(&x.name)), |l| {
                        l.line(format!("return {};", x.name));
                    });
                    l.blank();
                    l.block(format!("public void set{}({} {})", pascal(&x.name), x.type_name, x.name), |l| {
                        l.line(format!("this.{0} = {0};", x.name));
                    });
                }
            });
        }
    }
    f.render()
}

fn entity_read(entity: &ClassInfo, x: &FieldInfo, var: &str) -> String {
    if entity.is_record() {
        return format!("{var}.{}()", x.name);
    }
    let prefix = if x.type_name == "boolean" { "is" } else { "get" };
    format!("{var}.{}()", x.getter.clone().unwrap_or_else(|| format!("{prefix}{}", pascal(&x.name))))
}

fn dto_read(kind: &str, x: &FieldInfo, var: &str) -> String {
    if kind == "record" {
        format!("{var}.{}()", x.name)
    } else {
        let prefix = if x.type_name == "boolean" { "is" } else { "get" };
        format!("{var}.{prefix}{}()", pascal(&x.name))
    }
}

#[allow(clippy::too_many_arguments)]
fn mapper_text(
    o: &DtoOptions,
    mapper_name: &str,
    dto: &str,
    entity: &ClassInfo,
    fields: &[&FieldInfo],
    kind: &str,
    mapper: &str,
    entity_import: Option<String>,
) -> String {
    let mut f = JavaFile::new(&o.package);
    if let Some(import) = entity_import {
        f.imports.add(import);
    }
    let e = &entity.name;
    if mapper == "mapstruct" {
        f.imports.add("org.mapstruct.Mapper");
        f.body.line("@Mapper(componentModel = \"spring\")");
        f.body.block(format!("public interface {mapper_name}"), |l| {
            l.line(format!("{dto} toDto({e} entity);"));
            if o.to_entity {
                l.blank();
                l.line(format!("{e} toEntity({dto} dto);"));
            }
            if o.list_methods {
                l.blank();
                l.line(format!("List<{dto}> toDtos(List<{e}> entities);"));
                if o.to_entity {
                    l.blank();
                    l.line(format!("List<{e}> toEntities(List<{dto}> dtos);"));
                }
            }
        });
        return f.render();
    }
    let component = mapper == "component";
    if component {
        f.imports.add("org.springframework.stereotype.Component");
        f.body.line("@Component");
    }
    let modifier = if component { "public" } else { "public static" };
    let header = if component { format!("public class {mapper_name}") } else { format!("public final class {mapper_name}") };
    f.body.block(header, |l| {
        if !component {
            l.block(format!("private {mapper_name}()"), |_| {});
            l.blank();
        }
        l.block(format!("{modifier} {dto} toDto({e} entity)"), |l| {
            l.block("if (entity == null)", |l| {
                l.line("return null;");
            });
            if kind == "record" || kind == "lombok" {
                let args: Vec<String> = fields.iter().map(|x| entity_read(entity, x, "entity")).collect();
                l.line(format!("return new {dto}({});", args.join(", ")));
            } else {
                l.line(format!("{dto} dto = new {dto}();"));
                for x in fields {
                    l.line(format!("dto.set{}({});", pascal(&x.name), entity_read(entity, x, "entity")));
                }
                l.line("return dto;");
            }
        });
        if o.to_entity {
            l.blank();
            l.block(format!("{modifier} {e} toEntity({dto} dto)"), |l| {
                l.block("if (dto == null)", |l| {
                    l.line("return null;");
                });
                if entity.is_record() {
                    let args: Vec<String> = entity
                        .fields
                        .iter()
                        .map(|x| if fields.iter().any(|y| y.name == x.name) { dto_read(kind, x, "dto") } else { "null".to_string() })
                        .collect();
                    l.line(format!("return new {e}({});", args.join(", ")));
                } else {
                    l.line(format!("{e} entity = new {e}();"));
                    for x in fields {
                        l.line(format!("entity.set{}({});", pascal(&x.name), dto_read(kind, x, "dto")));
                    }
                    l.line("return entity;");
                }
            });
        }
        if o.list_methods {
            f.imports.add("java.util.List");
            let recv = if component { "this::".to_string() } else { format!("{mapper_name}::") };
            l.blank();
            l.block(format!("{modifier} List<{dto}> toDtos(List<{e}> entities)"), |l| {
                l.line(format!("return entities.stream().map({recv}toDto).toList();"));
            });
            if o.to_entity {
                l.blank();
                l.block(format!("{modifier} List<{e}> toEntities(List<{dto}> dtos)"), |l| {
                    l.line(format!("return dtos.stream().map({recv}toEntity).toList();"));
                });
            }
        }
    });
    f.render()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::*;

    fn gen(ctx: &GenContext, options: serde_json::Value) -> Output {
        DtoMapper.generate(&options, ctx).expect("generates")
    }

    #[test]
    fn defaults_name_the_dto_and_select_every_field() {
        let d = DtoMapper.defaults(&sample_context());
        assert_eq!(d["dto_name"], "CustomerDto");
        assert_eq!(d["fields"].as_array().unwrap().len(), 5);
        assert_eq!(d["package"], "com.acme.shop.dto");
    }

    #[test]
    fn a_record_dto_with_a_component_mapper() {
        let ctx = sample_context();
        let out = gen(&ctx, serde_json::json!({ "entity_name": "Customer", "entity_package": "com.acme.shop.model", "package": "com.acme.shop.dto" }));
        let dto = &out.files[0].text;
        assert!(dto.contains("public record CustomerDto(Long id, String name, String email, LocalDate birthDate, List<String> tags) {}"), "{dto}");
        let mapper = &out.files[1].text;
        assert!(mapper.contains("@Component"), "{mapper}");
        assert!(mapper.contains("import com.acme.shop.model.Customer;"), "{mapper}");
        assert!(mapper.contains("return new CustomerDto(entity.getId(), entity.getName(), entity.getEmail(), entity.getBirthDate(), entity.getTags());"), "{mapper}");
        assert!(mapper.contains("entity.setName(dto.name());"), "{mapper}");
        assert!(mapper.contains("entities.stream().map(this::toDto).toList()"), "{mapper}");
    }

    #[test]
    fn mapstruct_is_chosen_when_it_is_on_the_classpath() {
        let mut ctx = sample_context();
        ctx.project.versions.insert("org.mapstruct:mapstruct".into(), "1.5.5.Final".into());
        let out = gen(&ctx, serde_json::json!({ "package": "com.acme.shop.dto" }));
        let mapper = &out.files[1].text;
        assert!(mapper.contains("@Mapper(componentModel = \"spring\")"), "{mapper}");
        assert!(mapper.contains("CustomerDto toDto(Customer entity);"), "{mapper}");
        assert!(out.notes.iter().any(|n| n.contains("annotation processor")));
    }

    #[test]
    fn a_class_dto_has_accessors_and_a_static_mapper_has_a_private_constructor() {
        let mut ctx = sample_context();
        ctx.project.java = 11;
        let out = gen(&ctx, serde_json::json!({ "package": "p", "mapper": "manual", "fields": ["name"] }));
        assert!(out.files[0].text.contains("public String getName()"), "{}", out.files[0].text);
        let mapper = &out.files[1].text;
        assert!(mapper.contains("public static CustomerDto toDto(Customer entity)"), "{mapper}");
        assert!(mapper.contains("private CustomerMapper() {"), "{mapper}");
        assert!(mapper.contains("dto.setName(entity.getName());"), "{mapper}");
        assert!(mapper.contains("map(CustomerMapper::toDto)"), "{mapper}");
    }

    #[test]
    fn no_class_at_the_caret_is_an_error_and_records_need_java_16() {
        let mut ctx = sample_context();
        ctx.class = None;
        assert!(DtoMapper.generate(&serde_json::json!({}), &ctx).is_err());
        let mut old = sample_context();
        old.project.java = 11;
        assert!(DtoMapper.generate(&serde_json::json!({ "dto_kind": "record" }), &old).is_err());
    }
}
