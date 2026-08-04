# bennu-complete

The parts of "offer a completion" that are not about the language.

The extension seam has always carried completion *results* — `completions` returns a list,
`inline_hint` returns a continuation. It never carried the **production**, and production is
where the repetition lives. Every provider written for bennu independently grew the same five
steps: find the token's start, filter by prefix, drop what another vocabulary already offered,
stop at a ceiling, and decide whether the answer is certain enough to draw ahead of the caret.

None of those is about Spring, or XML, or Java.

## What it gives you

| Module | For |
|---|---|
| `caret` | `safe_offset`, `line_start`/`line_end`/`line_at`, `line_number`, `indent_of`, `token_before`/`token_after`, `within` |
| `prefix` | `matches` / `matches_ignore_case`, `continuation`, `unique_continuation`, `ghost`, `common_prefix` |
| `collect` | `Proposal` (fluent), `Proposals` (ordered, de-duplicated, capped) |

## The rule worth centralising

`unique_continuation` is why this crate is worth its `Cargo.toml`.

A completion popup may offer twenty candidates: being wrong costs a keystroke. Ghost text is
rendered inline, ahead of the caret, where it reads like text that is already there: being wrong
costs trust, and a provider that guesses once is a provider you stop reading.

So the rule is deliberately strict — an empty prefix never ghosts, an exact match contributes
nothing, and two candidates that continue differently produce nothing at all. Two candidates that
continue *identically* do produce it: they are the same string, and refusing there would be
superstition rather than caution (it also saves every caller from de-duplicating first).

`ghost` adds the clause that only shows up when you edit *inside* a token instead of at the end of
one: `</jav|a.version>` is certain and already written, and inserting the continuation at the caret
would read `java.versiona.version`. Refused rather than trimmed — the answer is committed at a
point, so with part of the token already ahead of the caret there is no insertion that produces the
right text. Providers pair it with `token_after` and the same `part` predicate that produced the
prefix, which is what keeps `=`, `>` and quotes from counting as "already written".

## Two matchers, chosen at the call site

Identifiers are case-sensitive — an XML element named `Order` is not `order`. Configuration keys
are not — Spring's relaxed binding makes `readTimeout` and `read-timeout` the same key. A single
"smart" matcher that tried to be right for both would be right for neither, so the choice is the
provider's and it is explicit.

## Why insertion order

Because the provider knows something a sort cannot: which of its vocabularies is the more
authoritative. A documented property beats an inferred one. Offering in that order and keeping it
means ranking is expressed by the code that has the knowledge, rather than by a score invented
afterwards to reconstruct it.

De-duplication follows from the same fact. Two vocabularies overlapping is the normal case — a
project whose `@ConfigurationProperties` are processed into metadata has every key in *both*
sources — so the first offer wins and the second is silently rejected.

## What is deliberately not here

Anything that needs to know what the text means. Where a key ends and a value begins, which
element may nest in which, whether the caret is inside a comment: that is the provider's job.
Pretending otherwise would produce a "generic" caret model that fits nothing.

## Consumers

`bennu-spring` (property keys and values), `bennu-xml` (elements, attributes, enumerated values).
