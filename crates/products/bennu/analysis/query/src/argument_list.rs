//! The **whole argument list** of a call, as ghost text: `transfer(` → `from, to, amount);`.
//!
//! The pieces already exist one slot at a time — the callee's parameters (type and name), the names in scope, how
//! well a name resembles a parameter. Here they are asked for every slot at once, and the answer is given only when
//! *every* slot has a confident choice: a type that fits and a name that resembles the parameter's (or the only name
//! of that type), a constant of an enum slot that clearly leads. One hole and there is no proposal at all — a list
//! with a gap in it is a worse start than nothing. Literals are never invented.
//!
//! Overloads are tried each as a whole list. The one that fills best wins; a tie is no answer.

use bennu_java::prelude::{TypeRef, TypeResolver};

use crate::argument_locals::{order, scope_candidates, Candidate, Origin};
use crate::enum_arguments::clear_constant;
use crate::hints::{call_site, Overload};
use crate::rank;

/// The text to draw after the `(`, and how sure the choice is (0..1).
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentList {
    pub text: String,
    pub confidence: f32,
}

/// What a slot was filled with, and how it was decided.
#[derive(Debug, Clone, PartialEq)]
struct Choice {
    text: String,
    /// Similarity to the parameter's name; `0` when it was chosen as the only name of that type or as an enum constant.
    similarity: u32,
    confidence: f32,
}

/// The choice for one slot among the names that fit it (`used` are taken by earlier slots): the one that resembles
/// the parameter more than any other, or the only one left. `anything` is a slot every name fits (`Object`), where
/// being the only one says nothing and a resemblance is required.
///
/// `habit` is the name the project usually passes at this place: when it is among the names left it beats a guess made by
/// declaration order and a tie, with 0.9; a strictly better resemblance to the parameter still wins over it.
fn choose(param: Option<&str>, fitting: Vec<Candidate>, used: &[String], anything: bool, habit: Option<&str>) -> Option<Choice> {
    let free: Vec<Candidate> = fitting.into_iter().filter(|c| !used.contains(&c.name)).collect();
    let ordered = order(param, free);
    if let (Some(habitual), false) = (habit, anything) {
        if let Some((sim, c)) = ordered.iter().find(|(_, c)| c.name == habitual) {
            let top = ordered[0].0;
            let already_clear = ordered.len() == 1 || (top > 0 && ordered[1].0 < top && ordered[0].1.name == habitual);
            if *sim >= top && !already_clear {
                return Some(Choice { text: c.name.clone(), similarity: *sim, confidence: 0.9 });
            }
        }
    }
    match ordered.as_slice() {
        [] => None,
        [(sim, c)] if *sim > 0 || !anything => Some(Choice { text: c.name.clone(), similarity: *sim, confidence: confidence_of(*sim) }),
        [(top, c), (next, _), ..] if *top > 0 && next < top => Some(Choice { text: c.name.clone(), similarity: *top, confidence: confidence_of(*top) }),
        // Several names fit equally (two `Account`s, a parameter that resembles neither): the one declared first, which
        // is how arguments are usually lined up with parameters — a guess, and drawn as one (0.5).
        [(top, first), rest @ ..] if !anything => {
            let tied = std::iter::once(first).chain(rest.iter().take_while(|(s, _)| s == top).map(|(_, c)| c));
            let pick = tied.filter(|c| c.origin == first.origin).min_by_key(|c| c.decl).unwrap_or(first);
            Some(Choice { text: pick.name.clone(), similarity: *top, confidence: 0.5 })
        }
        _ => None,
    }
}

/// Confidence of a slot that no name fits and that is filled with a value that merely compiles.
const HOLE: f32 = 0.4;

/// A value that compiles for `ty` and means nothing: `0`, `false`, `""`, `null`. For a slot nothing in scope fits.
fn default_value(ty: &TypeRef) -> String {
    if ty.dims > 0 {
        return "null".into();
    }
    match ty.binary_name.as_str() {
        "boolean" => "false",
        "char" => "'\\0'",
        "byte" | "short" | "int" => "0",
        "long" => "0L",
        "float" => "0f",
        "double" => "0.0",
        "java/lang/String" => "\"\"",
        _ => "null",
    }
    .to_string()
}

fn confidence_of(similarity: u32) -> f32 {
    match similarity {
        s if s >= 100 => 0.95,
        s if s > 0 => 0.85,
        _ => 0.7,
    }
}

/// One overload filled from left to right; a slot nothing fits gets a value that compiles.
fn fill(
    overload: &Overload,
    start: usize,
    written: &[String],
    names_fitting: &dyn Fn(&TypeRef) -> Vec<Candidate>,
    constant_for: &dyn Fn(&TypeRef) -> Option<String>,
    habit_for: &dyn Fn(usize) -> Option<String>,
) -> Option<Vec<Choice>> {
    let mut chosen: Vec<Choice> = Vec::new();
    for (index, (ty, param)) in overload.params.iter().enumerate().skip(start) {
        let anything = ty.dims == 0 && ty.binary_name == rank::OBJECT;
        // what the arguments already written use is not used twice
        let used: Vec<String> = written.iter().cloned().chain(chosen.iter().map(|c| c.text.clone())).collect();
        let pick = choose(param.as_deref(), names_fitting(ty), &used, anything, habit_for(index).as_deref())
            .or_else(|| constant_for(ty).map(|text| Choice { text, similarity: 0, confidence: 0.8 }))
            // nothing fits: a value that compiles, so the rest of the list is still worth drawing
            .unwrap_or_else(|| Choice { text: default_value(ty), similarity: 0, confidence: HOLE });
        chosen.push(pick);
    }
    Some(chosen)
}

/// Whether `overload` takes exactly the parameter types `picked` names (simple names, `[]` for arrays).
fn matches_picked(overload: &Overload, picked: &[String]) -> bool {
    overload.params.len() == picked.len()
        && overload.params.iter().zip(picked).all(|((ty, _), want)| {
            let simple = ty.binary_name.rsplit(['/', '$']).next().unwrap_or(&ty.binary_name);
            *want == format!("{simple}{}", "[]".repeat(ty.dims as usize))
        })
}

/// Among the overloads that could be filled, the one that fills best. `None` when none can, or when the two best are
/// equally good.
fn best(filled: Vec<Vec<Choice>>) -> Option<Vec<Choice>> {
    let mut scored: Vec<(u32, Vec<Choice>)> = filled.into_iter().map(|c| (c.iter().map(|x| x.similarity).sum(), c)).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0));
    if scored.len() > 1 && scored[0].0 == scored[1].0 {
        return None;
    }
    scored.into_iter().next().map(|(_, choices)| choices)
}

/// The argument list for the call whose `(` is the last character before `caret` (and nothing follows on the line).
pub fn argument_list(source: &str, caret: usize, resolver: &dyn TypeResolver) -> Option<ArgumentList> {
    argument_list_with(source, caret, resolver, &|_, _| None)
}

/// [`argument_list`] with what the project knows: `habit(callee, index)` is the variable name usually passed as that
/// argument of that method. Where two names fit a slot equally, the habitual one is taken.
pub fn argument_list_with(source: &str, caret: usize, resolver: &dyn TypeResolver, habit: &dyn Fn(&str, usize) -> Option<String>) -> Option<ArgumentList> {
    argument_list_for(source, caret, resolver, habit, None)
}

/// [`argument_list_with`] for a call whose overload the user already chose in the completion list: `picked` holds the
/// simple type name of each parameter of that overload (`int`, `String[]`, `List`). Only the overloads that match it
/// are filled, so a tie between overloads, or an overload with no parameters, no longer means no answer.
pub fn argument_list_for(
    source: &str,
    caret: usize,
    resolver: &dyn TypeResolver,
    habit: &dyn Fn(&str, usize) -> Option<String>,
    picked: Option<&[String]>,
) -> Option<ArgumentList> {
    if caret > source.len() || !source.is_char_boundary(caret) || !source[..caret].ends_with('(') {
        return None;
    }
    list_from(source, caret, resolver, habit, picked, 0)
}

/// The arguments that remain for a call whose first ones are written: the caret is right after the comma (`upload(first, |`),
/// and every slot from there on is filled the way [`argument_list_for`] fills a whole list. The first argument may have been the
/// one the user was unsure about; the second is as certain as its type makes it. `None` when the caret is not at the start of
/// an argument after the first.
pub fn argument_rest(source: &str, caret: usize, resolver: &dyn TypeResolver, habit: &dyn Fn(&str, usize) -> Option<String>) -> Option<ArgumentList> {
    if caret > source.len() || !source.is_char_boundary(caret) || !source[..caret].trim_end().ends_with(',') {
        return None;
    }
    let (_, index, start) = crate::hints::call_site_name_and_index(source, caret)?;
    if index == 0 || !source.get(start..caret)?.trim().is_empty() {
        return None;
    }
    let mut list = list_from(source, caret, resolver, habit, None, index)?;
    // the comma has no space after it yet: the list opens with one
    if !source[..caret].ends_with(char::is_whitespace) {
        list.text.insert(0, ' ');
    }
    Some(list)
}

fn list_from(
    source: &str,
    caret: usize,
    resolver: &dyn TypeResolver,
    habit: &dyn Fn(&str, usize) -> Option<String>,
    picked: Option<&[String]>,
    start: usize,
) -> Option<ArgumentList> {
    let eol = source[caret..].find('\n').map_or(source.len(), |p| caret + p);
    if !source[caret..eol].trim().is_empty() {
        return None;
    }
    let mut site = call_site(source, caret, resolver)?;
    // from a later argument on, only the overloads that still have a parameter there are in the running
    if start > 0 {
        site.overloads.retain(|o| o.params.len() > start);
    }
    if let Some(types) = picked {
        let same: Vec<Overload> = site.overloads.iter().filter(|o| matches_picked(o, types)).cloned().collect();
        if !same.is_empty() {
            site.overloads = same;
        }
    }
    // `()` as one of the overloads is a valid answer too, and no list is better than the empty one by itself
    if start == 0 && site.overloads.iter().any(|o| o.params.is_empty()) {
        return None;
    }
    // the names in scope are read off a buffer whose call is closed, as the parser wants it
    let closed = format!("{});{}", &source[..caret], &source[caret..]);
    let (all, statik) = scope_candidates(&closed, caret, resolver)?;
    let lambda_parameters = crate::argument_locals::untyped_lambda_parameters(&closed, caret, resolver);
    let fitting = |ty: &TypeRef| {
        let ctx = rank::Context::new(source, statik).expecting(Some(ty.clone()));
        let mut fits: Vec<Candidate> = all.iter().filter(|c| ctx.fit(&c.ty, resolver) != rank::Fit::None).cloned().collect();
        // inside a lambda, where nothing of a known type fits: the lambda's own parameter, whose type nobody knows
        if fits.is_empty() && !(ty.dims == 0 && bennu_java::prelude::is_primitive(&ty.binary_name)) {
            fits.extend(lambda_parameters.iter().map(|(name, decl)| Candidate { name: name.clone(), origin: Origin::Parameter, ty: ty.clone(), decl: *decl }));
        }
        fits
    };
    let constant = |ty: &TypeRef| clear_constant(source, caret, resolver, ty);
    let callee = crate::hints::call_site_name_and_index(source, caret).map(|(name, _, _)| name);
    let habit_for = |index: usize| callee.as_deref().and_then(|c| habit(c, index));
    // the names the written arguments already use: any name of the scope that appears in them
    let written: Vec<String> = if start == 0 {
        Vec::new()
    } else {
        let head = &source[..caret];
        let statement = &head[head.rfind([';', '{', '}']).map_or(0, |p| p + 1)..];
        let used: Vec<&str> = statement.split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '$')).collect();
        all.iter().filter(|c| used.contains(&c.name.as_str())).map(|c| c.name.clone()).collect()
    };
    let filled: Vec<Vec<Choice>> = site.overloads.iter().filter_map(|o| fill(o, start, &written, &fitting, &constant, &habit_for)).collect();
    let several = filled.len() > 1;
    let chosen = best(filled)?;
    // a list made only of placeholders says nothing
    if !chosen.iter().any(|c| c.confidence > HOLE) {
        return None;
    }
    let confidence = chosen.iter().map(|c| c.confidence).fold(1.0f32, f32::min) * if several { 0.9 } else { 1.0 };
    let closer = if site.closes_statement { ");" } else { ")" };
    let args: Vec<&str> = chosen.iter().map(|c| c.text.as_str()).collect();
    Some(ArgumentList { text: format!("{}{closer}", args.join(", ")), confidence })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::argument_locals::Origin;

    fn cand(name: &str, origin: Origin, decl: usize) -> Candidate {
        Candidate { name: name.to_string(), origin, ty: TypeRef::simple("z/Account"), decl }
    }

    #[test]
    fn a_name_that_resembles_the_parameter_is_chosen() {
        let c = choose(Some("from"), vec![cand("target", Origin::Local, 9), cand("from", Origin::Parameter, 0)], &[], false, None).unwrap();
        assert_eq!((c.text.as_str(), c.confidence), ("from", 0.95));
    }

    #[test]
    fn the_only_name_of_that_type_is_chosen_but_not_for_object() {
        let only = vec![cand("acc", Origin::Local, 1)];
        assert_eq!(choose(Some("source"), only.clone(), &[], false, None).unwrap().text, "acc");
        assert!(choose(Some("source"), only, &[], true, None).is_none());
    }

    #[test]
    fn two_names_with_no_resemblance_give_the_first_declared_as_a_guess() {
        let c = choose(Some("pin"), vec![cand("b", Origin::Local, 2), cand("a", Origin::Local, 1)], &[], false, None).expect("a guess");
        assert_eq!((c.text.as_str(), c.confidence), ("a", 0.5));
        // but never for a slot every name fits
        assert!(choose(Some("pin"), vec![cand("a", Origin::Local, 1), cand("b", Origin::Local, 2)], &[], true, None).is_none());
    }

    #[test]
    fn a_name_is_not_used_for_two_slots() {
        let used = vec!["acc".to_string()];
        assert!(choose(Some("to"), vec![cand("acc", Origin::Local, 1)], &used, false, None).is_none());
    }

    fn filled(sims: &[u32]) -> Vec<Choice> {
        sims.iter().map(|s| Choice { text: format!("v{s}"), similarity: *s, confidence: 0.8 }).collect()
    }

    #[test]
    fn the_overload_that_fills_best_wins_and_a_tie_is_no_answer() {
        assert_eq!(best(vec![filled(&[100, 10]), filled(&[100])]).unwrap().len(), 2);
        assert!(best(vec![filled(&[100]), filled(&[100])]).is_none());
        assert!(best(vec![]).is_none());
        assert_eq!(best(vec![filled(&[10])]).unwrap().len(), 1);
    }

    #[test]
    fn a_slot_that_nothing_fits_gets_a_value_that_compiles() {
        let overload = Overload { params: vec![(TypeRef::simple("z/Account"), Some("from".into())), (TypeRef::simple("z/Money"), Some("amount".into()))] };
        let names = |ty: &TypeRef| if ty.binary_name == "z/Account" { vec![cand("from", Origin::Parameter, 0)] } else { Vec::new() };
        let holes = fill(&overload, 0, &[], &names, &|_| None, &|_| None).unwrap();
        assert_eq!(holes.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["from", "null"]);
        assert!(holes[1].confidence <= HOLE);
        // numbers, booleans and strings get their own zero
        assert_eq!(default_value(&TypeRef::simple("int")), "0");
        assert_eq!(default_value(&TypeRef::simple("boolean")), "false");
        assert_eq!(default_value(&TypeRef::simple("java/lang/String")), "\"\"");
        assert_eq!(default_value(&TypeRef::simple("long")), "0L");
        let with_constant = fill(&overload, 0, &[], &names, &|ty| (ty.binary_name == "z/Money").then(|| "Money.ZERO".to_string()), &|_| None).unwrap();
        assert_eq!(with_constant.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["from", "Money.ZERO"]);
    }

    #[test]
    fn the_habitual_name_beats_the_declaration_order_guess_and_a_tie() {
        let two = || vec![cand("x", Origin::Local, 1), cand("y", Origin::Local, 2)];
        // no habit: the first declared, as a guess
        assert_eq!(choose(Some("payer"), two(), &[], false, None).unwrap().text, "x");
        // the project always passes `y` here
        let c = choose(Some("payer"), two(), &[], false, Some("y")).unwrap();
        assert_eq!((c.text.as_str(), c.confidence), ("y", 0.9));
        // a habit that is not among the free names changes nothing
        assert_eq!(choose(Some("payer"), two(), &[], false, Some("z")).unwrap().text, "x");
        assert_eq!(choose(Some("payer"), two(), &["y".to_string()], false, Some("y")).unwrap().text, "x");
        // a strictly better resemblance still wins over the habit
        let c = choose(Some("x"), two(), &[], false, Some("y")).unwrap();
        assert_eq!((c.text.as_str(), c.confidence), ("x", 0.95));
        // a slot every name fits is not guessed from a habit
        assert!(choose(Some("payer"), two(), &[], true, Some("y")).is_none());
    }

    #[test]
    fn the_hint_is_asked_for_each_slot_by_its_index() {
        let overload = Overload { params: vec![(TypeRef::simple("z/Account"), Some("a".into())), (TypeRef::simple("z/Account"), Some("b".into()))] };
        let names = |_: &TypeRef| vec![cand("x", Origin::Local, 1), cand("y", Origin::Local, 2)];
        let filled = fill(&overload, 0, &[], &names, &|_| None, &|i| (i == 0).then(|| "y".to_string())).unwrap();
        assert_eq!(filled.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(), ["y", "x"]);
    }
}
