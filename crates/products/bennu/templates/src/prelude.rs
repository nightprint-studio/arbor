//! Canonical entry point for `bennu-templates`' public API.

pub use crate::contexts::{
    builtins, ClassTemplateContext, ConfigPropertiesContext, ConfigProperty, LiveContext,
    NewFileContext, YamlLine,
};
pub use crate::config_class::{class_name_for, ConfigClassContext, ConfigField, ConfigType};
pub use crate::config_keys::{common_group, key_at, key_nodes, keys_in, narrow, relative_to, KeyNode};
pub use crate::config_props::read_properties;
pub use crate::config_tree::{canonical, read_config, ConfigValue};
pub use crate::config_yaml::read_yaml;
pub use crate::dates::today;
pub use crate::facts::{with_fact_schemas, ProjectFacts, StyleFacts, TemplateFacts};
pub use crate::line_map::{carry_lines, trace_lines, LineTrace};
pub use crate::style::STYLE_METHODS;
pub use crate::requires::unmet;
pub use crate::engine::{render, render_code, render_with, schema_of, Directives, Output, CUSTOM_FILTERS};
pub use crate::naming::{naming_members, NamingFacts};
pub use crate::kind::{Builtin, TemplateKind};
pub use crate::model::{
    class_at, class_named, type_simple, AnnotationModel, ClassModel, ConstraintModel, FieldModel,
};
pub use crate::names::{camel, java_string, pascal, snake};
pub use crate::placement::{insert_members, types_in, Insertion, TypeInFile};
pub use crate::store::{check_template_name, create_template, delete_template, kind_dir, list_templates, load_template, rename_template, set_directive, split_file_name, Template, TEMPLATE_EXTENSION, template_file_name, TemplateInfo, TemplateOrigin};
