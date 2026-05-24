# Project status

Arbor is under active development. This page tracks what's stable, what's still settling, and what isn't implemented yet.

## Stable

Used day-to-day, broad coverage, considered reliable.

- **Commit graph** — lane assignment, virtual scrolling, search, jump-to-commit
- **Staging** — file-level, hunk-level, and line-level partial staging; commit, amend
- **Branches** — create, checkout, delete, push, fetch, pull (with auto-stash on dirty pull), tags, stash
- **Three-way merge editor** — for merge, revert, cherry-pick, stash apply, and MR conflict resolution
- **Submodules** — fetch, pull, push, branch, open in a tab
- **Reflog** and **recovery journal**
- **Bisect** with persistent sessions, banner, sidebar history
- **Initialize repository** — gitignore/license templates, optional remote creation via GitHub/GitLab API
- **Clone** with progress and OAuth/SSH credentials from the OS keyring
- **Workspaces** — multi-repo grouping, tab snapshots, bulk *Fetch all*/*Pull all*/*Tag all*, import/export
- **Tabs**, missing/relocated project handling
- **Pull/Merge requests on GitHub and GitLab** — list, detail, create, merge (squash/rebase), close, reopen, mark-ready, auto-merge, comments, activity timeline (bot events), CI checks, file diffs, commit drill-down
- **CI/CD** — list, retrigger, create dispatch with variables (GitHub Actions and GitLab CI; self-hosted GitLab supported)
- **Security Dashboard** (per-repo) — GitLab Vulnerability Report (GraphQL) and GitHub GHAS/Dependabot/Secret Scanning (REST)
- **Issue trackers** — Linear and Jira, with automatic ticket detection from commit messages. Tested combinations to date: **Jira Data Center with API token**, **Linear with OAuth**. The other auth combinations (Jira Cloud, Linear with personal API key) share the same code paths and should work, but haven't been exercised by the maintainer.
- **Theme editor** with 19 bundled presets
- **Command palette** (verb-first, two-phase)
- **Background jobs panel** with streaming output
- **Notifications**
- **Integrated terminal** (multi-shell)
- **Statistics**
- **Lua plugin runtime** — hooks, schedulers, settings, keybindings, UI contributions, sandbox
- **OAuth flows** for GitHub and GitLab (including refresh and self-hosted GitLab)
- **Docs panel** with HTML and Markdown export

## Functional, less time in production

Wired into real workflows but with less mileage than the stable set. Expect occasional rough edges.

- **Linked Worktrees** — cross-project branch synchronisation via alias groups
- **Vetoable `on_pre_commit` hooks**, the pre-commit message validation API used by the `commit-validator` plugin
- **GitFlow** start/finish actions for feature, release, hotfix, and support branches. Currently opinionated about prefixes, base branches, and finish behaviour; a more configurable version is on the [roadmap](roadmap.md).

## Experimental

Recent additions. Functional but not broadly tested; APIs may change.

- **Theme and branding overlay APIs** — `arbor.ui.set_branding`, `arbor.ui.set_theme_tokens`, `on_theme_changed` hook
- **`arbor.repo.clone`** API (background clone with streaming progress)
- **Plugin schedulers with `only_when_focused`**
- **Workspace-aware automation** — plugin schedulers keyed to the active workspace

## Plugins

Arbor does not bundle any plugins in the binary. The plugin runtime and Marketplace ship empty — every plugin, including the ones that power workflows like build runners, format studios, source export, and the Lua-driven UI contributions, is installed on demand from the [arbor-extensions](https://github.com/nightprint-studio/arbor-extensions) registry (or from any custom GitHub source you point Arbor at).

Per-plugin maturity, descriptions, and screenshots live alongside each plugin in the registry — they're resolved straight from the source repo, so they're always in sync with the code that's actually shipping.

> **Plugin API stability.** The plugin runtime and the host-side Lua API surface are still settling. Breaking changes between releases are possible — manifest fields, hook signatures, and `arbor.*` namespaces may be renamed, restructured, or removed when a cleaner shape emerges. Plugins maintained inside the [arbor-extensions](https://github.com/nightprint-studio/arbor-extensions) registry are kept in sync; out-of-tree plugins may need small adjustments after a release.

The plugin development reference (manifest schema, hooks, full Lua API surface) lives in the in-app **Docs** panel.

## Known gaps

Not implemented yet, but on the list.

- **Visual interactive rebase** — drag-and-drop reordering with squash/edit/drop/fixup actions and a timeline preview. Today rebase delegates to the `git` CLI.

See [roadmap.md](roadmap.md) for what's planned next.

## Not planned

Features that have been considered and are intentionally out of scope. They may be reconsidered if a strong use case emerges.

- **Inline PR review.** Line-anchored review comments, formal *Submit review* (Approve / Request changes / Comment as a batched action), GitHub suggested-changes apply, mark-file-as-viewed, thread resolve. The general PR/MR comment timeline, activity feed, and CI checks panel are supported; the per-line review-submission layer isn't.
- **GPG / SSH commit-signature verification.** Surfacing a *verified* badge on signed commits, configuring the signing identity from the UI. Push and clone use OAuth or SSH credentials from the keyring; commit-signing trust chains are separate and not wired up.
- **Sparse checkout and partial clone.** Controlling which paths or blobs are materialised in the working tree. Useful for very large monorepos; the operation can still be driven from the integrated terminal.
- **Tag signing and annotated vs lightweight tag distinction.** Arbor creates tags with the default settings. Choosing between lightweight and annotated, attaching a tag message, or signing a tag isn't surfaced in the UI.

## Reporting issues

When opening a bug report, please include:

- the feature involved, ideally tagged stable / functional / experimental from the lists above
- minimal reproduction steps
- platform (Windows, macOS, Linux) and Arbor version
- relevant log output if the problem involves a plugin or a CI/security integration
