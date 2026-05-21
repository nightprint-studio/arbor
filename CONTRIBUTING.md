# Contributing to Arbor

> ## Pull requests are not being accepted yet
>
> Arbor has just gone public and the project is still finding its shape: plugin API, architectural boundaries, conventions, release cadence. For an initial period I'm keeping merges to maintainers only, so this groundwork can settle without juggling external patches at the same time.
>
> **What this means in practice:**
> - **Issues are open and very welcome.** Bug reports, feature suggestions, plugin ideas, design feedback, questions.
> - **PRs opened in the meantime will be closed politely**, not merged. Please don't invest time in a fully-built patch yet; even a great one will end up in the closed pile.
> - This is a temporary phase, not a permanent policy. The rest of this document describes the workflow that will apply once PRs open up; it stays here as a preview of what to expect.
> - When PRs open, this banner will be removed and an announcement will go on the repo.

If you want to flag something now, open an [issue](https://github.com/nightprint-studio/arbor/issues). That's the channel that's live today.

---

The rest of this page covers the workflow that will apply once PRs are accepted. Reading it is optional for now.

## Reporting bugs

Open an issue with:

- Platform (Windows, macOS, Linux) and Arbor version
- Whether the area involved is listed as **stable**, **functional**, or **experimental** in [docs/status.md](docs/status.md)
- Minimal reproduction steps
- Relevant log output if the problem involves a plugin, OAuth, CI, or the security dashboard

Bugs in features marked *experimental* are still worth reporting, but please tag them as such. They get triaged differently from regressions in stable areas.

## Suggesting features

Open an issue first to discuss the idea. This avoids the situation where a fully-built PR gets closed because the direction wasn't a fit. Small, self-evident improvements are fine to open as a PR directly.

The roadmap is in [docs/status.md](docs/status.md) under *Known gaps*. Feature suggestions that align with what's already planned tend to get a quicker yes.

## Pull requests

For anything beyond trivial fixes or typo corrections, please open an issue first and link to it from the PR.

A few practical points:

- **One concern per PR.** Several small PRs are easier to review than one large one.
- **Keep the diff minimal.** No unrelated reformatting, dead-code cleanup, or "while I was here" refactors mixed into a feature PR.
- **Match existing patterns.** If a similar piece of code already exists, follow its conventions rather than introducing a parallel approach.
- **Run the tooling.** `cargo fmt`, `cargo clippy`, and `yarn check` should all be clean before pushing.
- **Comments.** Default to no comments. Add one only when the *why* isn't obvious from the code.

PR descriptions should explain what changes and why, list anything reviewers should look at carefully, and note how it was tested.

## Building from source

See the *Installation* section of the [README](README.md). For development:

```bash
cargo tauri dev    # hot-reload Svelte, recompile Rust on change
cargo check        # fast Rust type-check
cd src-tauri && cargo test
```

`cargo tauri dev` is the primary loop. UI changes hot-reload; Rust changes recompile.

## Code conventions

- **Rust.** Formatted with `cargo fmt`. `cargo clippy` should pass without warnings.
- **TypeScript / Svelte.** Svelte 5 runes API (`$state`, `$derived`, `$effect`), not the Svelte 4 store API.
- **Language.** Code, comments, commit messages, PR descriptions, and issue titles in English.
- **CSS.** Use the design-system CSS custom properties (`var(--bg-base)`, `var(--accent)`, …); avoid hard-coded colours.
- **Errors.** Backend errors flow through `AppError` in `src-tauri/src/error.rs`; frontend errors via `uiStore.showToast`.
- **IPC.** Every new Tauri command needs a matching typed wrapper in `src/lib/ipc/` and any new Rust struct mirrored in `src/lib/types/`.

## Contributing plugins

Plugins live in `plugins/<name>/` next to the executable, with a `plugin.toml` manifest and a `main.lua` entry point. The full plugin API reference lives in the in-app **Docs** panel (Plugin Development section).

Plugins shipped with Arbor are kept in `plugins/` in this repository. If you'd like a third-party plugin to be considered for inclusion, open an issue to discuss it before sending a PR.

## Licensing

Arbor is released under the [GPL-3.0](LICENSE) license. By submitting a contribution, you agree that your work is licensed under the same terms.

There is no Contributor License Agreement (CLA) to sign. The project does not require copyright assignment — your contributions remain yours, licensed under GPL-3.0 like the rest of the codebase.

## Code of conduct

Be respectful in issues, pull requests, and discussions. Disagreements about technical direction are part of the process. Personal attacks, harassment, or hostile tone are not. Maintainers reserve the right to moderate or close threads that go off the rails.

For anything that needs to be reported privately rather than aired in public, write to **nightprint.studio@gmail.com**.
