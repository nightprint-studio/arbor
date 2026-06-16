//! Canonical entry point for this crate's public API.
//! `use corvus_provider_descriptor::prelude::*;`

pub use crate::descriptor::{
    AuthField, AuthMethod, AuthMethodKind, AuthStatus, FieldHint, FieldMatch, FieldRule,
    FieldWidget, OAuthFlow, OAuthStart, ProviderDescriptor, ProviderDomain, ProviderUserInfo,
};
