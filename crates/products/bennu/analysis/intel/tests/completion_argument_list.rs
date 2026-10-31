//! The whole argument list of a call as ghost text: `transfer(` → `from, to, amount);`, only when every slot has a
//! confident choice, and closed with `);` only where the call is the whole statement.

mod common;
use common::*;

fn project(body: &str) -> Project {
    Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        ("Currency.java", "package z;\npublic enum Currency { USD, EUR, GBP }\n"),
        (
            "Bank.java",
            "package z;\npublic class Bank {\n    public long transfer(Account from, Account to, long amount) { return 0; }\n    public void charge(Account who, Currency currency) { }\n    public void log(long v) { }\n    public void note(String a) { }\n    public void note(Account from, Account to) { }\n}\n",
        ),
        ("Use.java", &format!("package z;\npublic class Use {{\n    private int eur;\n    void run(Bank bank, Account from, Account to, long amount) {{\n        {body}\n    }}\n}}\n")),
    ])
}

/// The proposal with the caret just after `trigger`, whose last character is the `(`.
fn proposal(body: &str, trigger: &str) -> Option<bennu_query::prelude::ArgumentList> {
    let p = project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.argument_list("Use.java", off)
}

#[test]
fn every_slot_filled_by_type_and_name_closes_the_statement() {
    let got = proposal("bank.transfer(\n", "bank.transfer(").expect("a list");
    assert_eq!(got.text, "from, to, amount);");
    assert!(got.confidence > 0.8, "{}", got.confidence);
}

#[test]
fn inside_a_bigger_expression_only_the_parenthesis_is_closed() {
    assert_eq!(proposal("bank.log(bank.transfer(\n", "bank.log(bank.transfer(").unwrap().text, "from, to, amount)");
}

#[test]
fn an_initialiser_is_a_statement_too() {
    assert_eq!(proposal("long done = bank.transfer(\n", "long done = bank.transfer(").unwrap().text, "from, to, amount);");
}

#[test]
fn two_names_that_fit_equally_are_offered_in_declaration_order_as_a_guess() {
    // two Accounts, and the names say nothing about which is `payer` and which is `payee`: `x, y`, drawn with 0.5
    let p = Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        ("Bank.java", "package z;\npublic class Bank {\n    public void pay(Account payer, Account payee) { }\n}\n"),
        ("Use.java", "package z;\npublic class Use {\n    void run(Bank bank, Account x, Account y) {\n        bank.pay(\n    }\n}\n"),
    ]);
    let off = at(p.source("Use.java"), "bank.pay(") + "bank.pay(".len();
    let got = p.argument_list("Use.java", off).expect("a guess");
    assert!(got.text.starts_with("x, y"), "{got:?}");
    assert!(got.confidence <= 0.5 + 1e-6, "{got:?}");
}

#[test]
fn an_enum_slot_takes_the_constant_the_names_around_it_point_at() {
    let got = proposal("bank.charge(\n", "bank.charge(");
    // `who` resembles neither Account: the first declared is offered, as a guess (0.5), and the enum slot still gets its constant
    let got = got.expect("a guess");
    assert!(got.text.starts_with("from, Currency."), "{got:?}");
    assert!(got.confidence <= 0.5 + 1e-6, "{got:?}");
    let p = Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        ("Currency.java", "package z;\npublic enum Currency { USD, EUR, GBP }\n"),
        ("Bank.java", "package z;\npublic class Bank {\n    public void charge(Account account, Currency currency) { }\n}\n"),
        ("Use.java", "package z;\npublic class Use {\n    private int eur;\n    void run(Bank bank, Account account) {\n        bank.charge(\n    }\n}\n"),
    ]);
    let off = at(p.source("Use.java"), "bank.charge(") + "bank.charge(".len();
    assert_eq!(p.argument_list("Use.java", off).unwrap().text, "account, Currency.EUR);");
}

#[test]
fn text_after_the_parenthesis_or_an_overload_tie_means_none() {
    assert!(proposal("bank.transfer()\n", "bank.transfer(").is_none(), "the rest of the line is not empty");
    // `note` has an overload with one String and one with two Accounts: with no String in scope only the second fills
    let got = proposal("bank.note(\n", "bank.note(");
    assert_eq!(got.map(|g| g.text), Some("from, to);".to_string()));
}

#[test]
fn the_variable_the_project_usually_passes_wins_between_two_equal_names() {
    // two Accounts and parameters that resemble neither: without a habit `x, y`; the project passes `y` first
    let p = Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        ("Bank.java", "package z;\npublic class Bank {\n    public void pay(Account payer, Account payee) { }\n}\n"),
        ("Use.java", "package z;\npublic class Use {\n    void run(Bank bank, Account x, Account y) {\n        bank.pay(\n    }\n}\n"),
    ]);
    let off = at(p.source("Use.java"), "bank.pay(") + "bank.pay(".len();
    assert!(p.argument_list("Use.java", off).unwrap().text.starts_with("x, y"));
    let habit = |callee: &str, index: usize| (callee == "pay" && index == 0).then(|| "y".to_string());
    let got = p.argument_list_with("Use.java", off, &habit).expect("a list");
    assert!(got.text.starts_with("y, x"), "{got:?}");
    // a habit for another method changes nothing
    let other = |callee: &str, _: usize| (callee == "charge").then(|| "y".to_string());
    assert!(p.argument_list_with("Use.java", off, &other).unwrap().text.starts_with("x, y"));
}

#[test]
fn the_overload_chosen_in_the_list_decides_between_overloads_and_an_empty_one() {
    // `show()` has no parameters, which alone made the call unanswerable; the chosen overload is the one with two
    let p = Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        ("Bank.java", "package z;\npublic class Bank {\n    public void show() { }\n    public void show(Account from, Account to) { }\n    public void show(Account who) { }\n}\n"),
        ("Use.java", "package z;\npublic class Use {\n    void run(Bank bank, Account from, Account to) {\n        bank.show(\n    }\n}\n"),
    ]);
    let off = at(p.source("Use.java"), "bank.show(") + "bank.show(".len();
    assert!(p.argument_list("Use.java", off).is_none(), "without a choice the overloads disagree");
    let two = p.argument_list_picked("Use.java", off, &["Account".to_string(), "Account".to_string()]).expect("the chosen overload");
    assert_eq!(two.text, "from, to);");
    let one = p.argument_list_picked("Use.java", off, &["Account".to_string()]).expect("the other one");
    assert!(!one.text.contains(','), "{one:?}");
}

fn lambda_project(body: &str) -> Project {
    Project::new(&[
        ("Account.java", "package z;\npublic class Account { }\n"),
        ("Each.java", "package z;\npublic interface Each { void run(String s); }\n"),
        (
            "Bank.java",
            "package z;\npublic class Bank {\n    public void each(Each e) { }\n    public void tag(String s) { }\n    public void count(int n) { }\n    public void pay(Account a) { }\n}\n",
        ),
        ("Use.java", &format!("package z;\npublic class Use {{\n    void run(Bank bank) {{\n        {body}\n    }}\n}}\n")),
    ])
}

fn in_lambda(body: &str, trigger: &str) -> Option<bennu_query::prelude::ArgumentList> {
    let p = lambda_project(body);
    let off = at(p.source("Use.java"), trigger) + trigger.len();
    p.argument_list("Use.java", off)
}

/// Inside a lambda, with nothing of a known type to hand, the lambda's own parameter is the guess: it is what the body
/// is about, and one keystroke away from being changed.
#[test]
fn the_parameter_of_the_lambda_is_the_guess_where_nothing_else_fits() {
    let got = in_lambda("bank.each(x -> {\n            bank.tag(\n        });", "bank.tag(").expect("a list");
    assert_eq!(got.text, "x);");
}

#[test]
fn a_parameter_that_says_it_is_unused_is_not_offered() {
    assert!(in_lambda("bank.each(_s -> {\n            bank.tag(\n        });", "bank.tag(").is_none());
    assert!(in_lambda("bank.each(_ -> {\n            bank.tag(\n        });", "bank.tag(").is_none());
}

#[test]
fn a_primitive_slot_is_not_given_a_guess_of_unknown_type() {
    assert!(in_lambda("bank.each(x -> {\n            bank.count(\n        });", "bank.count(").is_none());
}

/// The lambda's parameter is typed by the interface it is passed to, so it is offered where that type fits — and is NOT
/// the guess for a slot of another type: a `String` is not an `Account`, and an unknown type is the only thing a guess is for.
#[test]
fn a_lambda_parameter_of_a_known_type_is_not_offered_where_it_does_not_fit() {
    assert!(in_lambda("bank.each(x -> {\n            bank.pay(\n        });", "bank.pay(").is_none());
}

/// After the first argument and its comma, the rest is filled by type: the user may have been unsure about the first, the
/// second is certain.
#[test]
fn the_arguments_after_a_comma_are_filled_whatever_the_first_was() {
    let p = project("bank.transfer(null, \n");
    let off = at(p.source("Use.java"), "bank.transfer(null, ") + "bank.transfer(null, ".len();
    let got = p.argument_rest("Use.java", off).expect("the rest");
    assert_eq!(got.text, "to, amount);");
}
