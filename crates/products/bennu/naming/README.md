# bennu-naming

A declaration whose name breaks the project's convention — and the name that would not.

## The one idea

A convention is a **function from words to a spelling**, not a pattern that accepts or refuses one.
`get_user_name` and `getUserName` are the same three words; a rule that can render those words as
camelCase can both *detect* the violation and *repair* it, and it is the same code doing both.

That is why `Convention` is an enum and not a regex. A regex can refuse a name; it cannot build
one, and a naming check that cannot build one is a list of complaints. `accepts(name)` is defined
as `render(name) == name`, so there is one description of the rule in the crate: the check and its
quick-fix cannot drift, the message names the rule that was actually applied, and the fix is
idempotent by construction — a rendered name is a fixed point of `render`.

Two things it deliberately preserves, both learned from what a Java project actually contains:
acronyms (`serialVersionUID` is already camelCase, and rewriting it to `serialVersionUid` is a rule
nobody asked for) and leading/trailing underscores (`_unused` is a marker, not a spelling).

## Everything is off by default

`enabled = false`, and every unset target is `any`. A legacy tree greeted with three thousand weak
warnings on first open is not a code-quality signal, it is a reason to switch the feature off and
never look at it again. The project opts in, per target, and the guards are ordered so that off
costs nothing: a project that has not opted in never reaches a grammar.

```toml
# <repo>/.arbor/bennu/config.toml
[naming]
enabled = true
ignore  = ["**/generated/**"]

[naming.rules.java]
type     = "PascalCase"
method   = "camelCase"
constant = "UPPER_SNAKE_CASE"
```

The value is its own example: `"camelCase"` *is* what camelCase looks like, so the file explains
itself and a dropdown needs no second column.

## Shape: a feature pack, with two ways to see a declaration

A leaf — declarations in, `Diagnostic` out. It knows nothing about projects, the index, the
resolver or the filesystem, which is what lets it be switched off for nothing and, later, be
something a user installs rather than something the editor always carries. `bennu-be`'s `naming`
module is the glue that gives it a project: where the config lives, which project owns a file, and
the cache that keeps a project-wide pass from re-reading the same TOML once per file.

A language supplies its declarations one of two ways, and the difference is not an implementation
detail — it changes which targets can fire and how a fix may be applied:

| | `DeclSource::Grammar` | `DeclSource::Symbols` |
|---|---|---|
| Source | a tree-sitter grammar parsed here | `textDocument/documentSymbol` |
| Languages | Java | TypeScript, JavaScript, Rust |
| Sees | everything — types, members, **locals, parameters**, type parameters, package segments | types and their members only |
| Needs | nothing | the language server installed and warm |
| Fix applied unseen | for locals and parameters | never |

Java is grammar-backed because Bennu's Java engine is its own and there is no server to ask — which
also makes it the only pack with no blind spots. The rest ride the outline Bennu already fetches for
the Structure panel, so they cost no new grammar and no new dependency.

Go and C++ are deliberately absent: their conventions are not a function of the declaration alone.
Go spells the same kind of thing `Foo` or `foo` depending on whether it is exported; C++ has no
single community convention. A rule that cannot decide from the declaration reports false
positives, and a naming check that cries wolf is one nobody leaves switched on. For the same
reason, `variable` and (outside Rust) `constant` are unmapped kinds: a module-level binding in
TypeScript is `camelCase` or `UPPER_SNAKE_CASE` depending on what the author considers a constant,
which is not something the declaration carries.

| Module | What |
|---|---|
| `words` | identifier → sub-words; the split every convention is defined on |
| `convention` | the conventions, as renderers |
| `target` | what a convention applies to, and whether its rename can leave the file |
| `config` | the `[naming]` section, and the path globs that exclude a file |
| `pack` | the seam a language plugs into, and the registry of packs |
| `java` | the Java pack — the grammar walk |
| `symbols` | the server-backed packs, and the outline → declarations mapping |
| `skip` | generated code, by location and by banner |
| `scan` | both entry points, and the violations they produce |

Every public item is re-exported through `prelude` — the canonical call-site path.

## Safety of the fix is a property of the declaration *and where it came from*

`Target::is_file_local` says a local or a parameter cannot be referred to from outside its file, so
renaming one is exact. That holds for a declaration a **grammar** found. It does not hold for one an
**outline** reported: an outline contains top-level and member declarations, so something it calls a
variable is exactly what another file imports. `Pack::fix_is_file_local` combines the two, and every
caller goes through it — getting this wrong would rename an exported symbol across a project with no
preview.

Everything else — a method, a field, a type — can be reached by a caller, by reflection, or by a
framework binding a name out of an XML or JSP file that no grammar here reads. Those are offered one
at a time, through the project's real rename engine, and never rewritten in place.

## What the Java pack does not report

Constructors (the name is the class's — fixing it means renaming the type, which is already
reported there), `@Override` methods (the name is the supertype's, very often the JDK's, and a
diagnostic whose only honest fix is "rename something you do not own" is noise), and JDK-mandated
names like `serialVersionUID`.
