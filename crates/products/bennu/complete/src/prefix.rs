//! Which candidates a prefix admits, and when a prefix has exactly one continuation.
//!
//! Two matchers rather than one, chosen explicitly at the call site: identifiers are
//! case-sensitive (an XML element named `Order` is not `order`) and configuration keys are
//! not (Spring's relaxed binding makes `readTimeout` and `read-timeout` the same key). A
//! single "smart" matcher that tried to be right for both would be right for neither.

/// Whether `candidate` starts with `prefix`, exactly.
pub fn matches(prefix: &str, candidate: &str) -> bool {
    candidate.starts_with(prefix)
}

/// Whether `candidate` starts with `prefix`, ignoring ASCII case.
pub fn matches_ignore_case(prefix: &str, candidate: &str) -> bool {
    candidate.len() >= prefix.len()
        && candidate.as_bytes()[..prefix.len()].eq_ignore_ascii_case(prefix.as_bytes())
}

/// The part of `candidate` that follows `prefix`, or `None` when it is not a continuation.
///
/// An exact match yields `Some("")` — the candidate is admitted, there is simply nothing left
/// to type. Callers that want "strictly longer" check for empty themselves, which is what
/// [`unique_continuation`] does.
pub fn continuation<'a>(prefix: &str, candidate: &'a str) -> Option<&'a str> {
    candidate.strip_prefix(prefix)
}

/// The continuation of `prefix` when **exactly one** is possible, and `None` otherwise.
///
/// This is the ghost-text rule, and it is the reason this crate exists. Ghost text is drawn
/// ahead of the caret as if it were already typed; unlike a popup it offers no alternatives
/// and gives the user nothing to reject. So it appears only where the answer is single-valued,
/// and the three ways it can fail to be are all handled here:
///
/// - **an empty prefix** never ghosts. There is nothing to continue, and completing from
///   nothing is inventing rather than predicting;
/// - **an exact match anywhere in the candidates** stops it. What is written is already a
///   complete, legal answer, and appending to it would silently turn it into a *different* one —
///   `redirect` becoming `redirectAction`, `create` becoming `create-drop`. This is the case that
///   would be indefensible, because the user did nothing wrong;
/// - **two candidates that continue differently** produce `None`, and the popup does the job
///   honestly.
///
/// Two candidates that continue *identically* are the same string, so they still count as one.
/// Refusing there would be superstition, and it saves every caller from de-duplicating first.
///
/// Short-circuits on the first disagreement, so passing a lazy iterator over a large vocabulary
/// is the intended use rather than a concession.
pub fn unique_continuation<I, S>(prefix: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    if prefix.is_empty() {
        return None;
    }
    let mut found: Option<String> = None;
    for candidate in candidates {
        let Some(rest) = continuation(prefix, candidate.as_ref()) else { continue };
        if rest.is_empty() {
            return None;
        }
        match &found {
            None => found = Some(rest.to_string()),
            Some(prev) if prev == rest => {}
            Some(_) => return None,
        }
    }
    found
}

/// The longest string every candidate begins with — the prefix a Tab could safely commit when
/// the full answer is still ambiguous.
///
/// Distinct from [`unique_continuation`] and held to a lower bar on purpose: this is not drawn
/// as if it were typed, it is what a *deliberate* keystroke may insert when several candidates
/// remain. `["create", "create-drop"]` shares `create`; `["true", "false"]` shares nothing and
/// yields `None`.
pub fn common_prefix<I, S>(candidates: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = candidates.into_iter();
    let mut common = iter.next()?.as_ref().to_string();
    for candidate in iter {
        let candidate = candidate.as_ref();
        let keep = common
            .char_indices()
            .zip(candidate.char_indices())
            .take_while(|((_, a), (_, b))| a == b)
            .map(|((i, c), _)| i + c.len_utf8())
            .last()
            .unwrap_or(0);
        common.truncate(keep);
        if common.is_empty() {
            return None;
        }
    }
    Some(common).filter(|c| !c.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn case_sensitivity_is_the_call_sites_decision() {
        assert!(matches("ser", "server.port"));
        assert!(!matches("SER", "server.port"));
        assert!(matches_ignore_case("SER", "server.port"));
        assert!(!matches_ignore_case("server.port.x", "server.port"), "longer than the candidate");
    }

    #[test]
    fn one_candidate_that_extends_the_prefix_is_ghosted() {
        assert_eq!(
            unique_continuation("server.servlet.context-p", ["server.servlet.context-path"]),
            Some("ath".to_string()),
        );
    }

    #[test]
    fn two_candidates_that_disagree_are_left_to_the_popup() {
        assert_eq!(unique_continuation("cre", ["create", "created"]), None);
    }

    /// The indefensible case: what is written is already a legal answer, and ghosting would turn
    /// it into a different one without the user doing anything wrong.
    #[test]
    fn a_prefix_that_is_itself_a_candidate_is_left_alone() {
        assert_eq!(unique_continuation("create", ["create", "create-drop"]), None);
        assert_eq!(unique_continuation("redirect", ["redirect", "redirectAction"]), None);
        // Still ghosted one character earlier, where nothing complete has been written.
        assert_eq!(
            unique_continuation("redirectA", ["redirect", "redirectAction"]),
            Some("ction".to_string()),
        );
    }

    /// The refinement over the hand-written version this replaced: a vocabulary that lists the
    /// same key twice is still certain, so callers need not de-duplicate before asking.
    #[test]
    fn the_same_continuation_twice_is_still_one_answer() {
        assert_eq!(
            unique_continuation("app.tim", ["app.timeout", "app.timeout"]),
            Some("eout".to_string()),
        );
    }

    #[test]
    fn nothing_is_ghosted_from_nothing_or_from_an_exact_match() {
        assert_eq!(unique_continuation("", ["server.port"]), None);
        assert_eq!(unique_continuation("server.port", ["server.port"]), None);
        assert_eq!(unique_continuation("zzz", ["server.port"]), None);
    }

    #[test]
    fn a_shared_head_is_offered_where_a_unique_answer_is_not() {
        assert_eq!(common_prefix(["create", "create-drop"]), Some("create".to_string()));
        assert_eq!(common_prefix(["true", "false"]), None);
        assert_eq!(common_prefix(Vec::<String>::new()), None);
        assert_eq!(common_prefix(["only"]), Some("only".to_string()));
    }

    /// Truncating a shared prefix mid-character would produce invalid UTF-8 and panic.
    #[test]
    fn a_shared_head_never_splits_a_character() {
        assert_eq!(common_prefix(["café-a", "café-b"]), Some("café-".to_string()));
        assert_eq!(common_prefix(["ça", "çb"]), Some("ç".to_string()));
        assert_eq!(common_prefix(["ça", "xb"]), None);
    }
}
