//! The extension — what a host registers.

use std::sync::atomic::{AtomicBool, Ordering};

use bennu_ext::prelude::{ExtHighlight, FileCtx, FrameworkExtension, ProjectScan};
use bennu_proto::prelude::CapabilitySet;

use crate::highlight;

#[derive(Default)]
pub struct MyBatisExtension {
    scanned: AtomicBool,
}

impl MyBatisExtension {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameworkExtension for MyBatisExtension {
    fn id(&self) -> &'static str {
        "mybatis"
    }

    fn display_name(&self) -> &'static str {
        "MyBatis"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.mybatis_mapper
    }

    fn reindex(&self, _scan: &ProjectScan<'_>) {
        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    fn highlights(&self, ctx: &FileCtx<'_>) -> Vec<ExtHighlight> {
        if ctx.extension() != "xml" {
            return Vec::new();
        }
        highlight::highlights(ctx.source)
    }
}
