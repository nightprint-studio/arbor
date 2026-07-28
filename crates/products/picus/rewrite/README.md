# picus-rewrite

The only part of Picus that writes into a user's repository.

## The refusal this crate is built on

**Picus never re-prints a file.** It replaces the byte ranges it means to change and leaves
every other byte exactly as it found it.

The alternative — parse, modify a tree, print it back — is how a tool reformats four thousand
lines nobody asked it to touch, turns a review into noise, and eventually loses something it did
not understand. Splicing makes the byte-identical round trip a **property of the algorithm**
rather than a quality to be tested for: with no edits, the output *is* the input, and there is
no code path by which it could be otherwise.

## Three consequences, which are the crate

| Module | What it guarantees |
|---|---|
| `source` | `SourceText` keeps the original bytes beside the decoded text, so *"can this file be written back exactly as found?"* is answered **before any edit is prepared** |
| `splice` | Non-overlapping range replacements applied in position order, so the result never depends on the order the caller listed them in |
| `apply` | `prepare` does everything fallible except writing; `commit` writes and nothing else, all-or-nothing |

### The round-trip guard

`SourceText::verify_round_trip()` encodes the decoded text back and compares it with the bytes
that came off disk. A file that fails — a mis-detected encoding, a lossy decode that produced
U+FFFD — is one Picus **refuses to write to at all**, however correct the edit itself is. A tool
that cannot reproduce a file has no business saving it.

### Line endings

Generated SQL arrives with `\n`; half these repositories are CRLF. Conversion happens here, on
the way in, rather than at the emitter: it keeps the emitter's golden tests free of line-ending
variants, and it means every path into this crate gets it right. Mixed endings would turn a
three-line addition into a whole-file diff.

### Why prepare/commit are separate

What `prepare` produces is the exact bytes that will land — which is also what the diff preview
renders, so the user reviews the real thing rather than an approximation of it. And because
every fallible decision is made before the first write, the only failures `commit` can hit are
I/O ones; when one does, every file already written is put back.

"All or nothing" matters more here than usual: half a change applied across a two-dialect
repository is worse than none of it, because the branches now disagree and the tool meant to
detect that is the one that caused it.

## Tests

```bash
cargo test -p picus-rewrite
```

Two are load-bearing and should not be weakened:

- **`every_file_in_the_corpus_reproduces_itself_byte_for_byte`** — the round trip over a corpus
  of deliberately awkward inputs: windows-1252 accents, `€` and typographic quotes, a UTF-8 BOM,
  mixed line endings, a lone `\r`, no trailing newline, a 20 000-character line, an empty file.
  Everything else in the crate is only safe because this holds.
- **`a_failure_half_way_puts_every_earlier_file_back`** — writes two files, fails on the third,
  and asserts the first is back to its original bytes and the second (which had been created) is
  gone again.
