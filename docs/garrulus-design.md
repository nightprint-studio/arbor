# Garrulus — design proposal

Garrulus is Arbor's product for **notes**: an Obsidian-shaped knowledge base whose
defining feature is the one Obsidian charges for — **automatic synchronisation between
machines**, over a transport that is abstracted from day one and happens to be git-on-GitHub
first.

> *Garrulus glandarius*, the Eurasian jay: the bird that caches thousands of acorns in
> thousands of places and remembers where it put them. The suite is bird-named and this one
> writes itself.

The brief, verbatim: an Obsidian-like editor with automatic online sync (abstract the
*where*, start on GitHub); Corvus already has a live-preview markdown editor that needs a
lot of work (no toolbar, few shortcuts); recycle from Bennu and Picus; and a system of
**templates / layouts per note kind** — bug reports, application improvements, game design.

---

## 1. Why this exists, and the one line that justifies it

Obsidian is excellent and its file format is plain markdown on disk. The single reason to
build anything is that **Obsidian Sync is a subscription and the vault has to live on two
PCs**. Everything else Obsidian does well is a thing we either already have in Arbor or can
recycle from a sibling product.

Which sets the strategic posture, and it is the most important decision in this document:

> **Garrulus is not a replacement for Obsidian. It is a second, better client on the same
> vault.**

The vault stays a folder of `.md` files with YAML frontmatter, `[[wikilinks]]`, `#tags`,
`![[embeds]]` and `> [!callout]` blocks — the Obsidian dialect, byte-compatible. Open the
same folder in Obsidian and it works. That is not a compromise, it is the de-risking: the
product can be half-finished and still be worth using, because nothing is ever trapped
inside it. It also means the "migration" from an existing vault is `git init`.

Corollary rules that fall out of this and are not negotiable:

- **Plain files, always.** No database of record, no proprietary container. An index may
  exist for speed, but it is a cache that can be deleted and rebuilt.
- **Never lose a keystroke.** The sync engine may fail, retry, conflict, or be offline for a
  week. It may never silently drop or overwrite text the user typed. Every ambiguous merge
  becomes a *visible artefact the user resolves*, never a quiet resolution.
- **Offline is the normal case**, not the degraded one. Sync is an eventual background
  concern; the editor never waits for it.

---

## 2. What Arbor already has that makes this cheap

This is the part that makes Garrulus a reasonable project rather than a two-year one. In
rough order of how much work it saves:

| Asset | Where | What it gives Garrulus |
|---|---|---|
| **`corvus-git`** | `crates/products/corvus/git/` | A Tauri-free git engine: `remote.rs` (fetch / push / pull), `merge.rs`, `status.rs`, `diff.rs`, `repo.rs`, `reset.rs`, `stash.rs`. **This is the sync engine**, already written and already used in anger. |
| **Credential broker** | `crates/foundation/shell-common/src/broker.rs` | The shell is the sole keyring holder; a BE asks for a token over the reverse channel. GitHub HTTPS auth injection (Basic `x-access-token:<token>`) is a *solved, documented* landmine. Garrulus pays nothing for auth. |
| **`arbor-scheduler`** | `crates/platform/scheduler/` | FixedRate / FixedDelay / Cron triggers with cooperative cancellation and per-tick focus gating. The background fetch that keeps the sync button honest is a registration, not a subsystem. |
| **`markdown-editor.ts`** | `src/lib/utils/markdown-editor.ts` (1539 lines) | The Obsidian-style live preview already exists and is good: per-inline-component conceal, ATX heading sizing, GFM tables rendered as real `<table>` widgets, fenced code tokenised through Prism, images/video/audio resolved relative to the doc, links with dimmed URLs. Viewport-only, rebuilt on scroll/selection. |
| **`shared/ui/code-editor/`** | `folding`, `sticky-scroll`, `minimap`, `scrollbar-overview`, `indent-guides`, `inline-completion` | Editor-agnostic CodeMirror extensions. Markdown gets heading-folding, a sticky heading breadcrumb and a minimap essentially for free. |
| **`arbor-fs`** | `crates/foundation/fs/` | read / mutate / copy / trash / zip / roots / encoding. Vault I/O, including *delete goes to trash* — which matters a lot when the thing being deleted is a note. |
| **The BE scaffold** | `arbor-be`, `arbor-rpc`, `arbor-ipc` | A new product BE is ~8 mechanical steps (`docs/backend-architecture.md` §8), and handlers self-register via `inventory` — adding an RPC method costs **zero** shell edits. |
| **The plugin host** | `arbor-plugin-*` | Lua plugins, hooks, settings panels, forms. Garrulus can expose vault hooks on day one at near-zero cost. |
| **Deep links** | `shared/DeepLink*` | `arbor://garrulus/note/<id>` — a Corvus commit can point at the note that explains it. |

---

## 3. Architecture

Standard Model D (`docs/backend-architecture.md`): one FE shell, one headless BE, JSON
frames on stdio, `#[arbor_rpc::handler]` auto-registered.

### 3.1 Crates

```
crates/products/garrulus/
  parse/    one Tree-sitter markdown grammar + a byte-range reader over it
  ast/      the FORMAT-AGNOSTIC document model + Reader/Writer traits
  core/     GarrulusState — open vault, index handle, sync engine handle, config
  vault/    the vault model: discovery, note types, templates, naming, frontmatter.
            PURE and heavily unit-tested.
  index/    the link graph + search index over the vault. A cache, rebuildable.
  sync/     the sync seam: `trait SyncRemote` + `GitRemote` + `FolderRemote`
  be/       [[bin]] garrulus-be — one file per handler domain
```

The layering mirrors Picus exactly, which is not an accident — it is the same shape of
problem (a repository of text files on disk, parsed, understood, and rewritten):
`picus-parse` : `picus-ast` :: `garrulus-parse` : `garrulus-ast`.

`vault/` depending on nothing but `arbor-fs` + `garrulus-ast` + serde is the important
discipline: the interesting logic (what is a link, what type is this note, what filename does
this template produce) is pure and testable without a process, a runtime, or a filesystem.

`sync/` depends on `corvus-git` for the git implementation — a product-to-product crate
edge. **Decided: `corvus-git` stays where it is for now**; the edge is documented and
accepted (§11.4).

### 3.2 Window and frontend

> **The UI is settled.** `docs/mockups/garrulus-ui.html` is the approved reference for the
> window: chrome, panel geometry, the five views (editor, search, home, table, graph), the
> sync button's state machine, and the visual vocabulary for note types, tags, callouts and
> commit-pinned code links. It is a stub — only part of it is wired — but where it and this
> document disagree about *appearance*, the mockup wins. It is also the first stub of this
> layout language for the suite at large, so changes to it are worth making there first.


- Window label `garrulus`, branched in `src/routes/+page.svelte` like every other product,
  dynamically imported so no other window pays for it.
- `src/lib/components/garrulus/`, `stores/garrulus/`, `ipc/garrulus.ts`.
- One FE helper: `garrulus(method, params)` → `invoke('rpc', { program: 'garrulus', … })`.
- Canopy roster entry in `src/lib/components/launcher/canopy.ts` + a mark in
  `src/lib/utils/product-marks.ts` (the shared per-product table — the launcher, the tab strip
  and anywhere else a product needs a face all draw from it). Accent: **`#3fb6d9`** proposed — the jay's barred wing-flash cyan-blue,
  picked to read apart from Corvus's periwinkle `#7c9cf5` and Picus's teal `#4fbfa8`.
  *TODO(user): confirm.*

### 3.3 Storage

Per `docs/backend-architecture.md` §9, profile × product:

| Path | Contents |
|---|---|
| `profiles/<p>/garrulus/config.toml` | product prefs: editor, sync cadence, device name |
| `profiles/<p>/garrulus/vaults.json` | UUID → `{ path, display_name, remote }` — the only place absolute vault paths live (mirrors `corvus/repos.json`) |
| `<vault>/.arbor/garrulus/types/*.toml` | note types + templates — **inside the vault**, so they sync |
| `<vault>/.arbor/garrulus/vault.toml` | vault-scoped settings: attachment folder, daily-note folder, link style |
| `<vault>/.arbor/garrulus/devices.json` | last-seen per device (§4.5) |
| `<vault>/.arbor/garrulus/session-<device>.json` | open tabs + cursors, for the handoff (§4.5) |
| `<vault>/.arbor/garrulus/trash/` | deleted notes, recoverable without digging in git |
| `cache/garrulus/<vault-uuid>/` | the index. Deletable. Never synced. |

**Everything a product writes into a project goes under `<project>/.arbor/<product>/`** —
one dot-folder per project, not one per tool. This is already the established convention
(`<repo>/.arbor/config.toml` per CLAUDE.md) and it now applies to every product from here on.
Practical consequence for a vault: a single `.arbor/` entry, and the whole of Garrulus's
project-scoped state is one folder you can inspect, back up, or delete.

Types and vault settings living *inside* the vault and syncing with it is deliberate: on the
second PC, the templates are already there.

### 3.4 The parser and the document model

Markdown is regular enough that you can get very far with regexes, which is exactly why
almost every note app is built on them and why almost every note app mangles a link inside a
code fence. Garrulus parses properly, for three reasons that are worth more than the effort:

**A Tree-sitter grammar** (`garrulus-parse`), vendored and compiled by `build.rs` with `cc`,
the same way `picus-parse` and `merula-lang` already do it — so the workspace builds one
`tree-sitter` artefact and the pattern is established. It gives byte-ranged, incremental,
error-tolerant parses: a link is a link only where a link can be, a `#tag` inside a fence is
text, a `[[` inside inline code is not an autocomplete trigger.

**A format-agnostic AST** (`garrulus-ast`), and this is the piece worth arguing for. It
carries the same invariant `picus-ast` carries about dialects:

> **The document model mentions no syntax.** `Document` / `Block` / `Inline` describe *what
> the note is* — headings, lists, tasks, links, callouts, code, tables, frontmatter — never
> how markdown spells it.

with two traits around it:

```rust
/// Parse a source format into the model.
pub trait Reader { fn read(&self, source: &str) -> Result<Document, ReadError>; }
/// Render the model into a target format.
pub trait Writer { fn write(&self, doc: &Document) -> Result<String, WriteError>; }
```

`MarkdownReader` / `MarkdownWriter` are impl #1 and #2. That single seam is what makes the
following *fall out* rather than each being its own hand-rolled string mangler:

- **HTML export and PDF export** are `Writer`s (§8). So is a static-site export, so is a
  plain-text one for pasting into a ticket.
- **Embeds and transclusion** (`![[note#heading]]`) are a subtree splice on the AST, not a
  substring hunt.
- **Templates with logic** — a `{{query}}` that inlines a live note list — insert *blocks*,
  not text.
- **Refactors**: split a note at its `##`s, merge two notes, promote a section to its own
  note with a link left behind. These are AST operations; on strings they are bug factories.
- **The outline, the task list and the link extraction** are one walk, not three regex passes.
- **Other input formats later** — org-mode, AsciiDoc, Typst, a Notion or Joplin import — are
  a new `Reader`, and everything downstream (index, search, export, refactors, views) works
  on day one. This was the user's stated reason for wanting the AST, and it is the right one.

So: not useless. The AST is the thing that makes half of §8 cheap instead of expensive.

**One honest note on duplication.** The editor keeps using CodeMirror's **Lezer** markdown
parser for live decoration in the frontend, because that is what does incremental
per-keystroke work inside a `ViewPlugin`. Tree-sitter runs in the BE over the whole vault.
Two parsers for one language — the same split Picus already lives with (`sql-intel` in the
FE, `picus-parse` in the BE), and it is the right one: they answer different questions on
different cadences. The rule that keeps them from drifting is that **the AST is authoritative**
— anything user-visible beyond styling (links, tasks, outline, tags) comes from the BE's
parse, and the FE's Lezer tree only paints.

> **New dependency:** `tree-sitter` is already in the workspace, but a **markdown grammar**
> is a new vendored `parser.c` (the tree-sitter-markdown family ships a block parser and an
> inline parser). Vendored and committed like Picus's — flagged per the "ask before adding"
> rule, and assumed approved since it is the explicit ask.

---

## 4. The sync seam — the centre of the product

The user asked to abstract *where* sync happens. Done properly this means the trait must not
be git-shaped, or the abstraction is a lie that costs a rewrite the first time a second
backend appears.

```rust
/// One place the vault is mirrored to. Implementations are free to be a git remote,
/// a folder on a USB stick, or an object store — the vocabulary here is
/// "reconcile two versions of a folder of notes", not "run git".
#[async_trait]
pub trait SyncRemote: Send + Sync {
    fn descriptor(&self) -> RemoteDescriptor;          // id, kind, display name, capabilities
    async fn probe(&self) -> SyncResult<RemoteHealth>; // reachable? authenticated? behind/ahead?
    async fn pull(&self, vault: &VaultPath) -> SyncResult<PullOutcome>;
    async fn push(&self, vault: &VaultPath, batch: &ChangeBatch) -> SyncResult<PushOutcome>;
    async fn history(&self, note: &NoteId) -> SyncResult<Vec<Revision>>;
    async fn revision(&self, note: &NoteId, rev: &RevisionId) -> SyncResult<String>;
}

pub struct RemoteCapabilities {
    pub history: bool,      // can it answer `history`/`revision`? (git yes, folder no)
    pub atomic_batch: bool, // is a push all-or-nothing?
    pub conflicts: bool,    // can it detect concurrent edits, or is it last-writer-wins?
}
```

`PullOutcome` carries `{ applied: Vec<NoteId>, conflicts: Vec<Conflict> }`. `Conflict`
carries the three sides — base, local, remote — as *text*, not as merge markers. The UI never
sees a `<<<<<<<`.

### 4.1 Two implementations from day one

Shipping one implementation behind a trait is the pattern CLAUDE.md warns about, so the
proposal is to ship two, and the second one earns its place:

- **`GitRemote`** — the real one. `corvus-git` does the work.
- **`FolderRemote`** — the vault mirrored to a plain directory: a USB stick, a network
  share, or a folder that Drive/OneDrive/Dropbox already syncs for the user. `history` is
  unsupported (`capabilities.history = false`, and the UI hides the history panel rather
  than showing a broken one). Concurrency is timestamp + content-hash based, and a genuine
  concurrent edit becomes the same `Conflict` the git path produces.

`FolderRemote` is not filler. It is how the whole engine gets tested without a network, it
covers the "I already pay for Drive" user in one afternoon, and — the real reason — it is
what proves the trait is not secretly `git`. If `FolderRemote` is awkward to write against
the trait, the trait is wrong.

Later candidates, unprioritised: WebDAV / Nextcloud, S3-compatible, an Arbor-hosted relay.

### 4.2 The git implementation

The vault is a git repository. That is the whole trick, and it is invisible: Garrulus never
shows a branch, a commit, or a staging area unless the user goes looking for them.

**Nothing writes without a click.** The background does exactly one thing, and it is
read-only: an `arbor-scheduler` FixedDelay tick (default 60 s, gated on window focus and on
a successful `probe`) that **fetches** and updates the state indicator. It never commits,
never pushes, never pulls, never touches the working tree. Everything that changes bytes —
local or remote — happens because the user pressed the button in §4.3.

This is a deliberate reversal of the first draft's timed auto-commit, and it is the better
design: a note vault is not a build server, and a commit log full of `Aggiornate 2 note`
every twenty seconds is a history nobody can read. The fetch still runs on a timer, because
*"c'è roba nuova dall'altro PC"* is only truthful if something asked.

**Commits** are made when the button acts: all dirty notes in one commit, message
auto-generated (`Nuova nota: Bug — crash all'avvio` / `Aggiornate 3 note`), author
`Garrulus (<device>)` so the history reads as a log of *where* you were working. A dropdown
entry lets the user write the message by hand when a change deserves one.

**Integration is rebase, not merge**, so history stays linear and readable as "what happened
to the vault", and a note's `git log` is its version history without merge noise.

**Buffer reload after a pull**: an open, unmodified buffer updates in place; a modified one
gets a non-blocking banner offering *take theirs*, *keep mine*, or *diff*.

**Failure behaviour** follows the CLAUDE.md auto-reconnect rules exactly: retries are silent
with an inline banner; one loss toast per episode after the first failed retry; a regain toast
only if a loss toast fired; one give-up toast at max retries. A single `disconnect_notified`
bool gates both.

**Repository creation is private, always.** `garrulus-sync` creates the remote through the
existing `GitProvider::create_repo` with `RepoVisibility::Private` — already implemented for
both GitHub and GitLab (`corvus/git-provider/*/src/repo.rs`). There is no public option in
the UI: a personal note vault has no business being public, and an accidental click is not a
mistake that can be undone (the content is already indexed by then). If the user genuinely
wants a public vault they can flip it on the provider's own website, deliberately.

### 4.3 The sync button — the whole sync UI

One control in the title bar. It is a `shared/ui/SplitButton`: the main half performs the
obvious action, the caret opens the rest. It is also the *only* place sync state is
displayed, so there is exactly one thing to look at.

**States**, each with its own icon, colour and tooltip:

| State | Reads | Main action | Colour |
|---|---|---|---|
| `synced` | "Allineato · casa, 3 min fa" | Fetch now | muted |
| `has-changes(n)` | "3 note da inviare" | **Sync** (commit → pull → push) | accent |
| `behind(n)` | "2 note in arrivo da casa" | **Pull** | info |
| `diverged(a,b)` | "3 da inviare · 2 in arrivo" | **Sync** | warning |
| `conflict(n)` | "1 conflitto da risolvere" | Open the Conflicts panel | danger |
| `syncing` | "Sincronizzo…" | Cancel | accent, spinner |
| `offline` | "Non raggiungibile · riprovo" | Retry | muted |
| `no-remote` | "Nessuna destinazione" | Configure a remote | muted |

The colours are state, not decoration — the CLAUDE.md line. The count rides in a `Badge`, so
the button is readable at a glance without reading the tooltip.

**Dropdown**: Pull only · Push only · Commit only (with a message) · Show pending changes ·
Conflicts · History of this note · Configure remote · Sync now (keyboard `Ctrl+Shift+S`).

The button is keyboard-reachable and its action is a palette verb, so nothing about sync
requires a mouse — the keyboard-first rule applies to it like everything else.

### 4.4 Conflicts — the part that decides whether this is usable

Two PCs and one vault means conflicts are not an edge case, they are Tuesday. The design:

1. **Frontmatter merges field-wise.** It is structured data; merging it as lines is
   gratuitous. Same field changed on both sides → that field alone is a conflict.
2. **Body merges three-way, line-based.** Prose and lists merge cleanly far more often than
   people expect, because two people editing the same note usually edit different parts of it.
3. **What does not auto-merge never goes into the file.** The note keeps the local version.
   The remote version is written beside it as `Nota (conflitto — casa, 31-07 14:22).md`, and a
   **Conflicts** entry appears in the bottom dock with a side-by-side diff (Corvus's
   `DiffViewer`, recycled) and three actions: *keep mine*, *take theirs*, *merge by hand*.
   Nothing is lost, nothing is corrupted, and the vault still opens in Obsidian mid-conflict.
4. **`.arbor/garrulus/` metadata never conflicts** — it is merged by rule (union of types,
   per-key last-writer-wins on settings) because a conflict in a settings file is pure noise.
5. **The daily note is append-merged, not three-way merged.** It is by far the most
   conflict-prone file in any vault — two machines appending to the same day — and it is also
   the one where a three-way merge is *wrong*: the correct answer is the union of both days'
   entries in time order. Special-cased, and it removes most real-world conflicts outright.

### 4.5 Two-PC affordances

Small things that only matter when there are literally two machines, which is the user's
actual situation:

- **Device identity** in config; used in commit authorship and in every UI string that
  mentions "the other machine".
- **`.arbor/garrulus/devices.json`** — last-seen per device, so the sync button can say
  *"allineato · casa, 3 min fa"* rather than a timestamp with no subject.
- **Session handoff** — `.arbor/garrulus/session-<device>.json` records open tabs and cursor
  positions. Opening Garrulus on the other PC offers *"riprendi da dove eri su casa"*.
  This is the feature that will get used every single day and takes an afternoon.
- **"Novità dall'altro PC"** — the first pull of the day produces a digest: which notes casa
  touched, with a one-click diff each. Coming back to a machine after two days and knowing
  what changed is the difference between trusting the vault and re-reading it.

---

## 5. The vault model

### 5.1 What a note is

A `.md` file. Parsed into:

```rust
pub struct Note {
    pub id: NoteId,               // stable: path-relative, or a frontmatter uid if present
    pub path: RelPath,
    pub title: String,            // frontmatter `title`, else H1, else filename
    pub frontmatter: Frontmatter, // ordered YAML map, round-trips byte-stable
    pub kind: Option<TypeId>,     // resolved via §6
    pub links: Vec<Link>,         // [[wikilink]] / [[wikilink|alias]] / ![[embed]] / [md](./rel.md)
    pub tags: Vec<Tag>,           // #tag, #nested/tag, and frontmatter `tags:`
    pub tasks: Vec<Task>,         // - [ ] / - [x], with due dates and the heading they sit under
    pub headings: Vec<Heading>,   // the outline
}
```

**Frontmatter must round-trip byte-stable when untouched.** If Garrulus reformats YAML on
every save, every note in the vault turns into a diff the first time it is opened, and the
sync history becomes worthless. This is a hard invariant with a test.

### 5.2 The index

Built at vault open, updated incrementally on save. It is a cache in `cache/garrulus/`, and
a corrupt or missing one triggers a rebuild rather than an error.

Contents: title → note (for the quick switcher, with fuzzy matching), the link graph both
directions (backlinks are just reversed edges), tag → notes, an inverted word index for
full-text search, and unresolved links (a `[[Foo]]` pointing at no note — a first-class
concept, since in Obsidian that is how you create a note).

**Sizing note:** a personal vault is thousands of notes, not the two million symbols
`bennu-index` was built for. Start with a plain in-memory index built at open — it will be
instant at this scale and a fraction of the code. `bennu-index`'s `fst` + `rkyv` + mmap store
is the documented upgrade path if a vault ever gets big enough to notice, and its
`relations.rs` is a working model for the graph half. Its *schema* is Java-specific and is not
reusable as-is.

### 5.3 Why a bug is a note and not a row

The question is fair and it is the right one to ask: a bug is a *record* — fields, states,
filters, sorts, aggregates — and records are what databases are for. Prose is what markdown is
for. So why not keep bugs and improvements in SQLite and leave markdown to the prose?

**Because git cannot merge a SQLite file.** That single fact settles it. The entire reason
this product exists is two-machine sync over a git remote; a database is one opaque binary
blob, so *any* concurrent edit — a bug closed at home while a note is filed at the office —
is a whole-file conflict whose only resolutions are "keep mine" or "keep theirs". Losing an
afternoon of records to a conflict is not a rough edge, it is the failure the product was
built to prevent. Markdown conflicts are per-file and usually per-hunk; database conflicts are
total.

Three further reasons, each independently sufficient:

- **A row cannot be a node in the graph.** Bugs and design notes are the *most* linked
  content in this vault: they point at each other, at decisions, at commits, at daily notes.
  Moving records into a database carves the most-linked content out of the linking system —
  no backlinks, no embeds, no graph, no `[[…]]`.
- **The body of a bug is prose.** Steps to reproduce, analysis, screenshots, a stack trace, a
  code link. Half the record would live in a text column that no editor understands anyway.
- **Obsidian compatibility is a hard constraint** (§1). Rows are not readable by anything else.

**And the tempting hybrid is the worst option**: records in SQLite, prose in markdown, joined
by an id. Two sources of truth that can disagree, a sync engine that has to reconcile both,
and a bug whose severity lives somewhere your text editor cannot see. Rejected explicitly.

**The real weakness of records-as-text is not query speed, it is schema drift** — nothing in a
text file stops `severity: bloker`. That is a solved problem here, and the solution is already
in the design: a field is written through the frontmatter form (§6), validated on write against
the type's `FieldSpec.values`, and any pre-existing violation shows up in the problems panel.
Typed records in plain files, enforced at the edge rather than by the storage engine.

#### Where SQLite would genuinely earn its place

Not as the record. As the **index** — and that is a real, open option, because the index is
explicitly a rebuildable cache (§5.2) sitting behind one API (`Index::search(&Query) -> Vec<Hit>`).
Two honest arguments for it: `parse_query` + `Filter` application is a small hand-rolled WHERE
clause, and hand-rolled full-text search is usually mediocre where FTS5 gives ranked BM25
matching with prefix search and snippets for free.

**Decided: no database in Garrulus. In-memory index, seam kept.** At personal-vault
scale — thousands of notes, not millions of symbols — plain scans in Rust are microseconds and
the cold-start rebuild is well under the threshold where anyone notices, especially since it is
async. Against that, `rusqlite` is a third vendored C dependency in a workspace that already
carries libgit2 and Lua, and committing to it now means committing before the query shapes are
known. Because the swap is contained behind the `Index` API, deferring costs a few hundred lines
of `text.rs`/`fuzzy.rs` if it ever happens — not an architecture.

The list below is kept as a **revisit condition, not a plan** — the point of writing it down is
so a future "should this be SQLite?" has an answer that is already reasoned. Nothing is built
toward it. Reopen only when **any one** of these becomes true:

1. Cold-start index rebuild becomes noticeable (order of a second) — realistically past ~20k notes.
2. Search *quality* keeps disappointing in ways stemming and BM25 would fix, rather than ways
   more tuning would fix.
3. A feature arrives that genuinely wants a persistent, queryable store: cross-vault search, or
   history-aware queries ("how did this field change across revisions").

If it flips: the store lives in `cache/garrulus/<vault-uuid>/`, is never synced, and is deleted
and rebuilt on any schema change. It is still not the record.

**Parked, and a Picus question rather than a Garrulus one: a SQLite engine in Picus.** It would
make the above cheaper without changing the decision. Not through Picus's abstraction —
`DbSession` is a string-oriented "show a human a grid" interface, and reading an index back
through `ExecuteResult` would be absurd; `garrulus-index` would use `rusqlite` directly. What
would be shared is the **dependency and its vendored C build**, under one workspace pin, exactly
the discipline already applied to mlua and libgit2. That removes the main thing currently weighing
against trigger 3: the marginal cost drops from "a third vendored C dependency" to "one more line
in a Cargo.toml". Worth doing on Picus's own merits (SQLite is the third thing a developer opens,
and being serverless makes it the cheapest engine to exercise the provider abstraction with — the
role `FolderRemote` plays for `SyncRemote`). The cost there is not the driver, it is `EngineKind`:
two variants today, matched in ~365 places across 46 files, so a third variant makes the compiler
demand an answer everywhere — which argues for adding it as a **client-only** engine that browses
and queries but does not participate in the dialect/script-repository half, which is exactly what
`EngineCapabilities` exists to express.

---

## 6. Note types, templates and layouts

The third ask, and the piece that makes this more than an Obsidian clone. A **note type** is
a first-class object, not a template file you copy from. It lives inside the vault (so it
syncs) under the one project dot-folder: `<vault>/.arbor/garrulus/types/`.

`<vault>/.arbor/garrulus/types/bug.toml`:

```toml
id     = "bug"
name   = "Bug"
icon   = "bug"          # lucide
accent = "#f28b82"
folder = "bugs"                       # where new ones land
naming = "{{date}}-{{slug}}"          # filename pattern

# How an EXISTING note is recognised as this type, in priority order.
match_frontmatter = { type = "bug" }
match_folder      = "bugs/**"

[[fields]]
key = "app";      label = "Applicazione"; kind = "enum"; source = "vault:apps"; required = true
[[fields]]
key = "version";  label = "Versione";     kind = "text"
[[fields]]
key = "severity"; label = "Gravità";      kind = "enum"; values = ["blocker","major","minor","cosmetic"]; default = "major"
[[fields]]
key = "status";   label = "Stato";        kind = "enum"; values = ["aperto","in corso","risolto","non riproducibile"]; default = "aperto"
[[fields]]
key = "commit";   label = "Commit";       kind = "link:corvus"   # ← see §7.4

template = """
## Passi per riprodurre
1. {{cursor}}

## Atteso

## Ottenuto

## Note
"""
```

What this buys, all of it mechanical once the schema exists:

- **A real form for the frontmatter.** Severity is a dropdown, not a string you mistype.
  Rendered from the field schema — recycling `BennuFormsPanel` / `BennuFormFieldRow` and
  `shared/ui/FormField`, the same "schema → form" pattern the plugin host's `FormNodeRenderer`
  already implements. On disk it is still ordinary YAML that Obsidian reads fine.
- **Table views for free.** Any query — a folder, a tag, a type — renders as a sortable,
  filterable, inline-editable grid whose columns are the type's fields. This is Obsidian's
  "Bases"/Dataview, and Picus's `QueryResultPanel` is already an editable grid; the work is
  wiring, not invention.
- **Board views for free.** A type with an `enum` field marked `board = true` gets a kanban
  grouped by it. Bug triage, feature pipeline, game-design status.
- **Per-type colour.** The accent flows into the tab, the graph node, the search result and
  the grid row. Colour used for *state and kind*, not decoration — the CLAUDE.md line.

**Built-in types shipped** (copied into a new vault, then fully editable): `bug`,
`improvement`, `gamedesign`, `daily`, `meeting`, `decision` (an ADR: context / decision /
consequences), `snippet`.

**Layouts.** A type also declares which panels open with it — a bug opens with Backlinks +
Tasks; a game-design note opens with the Graph and a wide editor. One `layout` block on the
type, applied on open, overridable per-tab. This is the "layout diversi" half of the ask.

Ideas that follow naturally and are worth having: a type can declare `on_create` actions
(open a specific field first, prefill `app` from the last used one), and `Alt+Enter` on an
untyped note offers *"promote to Bug"* — which applies the template's missing headings and
opens the frontmatter form, without touching what is already written.

---

## 7. The editor

### 7.0 It becomes a shared, abstract editor

**Decided**: the markdown editor is not a Garrulus component. It is extracted into a
product-agnostic editor that Garrulus is merely the first serious consumer of, so that Corvus
(README / commit bodies / MR descriptions / git notes) and Bennu (javadoc, project docs) can
mount the same thing later without a fork.

Concretely, the seam is a **capability set** passed in, not compiled in:

```ts
createMarkdownEditor({
  docPath,                    // resolves relative images/media (exists today)
  links?:   LinkProvider,     // resolve/complete/preview [[…]] — Garrulus supplies a vault,
                              // Corvus supplies nothing and wikilinks stay plain text
  tags?:    TagProvider,      // #tag completion, or absent
  actions?: BlockActionProvider, // the "apri in Picus" button on a ```sql fence
  toolbar?: ToolbarSpec,      // which groups appear, or none at all
  readOnly, theme, keymap,
})
```

Everything that knows about a *vault* is behind `LinkProvider` / `TagProvider`. The editor
itself knows markdown and nothing else. That is the difference between an editor Corvus can
adopt and one it would have to copy.

Where it lives: `src/lib/components/shared/ui/markdown-editor/` — the `shared/ui/` tier by
the CLAUDE.md rule, because with the providers factored out it is genuinely app-agnostic.

### 7.1 What exists, and its honest state

`src/lib/utils/markdown-editor.ts` is genuinely good and genuinely mis-housed: 1539 lines in
one file (the guidance is ~300 for `.svelte`, small-and-focused for `.ts`), reachable only
through a modal (`shared/MarkdownEditorModal.svelte`, opened from the Corvus files panel),
with no toolbar, no command surface, and no concept of a vault.

Step zero of Garrulus is therefore a **refactor, not a fork**: split
`utils/markdown-editor.ts` into `utils/markdown-editor/` (`preview.ts`, `tables.ts`,
`media.ts`, `theme.ts`, `commands.ts`, `index.ts`) preserving behaviour, and keep it shared —
Corvus's modal keeps working and immediately inherits everything Garrulus adds. Forking it
would guarantee two markdown editors that drift, which is precisely the failure mode CLAUDE.md
names.

### 7.2 What the editor is missing

Grouped by how much they matter:

**Must have (M1).**
- `[[wikilink]]` decoration: rendered as the title, `Ctrl+Click` / `Enter` to follow, unresolved
  links styled differently and creating the note on follow.
- `[[` autocomplete over the index, fuzzy, with heading (`[[Note#Heading]]`) and block refs.
- `#tag` decoration + autocomplete.
- **A toolbar.** The explicit gap in the brief. Contextual, IntelliJ-flavoured: block type
  (H1–H6 / quote / list / code), bold / italic / strike / inline code, link, image, table,
  callout, task. It must be *thin and not the primary interface* — every button is a keybinding
  first, and the toolbar shows the keybinding in its tooltip.
- **The shortcuts**, table in §9.
- Callouts (`> [!note]`, `[!warning]`, `[!tip]` …) rendered as boxes, folding.
- Frontmatter rendered as a *form card* at the top of the note rather than as raw YAML, with
  a toggle to see the source.

**Should have (M2).**
- Heading folding + sticky heading breadcrumb + minimap — three existing extensions, wired.
- Hover preview of a linked note (`bennu-hover.ts` is the working model).
- Paste handling: an image from the clipboard is written into the attachments folder, named,
  linked and committed; a URL pasted over a selection becomes a link; HTML pasted becomes
  markdown.
- Footnotes, task due-date decoration, `%%comments%%`.
- Outline panel from the heading tree (`markup-outline.ts` already parses this).

**Nice (M3+).**
- Math (KaTeX) and mermaid — mermaid especially, given how much of this vault will be design
  notes. **Both need a new dependency → ask before adding** (CLAUDE.md rule 7).
- Multi-cursor, split panes, linked panes.
- A "typewriter mode" / focus mode.

### 7.3 What we recycle, concretely

The point of this table is that the second column is mostly *existing files*, not analogies.

**From Bennu** — Bennu is an IDE, and an IDE and a note vault are the same shape: many files,
symbols, references, navigation, refactors.

| Bennu | Becomes |
|---|---|
| `BennuGotoModal` | **Quick switcher** (`Ctrl+O`) — open a note by fuzzy title |
| `BennuFindInFilesModal` | **Vault search** with results + preview + jump |
| `BennuStructurePanel`, `BennuSymbolList`, `markup-outline.ts` | **Outline** panel for the current note |
| `BennuRenameModal` + `rename-apply.ts` | **Rename a note and update every link to it**, with a preview of what changes — the refactor machinery is already there |
| `BennuIntentionsOverlay`, `bennu-intentions.ts` | **`Alt+Enter` on prose**: extract selection to a new note (leaving a link), promote to a type, convert to callout, turn a line into a task, link an unlinked mention |
| `BennuTodoPanel` | **Tasks across the vault**, grouped by note/type/due date |
| `BennuProblemsPanel` | **Vault problems**: broken links, orphan notes, missing attachments, duplicate titles, notes with no type |
| `bennu-hover.ts` | Hover preview of a link |
| `BennuBottomDock`, `BennuStatusBar`, `BennuSidebar` | Window chrome |
| `BennuWorkspaceSwitcher` / `ManagerModal` | **Vault switcher** |
| `BennuIndexInspectorModal` | Link-graph inspector (power tool / debugging) |

**From Picus** — Picus is the newest shell and the closest structural match: a window over a
repository of files on disk, with tabs, a tree, a toolbar and typed documents.

| Picus | Becomes |
|---|---|
| `PicusShell` + `shell/{TitleBar,TabBar,Toolbar,StatusBar}` | The Garrulus shell, near-identically |
| `picus-palette.ts`, `picus-shortcuts.ts`, `PicusShortcutsModal` | Command palette + keybinding registry + the shortcuts sheet |
| `picus/project` crate (`discover`, `tree`, `marker`, `naming`, `path`, `resolve`) | **Vault discovery**: find the vault by its `.arbor/garrulus/` marker, build the tree, resolve paths, generate filenames from a pattern. This crate is 80% of `garrulus-vault`'s plumbing already written. |
| `ClassifyFileModal`, `file-classify.ts`, `folder-classify.ts` | **Type assignment**: "which type is this note / this folder" — the same interaction, a different taxonomy |
| `QueryResultPanel` (editable grid) | **Table view** over a note query |
| `generate/` (`TargetEditor`, `SqlPreview`, form → preview → write) | **Template instantiation**: fill the fields, preview the note, create it |
| `PicusDocsPanel` + `docs/*.svelte` | In-app docs (mandatory per CLAUDE.md) |
| `PicusSettingsModal`, `PicusAboutModal`, `PicusNavigateTo` | The same, renamed |

**From Corvus**: `markdown-editor.ts` and `utils/markdown.ts` (§7.1), `DiffViewer` for
conflicts and note history, and the entire `corvus-git` crate as the sync engine.

**From shared**: `Modal`/`ConfirmModal`/`FilePickerModal`, `CommandPalette`, `Toast`,
`Tooltip`, `ContextMenu`, `ResizablePanel`, `Tree`, `SearchBar`, `Tabs`, `Badge`, `EmptyState`.

### 7.4 The Arbor advantage: things Obsidian structurally cannot do

Worth stating explicitly, because these are the reasons to use Garrulus rather than
Obsidian-plus-a-git-plugin:

- **Version history per note, free.** `git log -- note.md`, restore any revision, diff any
  two — with the `DiffViewer` the suite already has. Obsidian charges for this too.
- **Corvus cross-links.** A bug note carries a `commit` / `branch` / `MR` field; clicking it
  opens Corvus at that object. Corvus's `on_commit` hook can offer *"annota questo commit"*.
  Deep links both ways. No other note app is in the same process tree as your git client.
- **A vault plugin API on day one.** The Lua host already exists; Garrulus just declares
  hooks: `on_note_created` / `on_vault_note_saved` / `on_note_renamed` /
  `on_vault_note_deleted` (vault-namespaced — `on_note_saved` / `on_note_deleted` are
  Corvus's *git note* hooks and hook names are one global namespace),
  `on_sync_started` / `on_sync_done` / `on_sync_conflict`, `on_type_applied`,
  `on_vault_opened` / `on_vault_closed`. Auto-tagging, custom exports, integrations — all user-scriptable, and
  the marketplace plumbing is already built.
- **Quick capture from the tray.** A chromeless capture window (Tyto's `RecordingHud` is the
  working model) on a global shortcut: type a line, it appends to today's daily note, closes.
  Never open the app to write one sentence down.

---

## 8. Feature catalogue

Staged so that **M1 is independently worth using** — the test being "could this replace
Obsidian for the two-PC workflow, even while ugly".

### 8.1 M1 — a vault that syncs (the product's reason to exist)

- Vault open / create / switch; discovery via the `.arbor/garrulus/` marker; file tree.
- Editor: the recycled live preview + wikilinks + `[[` autocomplete + tags + toolbar + the M1
  shortcuts + callouts + the frontmatter form card.
- Index: titles, links, backlinks, tags, unresolved links.
- Panels: Backlinks, Outline, Tags, Search, Sync.
- Parser + AST: the Tree-sitter markdown grammar, `Document`/`Block`/`Inline`, the
  `MarkdownReader` / `MarkdownWriter` pair.
- **Sync**: `SyncRemote` trait, `GitRemote`, the **title-bar sync button** with its state
  machine (§4.3), background fetch-only probe, private-by-default remote creation, the
  conflict flow of §4.4.
- Quick switcher, vault search, command palette, shortcuts sheet, in-app docs.
- Note types: schema, matching, templates, the "new note of type X" flow, the built-in seven.
- **HTML export** — a `Writer`, self-contained single file, styled with the app theme.

### 8.2 M2 — the knowledge base

- Table view and board view over a note query.
- Tasks panel across the vault; due dates.
- Vault problems panel; rename-with-link-update; `Alt+Enter` intentions.
- Note history + restore + diff (git-backed).
- Attachments: paste-to-file, an attachments panel, an orphan-attachment check.
- Daily notes + quick capture window.
- Hover previews, folding, sticky headings, minimap.
- `FolderRemote`.
- Session handoff between devices; "novità dall'altro PC" digest.
- **PDF export** (§8.4).

### 8.3 M3 — the differentiators

- Graph view, filtered and coloured by type/tag.
- Corvus cross-links both directions; deep links.
- Plugin hooks + a Garrulus SDK namespace.
- Embeds (`![[note]]`, `![[note#heading]]`), block references.
- Templates with logic (a `{{query}}` placeholder that inlines a live note list).
- Whole-vault static site export with client-side search.

### 8.4 Export

Both formats are `Writer`s over the AST (§3.4), which is what keeps them honest: a heading is
a heading, not a regex that also matched a `#` inside a code fence.

**HTML (M1).** One self-contained file: inlined CSS (the app theme, light *and* dark via
`prefers-color-scheme`), images embedded as data URIs, wikilinks resolved to anchors within
the export or dropped to plain text when the target is outside it. Scope: this note, this
folder, this query result. Cheap, and it is the format you actually paste into a ticket or
send to somebody.

**PDF (M2).** No Rust PDF library — laying out markdown by hand is a bad trade. Two options,
in order of preference:

1. **Print the HTML.** Render the HTML export into a dedicated hidden window with a proper
   `@media print` stylesheet and trigger printing; the user picks "Save as PDF". Zero new
   dependencies, works today, one extra click, and the output is genuinely good because the
   styling is real CSS with page-break control (`break-inside: avoid` on code blocks and
   tables, repeated table headers, a footer with the note title and page number).
2. **Silent print-to-PDF** as the upgrade: WebView2 exposes `PrintToPdf`, WKWebView exposes
   `createPDF`. No dependency either, but it is per-platform glue in `src-tauri` — worth
   doing once the feature is proven, not before.

Option 1 for M2, option 2 when it starts to annoy. A third option — the `typst` crate, pure
Rust and typographically excellent — is real but a heavy dependency for one feature; noted,
not proposed.

### 8.5 Further proposals

Grouped by what they are *for*, each tagged with where it would sit. These are proposals, not
commitments — the point is to pick, not to build all of them.

**Zero-friction capture** — the thing that decides whether a vault gets used at all.

- **Inbox scratch** *(M2)* — one keystroke opens an unnamed, unfiled buffer. It asks nothing:
  no title, no folder, no type. Later, *"archivia come…"* applies a type and files it. Most
  notes die because the app asked three questions first.
- **Quick capture from the tray** *(M2)* — a chromeless window on a global shortcut that
  appends a line to today's daily note and closes. Tyto's `RecordingHud` is the working model.
- **Inline link autocompletion** *(M2)* — typing the title of an existing note offers the
  wikilink as a greyed proposal accepted with `Tab`, using the `inline-completion.ts`
  extension that already exists. Links get made because making them costs nothing.
- **Unlinked mentions** *(M2)* — a note's title appearing as plain text elsewhere shows up in
  its Backlinks panel under "menzioni non collegate", with a one-click link.

**A vault that knows it belongs to a developer** — the reason to write this instead of
installing Obsidian and a git plugin.

- **Commit-pinned code links** *(M3)* — a note can link `src/lib/foo.rs:120` in a Corvus
  repo, resolved through git and **pinned to a revision**. Open the bug note six months later
  and the link still points at the right line, because it points at the line *as it was*.
  With a "vai alla versione attuale" affordance next to it. No other note app can do this.
- **Actionable code blocks** *(M3)* — a ```` ```sql ```` block gets an "apri in Picus" button, a
  ```` ```merula ```` block "suona in Merula", a ```` ```java ```` block "apri in Bennu". The whole
  suite is one process tree away; not using that is leaving the best part on the table.
- **Screenshot into the note** *(M2)* — an action that calls Tyto, captures a region, writes
  it into the attachments folder and inserts the embed. For bug notes this *is* the workflow.
- **Spell check** *(M2)* — `bennu-intel/src/spell.rs` is a working EN+IT Hunspell engine with
  a tech allowlist and per-project custom dictionaries, already built and already tuned for
  low noise. On prose it needs a different tokenizer and nothing else. Excellent recycling.
- **Corvus hook: "annota questo commit"** *(M3)* — `on_commit` offers to open a note
  pre-linked to what was just committed. Decision records get written when writing them is
  one click at the moment you have the context.

**Finding things again** — a vault's value is entirely in retrieval.

- **Structured search, saved as views** *(M2)* — beyond full text:
  `type:bug status:aperto app:corvus sort:severity`. Because types are *typed*, the query can
  be validated and autocompleted instead of failing silently — the advantage over Dataview.
  A saved view is a sidebar entry; the result renders as a list, a table or a board.
- **Vault home** *(M2)* — a `Home` note with live embedded queries: bug aperti, task in
  scadenza, note toccate di recente, note orfane. The first thing the window shows.
- **Pinned + recent** *(M1)* — pinned notes and recents at the top of the sidebar. Trivial,
  used constantly.
- **Related notes** *(M3)* — "queste note condividono tag e link con questa". Cheap
  co-citation over the graph; no ML, no magic, genuinely useful.

**Keeping the vault healthy** — a knowledge base rots without maintenance tools.

- **Vault trash** *(M1)* — deleting a note moves it to `.arbor/garrulus/trash/` (and OS trash
  via `arbor-fs::trash`), recoverable without going through git. Deleting a note is scary in
  a way deleting a file is not.
- **Attachment GC** *(M2)* — find attachments nothing references, propose them for the trash.
  This is also the mitigation for the repo-bloat risk (§11.1).
- **Split and merge notes** *(M3)* — "spezza alle `##`" produces one note per section with
  links left behind; the inverse inlines a note into its referrer. AST operations, and the
  direct analogue of Bennu's extract-method — the refactor UI is already there.
- **Checkpoint** *(M3)* — "segna questo stato del vault" writes a lightweight git tag you can
  return to. A safety net before a big reorganisation.

**Writing** — smaller, but this is where the hours actually go.

- **Presentation mode** *(M3)* — `---` splits slides, one key presents. For showing a game
  design pillar to somebody without exporting anything.
- **Follow-ups** *(M3)* — a task with `follow-up: 2w` disappears and returns to the Tasks
  panel in two weeks. "Ripensare a questa meccanica tra un mese" is a real need that plain
  checkboxes cannot express.
- **Focus mode + writing stats** *(M3)* — typewriter scrolling, everything but the current
  paragraph dimmed; word count, reading time, and a "quanto ho scritto questo mese" chart —
  Corvus's `stats.rs` already sets the rendering discipline for that last one.

**Interoperability**

- **Import** *(M3)* — from Obsidian it is nearly a no-op (same format); the wizard's real job
  is turning Obsidian templates into Garrulus note types. Notion (md + csv), Joplin and Bear
  exports are each a `Reader` plus a mapping, which is exactly what §3.4 was built for.

### 8.6 Later / to decide

- **Encryption for a marked folder.** The remote is private, but it is still somebody else's
  computer; a `private/` folder encrypted before it leaves the machine is a reasonable ask.
  Needs a crypto dependency → a decision, not an assumption.
- Canvas / whiteboard.
- Web clipper.
- Mobile — out of scope; the answer is "the vault is markdown in git, read it with anything".

---

## 9. Shortcuts and command palette

Following Picus's conventions exactly (`Ctrl+K` palette, `Ctrl+B` sidebar, `Ctrl+J` bottom
dock, `Ctrl+,` settings, `F1` docs, `Ctrl+1..n` sections). Never `Ctrl+Alt+<letter>`.

| Keys | Action |
|---|---|
| `Ctrl+O` | Quick switcher — open a note by title |
| `Ctrl+K` | Command palette |
| `Ctrl+Shift+F` | Search the vault |
| `Ctrl+N` | New note · `Ctrl+Shift+N` new note **of a type** (type picker first) |
| `Ctrl+S` | Save now (also flushes the sync debounce) |
| `Ctrl+Shift+S` | Sync now |
| `Alt+Enter` | Intentions on the selection / line |
| `Ctrl+Click` / `Enter` on a link | Follow · `Ctrl+Shift+Click` opens in a split |
| `Alt+←` / `Alt+→` | Back / forward through visited notes |
| `Ctrl+1..5` | Files · Search · Backlinks · Tasks · Sync |
| `Ctrl+B` / `Ctrl+J` | Sidebar / bottom dock |
| `Ctrl+E` | Toggle rendered ↔ source for the whole note |
| `Ctrl+D` | Open today's daily note |
| `Ctrl+Shift+K` | Insert a link (`[[` picker) · `Ctrl+Shift+T` insert a table |
| `F2` | Rename this note and update every link to it |
| `F1` / `Ctrl+,` | Docs / settings |
| Global (OS-level) | Quick capture into today's note — combination TBD |

**Inline formatting: an `Alt` family.** `Ctrl+B` is the left sidebar suite-wide and stays
that way; `Ctrl+Shift+B` is already the *right* sidebar in `keybindings.ts`, so bold cannot
have either. Rather than scatter formatting across whichever `Ctrl+Shift+<letter>` happens to
be unclaimed, the whole family goes on one modifier so it is learnable as a group:

| Keys | | Keys | |
|---|---|---|---|
| `Alt+B` | Bold | `Alt+S` | Strikethrough |
| `Alt+I` | Italic | `Alt+H` | Highlight (`==`) |
| `Alt+C` | Inline code | `Alt+Q` | Quote / callout |
| `Alt+L` | Link | `Alt+T` | Task checkbox |

`Alt+<letter>` is safe on IT/DE/FR/ES layouts — only `Ctrl+Alt` is forbidden, because that is
AltGr. Link is `Alt+L` rather than `Alt+K` so it is not read as a cousin of `Ctrl+K`, the
palette. Every one of these is on its toolbar button's tooltip, so the family is discoverable
without reading this table.

Two more worth reserving: `Ctrl+Shift+Space` for the inbox scratch buffer, and `Ctrl+Alt`-free
`Alt+Shift+E` for export (the palette carries the format choice).

Every entry above also exists in the command palette, plus the verbs with no keybinding: open
vault, add remote, resolve conflicts, rebuild index, export, apply type, insert template.

---

## 10. Sizing

Rough, in units of "a focused session", assuming the recycling actually lands:

| Milestone | Size | Where the time goes |
|---|---|---|
| M0 — BE scaffold, window, canopy entry, `be_ping` | small | mechanical, §8 of the BE doc |
| M1 — grammar + AST + Reader/Writer | medium | the grammar is vendored; the model and its round-trip tests are the work |
| M1 — vault + index + editor upgrades + types | large | the `markdown-editor.ts` cleanup, then wikilinks/autocomplete |
| M1 — sync + the button | medium-large | the engine is small; **conflicts and reload-on-pull are the work** |
| M2 | large | mostly recycled UI, wired to a new model |
| M3 | large | graph view and the plugin surface dominate |

M0+M1 is the honest MVP and the only part that needs to be committed to now.

---

## 11. Risks and landmines

1. **Attachments bloat the git history.** Images pasted into notes are binaries in a repo
   forever. Mitigation: an attachments size cap with a warning above it, an
   attachment-size-by-folder report, and a documented "the vault repo is not for 200 MB
   videos". Git LFS is a possibility but adds a hard dependency on remote support — flagged,
   not assumed.
2. **The conflict flow is the whole product.** If it loses text once, the user stops trusting
   it and the product is dead. It gets the tests, the fixtures and the paranoia.
3. **Frontmatter round-tripping.** Reformat YAML on save and every note becomes a diff.
   Hard invariant, tested.
4. **`garrulus-sync` → `corvus-git` is a product-to-product crate edge**, which the workspace
   has not needed before. **Decided: `corvus-git` stays under `corvus/` for now.** It is
   already Tauri-free and self-contained — a library that happens to live there — so the edge
   is accepted and documented rather than paid for with a move that touches Corvus. If a
   third consumer ever appears, `crates/foundation/git` is a rename away, not a refactor.
5. **File watching.** Notes change under the app (the sync engine, Obsidian running on the
   same folder, the other PC). `notify` is currently a `src-tauri`-only dependency, so
   `garrulus-be` watching the vault means a **new dependency in a new crate → must be asked
   for**, or the alternative is polling on the sync tick, which is worse but free.
6. **`markdown-editor.ts` is shared with Corvus.** Every change to it is a change to Corvus's
   modal. That is the point (one editor, one set of bugs), but it means the refactor in §7.1
   has to be behaviour-preserving and the Corvus modal is part of the test surface.
7. **Scope.** This is a product, not a feature. The staging exists so it can stop at M1 and
   still have been worth doing.

---

## 12. Decisions

### Settled

1. **A Tree-sitter markdown grammar and a format-agnostic AST**, even though nothing today
   strictly needs them — because other input formats are a real future ask, and because half
   of §8 gets cheap as a result (§3.4).
2. **No timed auto-commit.** One title-bar button, carrying the state (`has-changes`,
   `behind`, `diverged`, `conflict`, …); the background only *fetches*, and never writes
   (§4.2–4.3).
3. **`corvus-git` stays under `corvus/`** for now (§11.4).
4. **Project-scoped state lives in `<project>/.arbor/<product>/`** — one dot-folder per
   project across the whole suite, from here on (§3.3).
5. **HTML and PDF export** are first-class, both as AST `Writer`s (§8.4).
6. **Remotes are created private, with no public option in the UI** (§4.2).
7. **`Ctrl+B` stays the sidebar**; inline formatting moves to an `Alt` family, bold on
   `Alt+B` (§9).
8. **The markdown editor becomes product-agnostic** and moves to
   `shared/ui/markdown-editor/`, with vault knowledge behind `LinkProvider` / `TagProvider`
   so Corvus and Bennu can mount it later without a fork (§7.0). Its cleanup is step zero.
9. **Obsidian compatibility is a hard constraint** (§1): the escape hatch is worth the
   format discipline.
10. **The Tree-sitter markdown grammar is approved** as a vendored dependency.
11. **File watching is approved**: `notify` in `garrulus-be`, watching the vault.
12. **Callouts are M1**, not a nicety: `> [!NOTE|TIP|INFO|WARNING|DANGER|QUESTION|EXAMPLE|QUOTE]`,
    rendered as boxed blocks with per-kind icon and accent, foldable, `> [!WARNING]-` collapsed
    by default. Obsidian-compatible spelling.
13. **Typed metadata with a filter surface is M1**, not M2 — frontmatter fields are editable
    from a panel, and every field is a filter axis in search, in the sidebar and in the table
    view (§6).
14. **Find-in-vault is a first-class view**, explicitly rebuilt rather than copied from
    Picus's (§8.5, "Ricerca strutturata"). The Picus one is the anti-pattern to avoid, not the
    model to follow — flagged separately as worth improving in Picus itself.
15. **The dependency graph ships last** and is understood to be mostly a toy (§8.3).
16. **Records are notes, not database rows** — bugs and improvements stay markdown with typed
    frontmatter, because git cannot merge a binary database and a row cannot be a node in the
    link graph. The index may become SQLite later, behind its existing API, on one of three
    named triggers (§5.3).

### Still open

1. **Encryption for a `private/` folder** before it leaves the machine — in scope at all?
   Assumed out for now, but the remote being private does not make it *yours* (§8, Later).
2. **Math (KaTeX) and mermaid** need new frontend dependencies. Assumed deferred to M3 and
   asked for then.
3. **One repo per vault**, and multiple vaults = multiple repos? Assumed yes.
4. **The canopy accent** `#3fb6d9` for Garrulus (§3.2).
