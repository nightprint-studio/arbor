//! Finding scheduled work in a Java source.
//!
//! Only `@Scheduled` for now, and deliberately: Quartz's own triggers are built in code or declared
//! in XML whose shape varies per project, while `@Scheduled` is one annotation with three mutually
//! exclusive spellings — and it is what the overwhelming majority of scheduled work in a Spring
//! project is written as. [`crate::cron`] serves both, so a Quartz reader is an addition here
//! rather than a second dialect somewhere else.

use bennu_java::prelude::parse_java;
use tree_sitter::Node;

/// How a job says when it runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trigger {
    /// `cron = "…"`, with the byte span of the expression text.
    Cron { expression: String, start: usize, end: usize },
    /// `fixedRate` / `fixedDelay` / `fixedRateString` — a period in milliseconds.
    Fixed { attribute: String, value: String },
    /// The annotation is there and says nothing about when. Spring refuses this at startup.
    None,
}

/// One scheduled method.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    /// The method's name.
    pub method: String,
    /// The type declaring it.
    pub owner: String,
    pub trigger: Trigger,
    /// Byte span of the `@Scheduled` annotation.
    pub start: usize,
    pub end: usize,
    /// Byte offset of the method's name — the go-to target.
    pub offset: usize,
}

/// Every `@Scheduled` method in the source.
pub fn jobs_in(source: &str) -> Vec<Job> {
    if !source.contains("Scheduled") {
        return Vec::new();
    }
    let Some(tree) = parse_java(source) else { return Vec::new() };
    let mut out = Vec::new();
    walk(tree.root_node(), source, "", &mut out);
    out.sort_by_key(|j| j.start);
    out
}

fn walk(node: Node<'_>, source: &str, owner: &str, out: &mut Vec<Job>) {
    let mut cursor = node.walk();
    for child in node.named_children(&mut cursor) {
        let owner = if matches!(child.kind(), "class_declaration" | "record_declaration") {
            child
                .child_by_field_name("name")
                .map(|n| text(&n, source).to_string())
                .unwrap_or_else(|| owner.to_string())
        } else {
            owner.to_string()
        };
        if child.kind() == "method_declaration" {
            if let Some(job) = read_method(child, source, &owner) {
                out.push(job);
            }
        }
        walk(child, source, &owner, out);
    }
}

fn read_method(member: Node<'_>, source: &str, owner: &str) -> Option<Job> {
    let name_node = member.child_by_field_name("name")?;
    let modifiers = child_of_kind(member, "modifiers")?;
    let mut cursor = modifiers.walk();
    let annotation = modifiers.named_children(&mut cursor).find(|c| {
        matches!(c.kind(), "annotation" | "marker_annotation")
            && c.child_by_field_name("name")
                .map(|n| simple(text(&n, source)) == "Scheduled")
                .unwrap_or(false)
    })?;

    Some(Job {
        method: text(&name_node, source).to_string(),
        owner: owner.to_string(),
        trigger: trigger_of(annotation, source),
        start: annotation.start_byte(),
        end: annotation.end_byte(),
        offset: name_node.start_byte(),
    })
}

fn trigger_of(annotation: Node<'_>, source: &str) -> Trigger {
    let Some(args) = annotation.child_by_field_name("arguments") else { return Trigger::None };
    let mut cursor = args.walk();
    for child in args.named_children(&mut cursor) {
        if child.kind() != "element_value_pair" {
            continue;
        }
        let Some(key) = child.child_by_field_name("key") else { continue };
        let key = text(&key, source);
        let Some(value) = child.child_by_field_name("value") else { continue };
        match key {
            "cron" => {
                // Only a plain literal. A constant reference resolves to something this cannot
                // read, and judging it would be judging a value nobody here has seen.
                if value.kind() != "string_literal" {
                    return Trigger::None;
                }
                let raw = text(&value, source);
                if raw.len() < 2 || !raw.starts_with('"') {
                    return Trigger::None;
                }
                return Trigger::Cron {
                    expression: raw[1..raw.len() - 1].to_string(),
                    start: value.start_byte() + 1,
                    end: value.end_byte() - 1,
                };
            }
            "fixedRate" | "fixedDelay" | "fixedRateString" | "fixedDelayString" => {
                return Trigger::Fixed {
                    attribute: key.to_string(),
                    value: text(&value, source).trim_matches('"').to_string(),
                };
            }
            _ => {}
        }
    }
    Trigger::None
}

/// Whether a source switches Spring's scheduling on.
///
/// `@EnableScheduling` is the explicit switch; a Spring Boot application class turns it on through
/// autoconfiguration **only** when the annotation is present somewhere, so `@SpringBootApplication`
/// alone is not enough — which is exactly the trap. What IS enough is a `SchedulerFactoryBean` or
/// a `TaskScheduler` the project builds itself, because then the jobs are wired by hand.
pub fn enables_scheduling(text: &str) -> bool {
    text.contains("@EnableScheduling")
        || text.contains("EnableScheduling.class")
        || text.contains("SchedulerFactoryBean")
        || text.contains("ScheduledAnnotationBeanPostProcessor")
        || text.contains("<task:annotation-driven")
}

fn simple(name: &str) -> &str {
    name.rsplit('.').next().unwrap_or(name)
}

fn child_of_kind<'t>(node: Node<'t>, kind: &str) -> Option<Node<'t>> {
    let mut cursor = node.walk();
    let found = node.named_children(&mut cursor).find(|c| c.kind() == kind);
    found
}

fn text<'a>(node: &Node<'_>, source: &'a str) -> &'a str {
    source.get(node.start_byte()..node.end_byte()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: &str = r#"@Component
public class Reports {

    @Scheduled(cron = "0 0 2 * * ?")
    public void nightly() { }

    @Scheduled(fixedDelay = 60000)
    public void poll() { }

    @Scheduled(cron = CRON_CONSTANT)
    public void external() { }

    public void notScheduled() { }
}
"#;

    #[test]
    fn every_scheduled_method_is_found_with_its_owner() {
        let found = jobs_in(SRC);
        let names: Vec<&str> = found.iter().map(|j| j.method.as_str()).collect();
        assert_eq!(names, ["nightly", "poll", "external"]);
        assert_eq!(found[0].owner, "Reports");
    }

    #[test]
    fn a_cron_carries_the_span_of_the_expression_itself() {
        let found = jobs_in(SRC);
        let Trigger::Cron { expression, start, end } = &found[0].trigger else {
            panic!("a cron, got {:?}", found[0].trigger)
        };
        assert_eq!(expression, "0 0 2 * * ?");
        assert_eq!(&SRC[*start..*end], "0 0 2 * * ?");
    }

    #[test]
    fn a_fixed_period_is_read_as_one() {
        let found = jobs_in(SRC);
        assert_eq!(
            found[1].trigger,
            Trigger::Fixed { attribute: "fixedDelay".into(), value: "60000".into() }
        );
    }

    /// A constant reference resolves to something this cannot read, and judging it would be
    /// judging a value nobody here has seen.
    #[test]
    fn a_cron_that_is_not_a_literal_is_not_judged() {
        let found = jobs_in(SRC);
        assert_eq!(found[2].trigger, Trigger::None);
    }

    #[test]
    fn scheduling_is_recognised_in_every_place_it_is_switched_on() {
        assert!(enables_scheduling("@EnableScheduling\npublic class App {}"));
        assert!(enables_scheduling("<task:annotation-driven scheduler=\"s\"/>"));
        assert!(enables_scheduling("new SchedulerFactoryBean()"));
        // The trap: a Boot application alone does NOT switch it on.
        assert!(!enables_scheduling("@SpringBootApplication\npublic class App {}"));
    }

    #[test]
    fn a_file_with_no_scheduling_is_not_even_parsed() {
        assert!(jobs_in("class A { void go() { } }").is_empty());
    }
}
