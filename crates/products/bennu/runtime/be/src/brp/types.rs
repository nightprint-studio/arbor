//! The game's registered types — the vocabulary the inspector's "add component" picker and typed
//! editors are built from.
//!
//! Everything is answered from the registry read once at connect time (`registry.schema`), so none
//! of these handlers touch the network. A game that withheld its registry gets empty answers and the
//! editors fall back to raw JSON.

use arbor_brp::prelude::{skeleton, Shape, TypeFilter, TypeInfo};
use bennu_core::prelude::BennuState;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::live;

const DEFAULT_PAGE: usize = 100;
const MAX_PAGE: usize = 1_000;

/// Args for [`bennu_brp_types`].
#[derive(Deserialize)]
pub struct TypesArgs {
    /// `component`, `resource`, or absent for everything registered.
    #[serde(default)]
    pub kind: Option<String>,
    /// Substring of the type path; a short name that starts with it ranks first.
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub offset: usize,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Serialize)]
pub struct TypesDto {
    pub total: usize,
    pub items: Vec<TypeInfo>,
}

/// Args for [`bennu_brp_type_shape`].
#[derive(Deserialize)]
pub struct ShapeArgs {
    pub type_path: String,
}

#[derive(Serialize)]
pub struct ShapeDto {
    pub shape: Shape,
    /// A well-formed placeholder value of this type — where "insert" starts from.
    pub skeleton: Value,
    pub info: Option<TypeInfo>,
}

/// A page of the registered types.
#[arbor_rpc::handler]
async fn bennu_brp_types(_ctx: &BennuState, args: TypesArgs) -> Result<TypesDto, String> {
    let live = live()?;
    let filter = TypeFilter::parse(args.kind.as_deref());
    let limit = args.limit.unwrap_or(DEFAULT_PAGE).clamp(1, MAX_PAGE);
    let (total, page) = live.registry.search(
        filter,
        args.query.as_deref().unwrap_or(""),
        args.offset,
        limit,
    );
    Ok(TypesDto { total, items: page.into_iter().cloned().collect() })
}

/// The editor shape of one type, and a skeleton value for it.
#[arbor_rpc::handler]
async fn bennu_brp_type_shape(_ctx: &BennuState, args: ShapeArgs) -> Result<ShapeDto, String> {
    let live = live()?;
    let shape = live.registry.shape(&args.type_path);
    Ok(ShapeDto {
        skeleton: skeleton(&shape),
        info: live.registry.info(&args.type_path).cloned(),
        shape,
    })
}
