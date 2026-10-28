//! A ranking benchmark: where the member the author MEANT lands in the list, over a small corpus,
//! with the smart signals off and on.
//!
//! ## Why this exists
//!
//! The ranking is a handful of weights, and a weight cannot be judged by reading it. Each case here
//! is one completion the author would really ask — the caret after a `receiver.` with the member
//! that was actually written known — and the report is where that member came out: the top-1 and
//! top-3 rates, and the mean reciprocal rank (`1/rank`, so being first is worth 1, second 0.5, and
//! absent 0). A change to a weight, a new signal or a new tie-break is then a number moving, in one
//! direction or the other, instead of an argument.
//!
//! The corpus is deliberately the cases the TYPE cannot settle — several members of the same type,
//! alphabetically in the wrong order — because those are the only ones a signal other than the
//! type can change. Add a case whenever a completion in real use came out in the wrong place.
//!
//! ## Why one test
//!
//! The tuning is a process-wide value and this test flips it. Integration test files are separate
//! processes, so no other file is affected; inside this file there is exactly one `#[test]`, so
//! there is nothing to race with.

mod common;
use bennu_query::prelude::{set_completion_tuning, Tuning};
use common::*;

/// One completion: the text before the caret (the caret goes at the `|`), and the label that was
/// meant.
struct Case {
    name: &'static str,
    body: &'static str,
    meant: &'static str,
}

const CASES: &[Case] = &[
    // The type cannot decide: every member below returns what the declaration wants, and the
    // alphabet puts the wrong one first. The hole's name — a partial echo, `customerName` for
    // `name` — does. (An EXACT echo, `String name = c.|`, is already lifted by the buffer's own use
    // count, so those cases are in the "settled" group below: they must simply not get worse.)
    Case { name: "string named customerName", body: "String customerName = c.|", meant: "name" },
    Case { name: "long named orderTotal", body: "long orderTotal = o.|", meant: "total" },
    Case { name: "string named mobilePhone", body: "String mobilePhone = c.|", meant: "phone" },
    Case { name: "nested string named mainStreet", body: "String mainStreet = c.address().|", meant: "street" },
    Case { name: "nested string named postalZip", body: "String postalZip = c.address().|", meant: "zip" },
    // An exact echo: the use count already settles these.
    Case { name: "string named name", body: "String name = c.|", meant: "name" },
    Case { name: "string named vendor", body: "String vendor = o.|", meant: "vendor" },
    Case { name: "long named tax", body: "long tax = o.|", meant: "tax" },
    // The type decides on its own: signals must not make these worse.
    Case { name: "customer for a customer", body: "Customer buyer = o.|", meant: "customer" },
    Case { name: "boolean declaration", body: "boolean ok = o.|", meant: "paid" },
    Case { name: "int declaration", body: "int n = o.|", meant: "count" },
    Case { name: "condition", body: "if (o.|) { }", meant: "paid" },
    Case { name: "typed prefix", body: "String t = o.ti|", meant: "title" },
    // Only a chain can answer: the right member is a hop away.
    Case { name: "string through the customer", body: "String email = o.|", meant: "customer().email()" },
];

const DOMAIN: &[(&str, &str)] = &[
    (
        "Order.java",
        "package shop;\n\
         public class Order {\n\
         \x20   public String title() { return \"\"; }\n\
         \x20   public String vendor() { return \"\"; }\n\
         \x20   public int count() { return 0; }\n\
         \x20   public boolean paid() { return true; }\n\
         \x20   public long tax() { return 0; }\n\
         \x20   public long total() { return 0; }\n\
         \x20   public Customer customer() { return null; }\n\
         \x20   public void cancel() { }\n\
         }\n",
    ),
    (
        "Customer.java",
        "package shop;\n\
         public class Customer {\n\
         \x20   public String email() { return \"\"; }\n\
         \x20   public String name() { return \"\"; }\n\
         \x20   public String phone() { return \"\"; }\n\
         \x20   public Address address() { return null; }\n\
         }\n",
    ),
    (
        "Address.java",
        "package shop;\n\
         public class Address {\n\
         \x20   public String city() { return \"\"; }\n\
         \x20   public String street() { return \"\"; }\n\
         \x20   public String zip() { return \"\"; }\n\
         }\n",
    ),
];

/// Where each case came out: `usize::MAX` when the meant member was not offered at all.
struct Report {
    ranks: Vec<usize>,
}

impl Report {
    fn mrr(&self) -> f64 {
        let sum: f64 = self
            .ranks
            .iter()
            .map(|&r| if r == usize::MAX { 0.0 } else { 1.0 / (r as f64 + 1.0) })
            .sum();
        sum / self.ranks.len() as f64
    }

    fn hit_at(&self, k: usize) -> f64 {
        self.ranks.iter().filter(|&&r| r < k).count() as f64 / self.ranks.len() as f64
    }

    fn render(&self, title: &str) -> String {
        let cells: Vec<String> = CASES
            .iter()
            .zip(&self.ranks)
            .map(|(case, &r)| {
                let at = if r == usize::MAX { "absent".to_string() } else { format!("#{}", r + 1) };
                format!("    {:<30} {at}", case.name)
            })
            .collect();
        format!(
            "{title}: top-1 {:.0}%  top-3 {:.0}%  MRR {:.3}\n{}",
            self.hit_at(1) * 100.0,
            self.hit_at(3) * 100.0,
            self.mrr(),
            cells.join("\n")
        )
    }
}

/// Every case in its own file of one project — one trigger per file, since a dangling `receiver.`
/// is a syntax error whose recovery can swallow a neighbour.
fn corpus() -> (Project, Vec<(String, usize)>) {
    let mut files: Vec<(String, String)> =
        DOMAIN.iter().map(|(n, s)| (n.to_string(), s.to_string())).collect();
    let mut carets = Vec::new();
    for (i, case) in CASES.iter().enumerate() {
        let file = format!("Use{i}.java");
        let head = format!(
            "package shop;\npublic class Use{i} {{\n    public void run(Order o, Customer c) {{\n        "
        );
        let marker = case.body.find('|').expect("every case marks its caret with |");
        let body = case.body.replacen('|', "", 1);
        carets.push((file.clone(), head.len() + marker));
        files.push((file, format!("{head}{body}\n    }}\n}}\n")));
    }
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(n, s)| (n.as_str(), s.as_str())).collect();
    (Project::new(&borrowed), carets)
}

fn evaluate(project: &Project, carets: &[(String, usize)]) -> Report {
    let ranks = CASES
        .iter()
        .zip(carets)
        .map(|(case, (file, offset))| {
            let labels = project.complete_labels(file, *offset);
            labels.iter().position(|l| l == case.meant).unwrap_or(usize::MAX)
        })
        .collect();
    Report { ranks }
}

fn rank_of(report: &Report, case_name: &str) -> usize {
    let at = CASES.iter().position(|c| c.name == case_name).expect("a case of that name");
    report.ranks[at]
}

#[test]
fn the_smart_signals_improve_the_ranking_and_never_make_it_worse() {
    let (project, carets) = corpus();

    set_completion_tuning(Tuning::OFF);
    let off = evaluate(&project, &carets);
    set_completion_tuning(Tuning::DEFAULT);
    let on = evaluate(&project, &carets);

    // The numbers are the point: they print when the test fails, and with `--nocapture` when it
    // passes, so a tuning session is a loop of editing a weight and reading these two blocks.
    eprintln!("{}\n{}", off.render("signals off"), on.render("signals on"));

    // The corpus is sound: every member that was meant is offered when the signals are on.
    for (case, rank) in CASES.iter().zip(&on.ranks) {
        assert_ne!(*rank, usize::MAX, "`{}` was not offered for `{}`\n{}", case.meant, case.name, on.render("on"));
    }

    // Name affinity decides what the type cannot: the meant member beats its alphabetical rivals.
    for decided_by_the_name in [
        "string named customerName",
        "long named orderTotal",
        "string named mobilePhone",
        "nested string named mainStreet",
        "nested string named postalZip",
    ] {
        let (before, after) = (rank_of(&off, decided_by_the_name), rank_of(&on, decided_by_the_name));
        assert!(after < before, "{decided_by_the_name}: #{} with signals off, #{} on", before + 1, after + 1);
    }

    // The cases the type already settles do not get worse.
    for settled in [
        "string named name",
        "string named vendor",
        "long named tax",
        "customer for a customer",
        "boolean declaration",
        "int declaration",
        "condition",
        "typed prefix",
    ] {
        assert!(
            rank_of(&on, settled) <= rank_of(&off, settled),
            "{settled} got worse: #{} off, #{} on",
            rank_of(&off, settled) + 1,
            rank_of(&on, settled) + 1
        );
    }

    // Only a chain reaches the last case, and it must come out in the first screenful.
    assert_eq!(rank_of(&off, "string through the customer"), usize::MAX);
    assert!(rank_of(&on, "string through the customer") < 10);

    // The aggregate: better on average, and good in absolute terms. These floors are conservative
    // on purpose — they are there to catch a regression, not to pin a tuning.
    assert!(on.mrr() > off.mrr(), "MRR {:.3} on vs {:.3} off", on.mrr(), off.mrr());
    assert!(on.hit_at(3) >= 0.75, "top-3 rate {:.2}", on.hit_at(3));

    // Leave the process the way every other test file expects to find it.
    set_completion_tuning(Tuning::DEFAULT);
}
