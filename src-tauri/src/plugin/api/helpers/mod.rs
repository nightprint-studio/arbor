//! Shim — the helpers themselves migrated to
//! [`arbor_plugin_core::lua_api::helpers`] in PR #4 Step 5. The submodule
//! tree here is preserved as `pub use` re-exports so the src-tauri ns/*
//! files keep their existing import paths until they migrate into
//! plugin-core (Step 6) and import the helpers directly.

pub(crate) mod contrib_write {
    pub(crate) use arbor_plugin_core::lua_api::helpers::contrib_write::*;
}
pub(crate) mod convert {
    pub(crate) use arbor_plugin_core::lua_api::helpers::convert::*;
}
pub(crate) mod fs_perm {
    pub(crate) use arbor_plugin_core::lua_api::helpers::fs_perm::*;
}
pub(crate) mod glob {
    pub(crate) use arbor_plugin_core::lua_api::helpers::glob::*;
}
pub(crate) mod http_worker {
    pub(crate) use arbor_plugin_core::lua_api::helpers::http_worker::*;
}
pub(crate) mod json_patch {
    pub(crate) use arbor_plugin_core::lua_api::helpers::json_patch::*;
}
pub(crate) mod settings_scope {
    pub(crate) use arbor_plugin_core::lua_api::helpers::settings_scope::*;
}
pub(crate) mod timer {
    pub(crate) use arbor_plugin_core::lua_api::helpers::timer::*;
}
pub(crate) mod tuple {
    pub(crate) use arbor_plugin_core::lua_api::helpers::tuple::*;
}
pub(crate) mod xml_patch {
    pub(crate) use arbor_plugin_core::lua_api::helpers::xml_patch::*;
}
