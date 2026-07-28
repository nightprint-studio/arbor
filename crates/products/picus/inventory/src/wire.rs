//! [`InventoryObject`] — the shape that crosses the RPC seam.
//!
//! Field-for-field with `InventoryObject` in `src/lib/types/picus/index.ts`, and
//! deliberately smaller than [`ObjectEntry`]: the sites are backend detail that
//! the inventory table does not draw, and shipping them would multiply the
//! payload of a large repository by the number of times each object is named.
//!
//! `Serialize` only. Nothing sends an inventory back — it is derived from the
//! files, and the files are the truth.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::entry::ObjectEntry;
use crate::kind::InventoryKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InventoryObject {
    pub name: String,
    pub kind: InventoryKind,
    /// Keyed `"<branchId>/<folderId>"`. `0` means the folder exists and does
    /// nothing with this object, which is the value the interface highlights.
    pub coverage: BTreeMap<String, usize>,
}

impl InventoryObject {
    pub fn from_entry(entry: &ObjectEntry) -> InventoryObject {
        InventoryObject {
            name: entry.name.clone(),
            kind: entry.kind,
            coverage: entry.coverage.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wire_shape_is_what_the_interface_reads() {
        let object = InventoryObject {
            name: "PARAMETRI".to_string(),
            kind: InventoryKind::Table,
            coverage: [("ora/ora-init".to_string(), 1usize), ("pg/pg-upd".to_string(), 0)]
                .into_iter()
                .collect(),
        };
        let json = serde_json::to_string(&object).unwrap();
        assert_eq!(
            json,
            r#"{"name":"PARAMETRI","kind":"table","coverage":{"ora/ora-init":1,"pg/pg-upd":0}}"#
        );
    }
}
