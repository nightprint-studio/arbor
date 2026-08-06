# arbor-syntax

Two things worth doing to a Tree-sitter tree once you stop caring which language produced it:
**look at it**, and **rewrite it by shape**.

```
INSERT INTO $t$ ($cols...$) VALUES ($vals...$)
→ matches every INSERT on any table, whatever the formatting
```

## It knows no language, and that is the point

Nothing here names a keyword, a node kind or a file extension. The caller brings a
`tree_sitter::Language` — Picus its SQL grammar, Bennu its Java one — and both halves work the same
for both. That is not speculative generality: a syntax-tree panel and a structural replace are the
same two features in every editor that has them, and written twice they would be two sets of bugs
about one thing.

## Byte ranges, never text

This crate does not store the source and never reconstructs it. Everything it hands back is a
`ByteRange` into a string the caller still owns — the same discipline as `picus-parse`, for the same
reason: a click on a node selects the *real* bytes, and an edit splices with the rest of the file
surviving byte for byte. The test that pins it applies a replacement to a CRLF file with an accented
comment in it and asserts every other byte is untouched.

## Half one — `outline`

The tree as data: kind, field, byte range, line, a one-line text preview, and whether the node is an
error or a token the parser had to invent to recover.

Three decisions worth knowing:

- **Anonymous nodes are reported.** It is tempting to show only named ones — commas and keywords
  make the tree noisy — but the panel exists to answer *why did the parser read it that way*, and the
  answer is very often a comma that landed somewhere unexpected. `named_only` is there for the
  caller who wants the tidy version.
- **Limits are part of the contract.** A 40 000-line install script has a few hundred thousand
  nodes. `outline` takes a budget and reports `truncated`, because a partial tree that looks complete
  is worse than one that admits it stopped.
- **A file that will not parse still has a tree.** Tree-sitter always produces one, its errors are
  nodes, and that is the case the panel is *most* useful for — so it is not the case it refuses.

`node_path_at` gives the root-to-leaf path holding a byte offset: what "reveal the node under the
cursor" needs. It walks the real tree rather than the outline, so it is correct even where the
outline was truncated.

`SyntaxNode` is also **constructible by hand**, deliberately. A product that derives a *semantic*
model from its parse can express it in this shape and reuse the panel that draws trees rather than
growing a second one — Bennu renders its Java declaration model beside the parse that way.
`synthesized` exists only for such a tree: it marks a node no source backs (a record's accessors,
a Lombok getter), whose range points at whatever declares it. `outline` never sets it, because a
parse tree is all source by definition.

### Islands

Some grammars hand back a region as **one token**: PostgreSQL's `$$ … $$` routine body is a single
string as far as the SQL grammar is concerned, and a JSP's scriptlet is one blob to an HTML one. A
tree that stops there is not wrong, but it is useless exactly where the interesting code is — an
update script does its work *inside* that body.

So the caller declares the islands, and `outline_with` / `node_path_at_with` parse them and splice
the sub-tree in:

```rust
Injection {
    kind: "dollar_quoted_string".into(),
    parents: vec!["do_statement".into(), "routine_body".into()],
    inner: |text| /* the range inside $tag$ … $tag$ */,
    language: sql,
}
```

Two things carry their weight here:

- **`parents` is load-bearing.** `$$ … $$` is a body only under a routine; anywhere else it is an
  ordinary string literal, and re-parsing one would invent structure the author never wrote. A tree
  that says something false is worse than one that stops.
- **Both coordinates are shifted**, bytes *and* lines. A sub-parse counts lines from one again, and
  forgetting the second shift produces the failure that is hardest to see: the ranges select
  correctly and every line number below the island is wrong.

Nodes whose children came from a second parse are flagged `injected`, so a panel can say so.

## Half two — `pattern`

A pattern is **source text of the target language with holes in it**. There is no second syntax to
learn, and it is parsed with the same grammar as the subject — which is what makes the match
structural. A pattern survives line breaks, extra whitespace, and a comment in the middle of a
statement, because none of those are nodes it compares.

| Syntax | Meaning |
|---|---|
| `$name$` | exactly one node |
| `$name...$` | a run of consecutive siblings, possibly empty |
| `$$` | a literal `$` — needed the moment somebody writes a PostgreSQL `$$ … $$` body |

A list capture comes back as **the original bytes from the first sibling to the last**, separators
included. Nothing is re-joined, so nothing can be re-joined wrongly: `COD, VAL` is what the file
said, not what this crate guessed a separator should be.

### The limits, stated plainly

- A placeholder is substituted with an ordinary identifier before parsing, so it can sit **anywhere
  an identifier can** — table names, column names, values, arguments. It cannot stand for a whole
  statement, because no grammar accepts an identifier there.
- A pattern is usually a fragment. `compile` parses it alone, which is right where a fragment is a
  top-level construct (an SQL statement is). `compile_in` takes a prefix and a suffix for the
  languages where it is not (`class C { void m() { … } }`). No range ever escapes pointing into the
  wrapper.
- Matches never nest. A replacement rewrites the matched range whole, so an inner match inside an
  outer one would be an edit inside an edit — and the second would be applied to text that no longer
  exists.

### Leaves compare their text, not only their kind

`T` and `U` are both `identifier`. A matcher that stopped at the kind would rewrite every table in
the repository when asked about one. Whether that comparison is case-sensitive is the **caller's**
decision (`case_insensitive`) — SQL keywords fold, Java names do not, and this crate refuses to
infer which from the grammar.

## Replacement templates

Source text again, read the other way. `$name$` writes back what that placeholder captured, byte for
byte from the subject, so values keep their quoting and their casing without anything having to
reconstruct them.

`$name.0$` addresses one element of a list capture — which is what makes reordering expressible:

```
pattern:     registra($v...$)
replacement: registra($v.2$, $v.0$, $v.1$)
```

Indices count **elements**, not separators, so the person writing the template counts what they can
see.

### Addressing through a parallel list

A position is a poor address exactly where it matters. If some statements write their arguments in
one order and some in another, `$v.0$` means a different thing in each — which is the bug, not the
fix. So one list can be addressed **through another**:

```
$values[columns=keycode]$
```

reads as *"the element of `values` at the index where `columns` is `keycode`"*. A template written
this way normalises every statement to one shape, whatever shape it was written in.

Nothing here knows those two lists are an `INSERT`'s columns and values — only that they are
parallel, which is a property of the pattern and not of SQL. Three rules keep it honest:

- **Which list holds the name is named, never guessed.** A shorthand that picked a list would be a
  rewriting tool guessing, and this one writes into somebody's file.
- **Lists of different lengths are refused.** They have no position in common, and pairing them
  anyway would write a value into a slot it does not belong to — the exact failure the form exists
  to prevent.
- **A name the index list does not hold is an error that says what it does hold**, which is usually
  the more interesting finding.

`render_with` takes the case-sensitivity of that lookup as a parameter, for the same reason the
pattern's leaves do.

`apply` writes a set of edits right to left and **refuses overlapping ones** rather than resolving
them: whichever way a resolution went, half the intent would be lost and nothing would say which
half.

## Tested against a language it is not used for

The unit tests run against `tree-sitter-java`, deliberately — a language-agnostic matcher proved only
against SQL is a matcher that has quietly learned SQL.
