# bennu-lsp

A generic **Language Server Protocol client**. The implementation behind the slot
`bennu-intel` has documented since Phase 0: the frontend speaks one code-intel protocol for
every language, Java goes to Bennu's own index-backed engine, and everything else goes to a
language server.

Rust (rust-analyzer) is the first tenant, not a special case: it is one entry in
`catalogue.rs`, and a language nobody anticipated needs a `[[lsp.servers]]` block in the
user's config rather than a release.

## Layout

| module | responsibility |
|---|---|
| `jsonrpc` | base protocol — `Content-Length` framing, JSON-RPC message shapes |
| `types` | the protocol subset Bennu speaks, hand-rolled as serde structs |
| `uri` | `file:` URI ↔ path (percent-encoding, Windows drives, UNC) |
| `line_index` | `{line, character}` ↔ UTF-8 byte offset, in any position encoding |
| `client` | one server process: threads, request correlation, server→client traffic |
| `session` | one *initialized* server: handshake, capability gate, document sync, status |
| `ops` | the editor features, as requests |
| `convert` | protocol answers → `model` values |
| `semantic` | the delta-encoded token stream → coloured byte spans |
| `catalogue` | which servers exist, what they serve, where their root is |
| `discovery` | where their executables actually are on this machine |
| `model` | what a session returns: byte offsets, absolute forward-slashed paths |

Public API through `bennu_lsp::prelude::*` (workspace convention). `types` is deliberately
**not** in the prelude: a consumer reaching for `types::Position` is about to redo a
conversion the session already did.

## Dependencies

`serde`, `serde_json`, plus `arbor-process-ext` (`no_window`, so a long-lived child does not
keep a console window on Windows) and `arbor-core` (`user_home`, for discovery). **No new
external crates.**

The protocol types are written out rather than taken from a crate that models the whole
specification. Bennu drives a bounded subset — about fifteen requests — the mapping onto
Bennu's own wire types has to be written either way, and the shapes that are genuinely easy
to get wrong have tests beside them: the `boolean | Options` capability fields, the four
legal answers to a goto request, the two ways a completion item carries its edit.

## The three things that are easy to get wrong

**Coordinates.** LSP counts a `character` within a line, in units the *server* chooses —
UTF-16 by default, which is neither characters nor bytes. Bennu counts bytes from the start
of the file. Everything crossing the seam goes through `line_index`; a server that says
nothing about its encoding means UTF-16, **not** "whatever the client asked for". Get this
wrong and go-to is correct on ASCII and off by a column per accent everywhere else.

**Document state.** The editor owns the buffer, the server keeps a copy. Every
position-based request re-syncs first (`LspSession::sync`) instead of trusting editor events
to have arrived in order: a request whose offsets describe text the server has not seen is
answered confidently and wrongly.

**Liveness.** A server can take half a minute to become useful and can die at any moment.
Every request is bounded by a timeout sized to what the user is doing, and a dead server
*releases its waiters* rather than letting each one time out in turn — otherwise one crash
becomes a UI that hangs once per feature the user touches.

## Threads, and the reverse-channel landmine

Three threads per server: the reader (blocked on the child's stdout), the stderr drain (the
only place a refusal-to-start is ever written down), and whichever caller is making a
request.

A server→client **request** is dispatched on a short-lived worker thread, never inline on
the reader. Same landmine as `docs/reverse-channel.md` describes for Bennu's own IPC seam: a
handler that answered by calling *back* into the server would be waiting for a response only
the reader can deliver, and the reader is inside the handler. Notifications stay inline,
which is why `ServerHandler`'s notification methods must not block.

The server's stderr is mirrored to **our** stderr. bennu-be's stdout is its IPC channel to
the shell; one stray line on it desyncs the shell's framing, and a language server's log is
nothing but stray lines.

## Deliberate limitations

* **Full document sync**, never incremental. Incremental requires our idea of the document
  and the server's to stay byte-identical through every edit forever; one dropped change and
  the server silently analyses a file that does not exist. The editor hands over whole
  buffers anyway.
* **No resource operations.** Bennu applies text edits through the editor so undo works, but
  does not create, move or delete files for a server. The capability is therefore *not*
  advertised, so a server refuses a rename that needs a file move (a Rust `mod` rename)
  instead of answering with a move we would drop on the floor. Any that arrive anyway are
  surfaced as `FileOp`s for the caller to report.
* **`semanticTokens/full` only** — no delta, no range. Simpler, and fast enough on real
  files.
* **No `didChangeWatchedFiles`.** Dynamic registration is acknowledged but not acted on;
  document sync covers the buffers the editor owns, and a server re-reads the rest itself.
