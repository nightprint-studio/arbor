//! The extension — what a host registers.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use bennu_ext::prelude::{
    ExtEntry, ExtHover, ExtStat, ExtTarget, FileCtx, FrameworkExtension, ProjectScan,
};
use bennu_proto::prelude::{CapabilitySet, Diagnostic};

use crate::cron::{check, describe, Dialect};
use crate::jobs::{enables_scheduling, jobs_in, Job, Trigger};

pub const CODE_BAD_CRON: &str = "scheduling.bad-cron";
pub const CODE_NOT_ENABLED: &str = "scheduling.not-enabled";
pub const CODE_NO_TRIGGER: &str = "scheduling.no-trigger";

#[derive(Default)]
pub struct SchedulingExtension {
    /// Whether the project switches scheduling on **anywhere**. The single project-wide fact this
    /// extension needs, and the one that makes the second check possible at all.
    enabled: AtomicBool,
    /// Every scheduled method in the project, for the catalogue.
    catalogue: RwLock<Arc<Vec<ExtEntry>>>,
    scanned: AtomicBool,
}

impl SchedulingExtension {
    pub fn new() -> Self {
        Self::default()
    }
}

impl FrameworkExtension for SchedulingExtension {
    fn id(&self) -> &'static str {
        "scheduling"
    }

    fn display_name(&self) -> &'static str {
        "Scheduled jobs"
    }

    fn applies(&self, caps: &CapabilitySet) -> bool {
        caps.scheduling
    }

    fn reindex(&self, scan: &ProjectScan<'_>) {
        let enabled = scan
            .java
            .iter()
            .chain(scan.xml.iter())
            .any(|f| enables_scheduling(&f.text));
        self.enabled.store(enabled, Ordering::Release);

        let mut rows: Vec<ExtEntry> = Vec::new();
        for file in scan.java {
            let path = file.path.to_string_lossy().replace('\\', "/");
            for job in jobs_in(&file.text) {
                rows.push(entry_for(&job, &path, &file.text));
            }
        }
        rows.sort_by(|a, b| a.primary.cmp(&b.primary));
        if let Ok(mut slot) = self.catalogue.write() {
            *slot = Arc::new(rows);
        }
        self.scanned.store(true, Ordering::Release);
    }

    fn is_ready(&self) -> bool {
        self.scanned.load(Ordering::Acquire)
    }

    /// The two ways a job never runs, plus the annotation that says nothing.
    ///
    /// The "not enabled" one is reported on **every** `@Scheduled` in the file rather than once
    /// somewhere central, and deliberately: there is no central place to look at, and the person
    /// who needs to know is the one reading the method that is not being called.
    fn diagnostics(&self, ctx: &FileCtx<'_>) -> Vec<Diagnostic> {
        if ctx.extension() != "java" {
            return Vec::new();
        }
        let jobs = jobs_in(ctx.source);
        if jobs.is_empty() {
            return Vec::new();
        }
        // Before a scan has landed, nothing is known about the project — and reporting "scheduling
        // is off" from a cold start would be reporting our own state as the project's.
        let scanned = self.scanned.load(Ordering::Acquire);
        let enabled = self.enabled.load(Ordering::Acquire);

        let mut out = Vec::new();
        for job in &jobs {
            if scanned && !enabled {
                out.push(Diagnostic {
                    message: format!(
                        "nothing switches scheduling on in this project, so `{}` is never called \
                         — add @EnableScheduling to a @Configuration class",
                        job.method
                    ),
                    severity: "warning".to_string(),
                    code: CODE_NOT_ENABLED.to_string(),
                    start: job.start,
                    end: job.end,
                });
            }
            match &job.trigger {
                Trigger::Cron { expression, start, end } => {
                    if let Err(bad) = check(expression, Dialect::Quartz) {
                        out.push(Diagnostic {
                            message: format!(
                                "this is not a cron expression Spring will accept: {} — the \
                                 context fails to start",
                                bad.message
                            ),
                            severity: "error".to_string(),
                            code: CODE_BAD_CRON.to_string(),
                            start: *start,
                            end: *end,
                        });
                    }
                }
                Trigger::None => out.push(Diagnostic {
                    message: format!(
                        "@Scheduled on `{}` says nothing about when — exactly one of cron, \
                         fixedRate or fixedDelay is required, and Spring refuses the bean without \
                         one",
                        job.method
                    ),
                    severity: "warning".to_string(),
                    code: CODE_NO_TRIGGER.to_string(),
                    start: job.start,
                    end: job.end,
                }),
                Trigger::Fixed { .. } => {}
            }
        }
        out
    }

    /// The expression in words. The reason the crate is worth having open in front of you rather
    /// than only in a report.
    fn hover(&self, ctx: &FileCtx<'_>, offset: usize) -> Option<ExtHover> {
        if ctx.extension() != "java" {
            return None;
        }
        let jobs = jobs_in(ctx.source);
        let job = jobs.iter().find(|j| offset >= j.start && offset <= j.end)?;
        let Trigger::Cron { expression, .. } = &job.trigger else { return None };
        let words = describe(expression, Dialect::Quartz)?;
        Some(ExtHover {
            title: words,
            signature: expression.clone(),
            doc: format!("`{}` runs on this schedule.", job.method),
        })
    }

    /// Every scheduled method, so "what does this application do on its own" is a list rather than
    /// a grep.
    fn navigate(&self, _ctx: &FileCtx<'_>, _offset: usize) -> Vec<ExtTarget> {
        Vec::new()
    }

    fn catalog(&self, kind: &str) -> Vec<ExtEntry> {
        if kind != "jobs" {
            return Vec::new();
        }
        self.catalogue.read().map(|c| (**c).clone()).unwrap_or_default()
    }

    fn stats(&self) -> Vec<ExtStat> {
        let jobs = self.catalogue.read().map(|c| c.len()).unwrap_or(0);
        vec![ExtStat {
            label: "Scheduled jobs".to_string(),
            value: jobs,
            catalog: Some("jobs".to_string()),
        }]
    }
}

fn entry_for(job: &Job, path: &str, text: &str) -> ExtEntry {
    let (secondary, tags) = match &job.trigger {
        Trigger::Cron { expression, .. } => (
            // The words when there are any, the expression when there are not — the column is
            // there to be read, and a raw cron is the thing nobody can read.
            describe(expression, Dialect::Quartz).unwrap_or_else(|| expression.clone()),
            vec![expression.clone()],
        ),
        Trigger::Fixed { attribute, value } => {
            (format!("{attribute} = {value}"), vec!["fixed".to_string()])
        }
        Trigger::None => ("no trigger".to_string(), vec!["incomplete".to_string()]),
    };
    ExtEntry {
        id: format!("{}#{}", job.owner, job.method),
        primary: format!("{}.{}", job.owner, job.method),
        secondary,
        kind: "@Scheduled".to_string(),
        file: Some(path.to_string()),
        offset: Some(job.offset),
        line: Some(text[..job.offset.min(text.len())].bytes().filter(|&b| b == b'\n').count() as u32 + 1),
        tags,
        children: Vec::new(),
    }
}
