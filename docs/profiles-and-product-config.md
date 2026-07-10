# Profiles & per-product config

Status: **in progress** — Phases 1–2 done (foundation + config-model split).
Tracks the restructure of Arbor's on-disk config from one flat monolith into a
**profile × product** tree.

## Motivation

Today everything lives flat under `arbor/`: one monolithic `config.toml` mixing
generic UI prefs with git-specific settings, plus `workspaces.json`,
`repos.json`, `plugins/`, `marketplace_*`, `themes/`, … all in the same folder.
There is already an *ad-hoc* dev/prod split — the marketplace paths do
`switch("marketplace_cache.json", "marketplace_cache-dev.json")` — a hand-rolled
suffix scattered across call sites. That is the "structured badly" we are fixing.

Two things drive the new layout:

1. **Multiple products share the binary.** Arbor is becoming a launcher over
   several products: **corvus** (git, today's app), **merula** (music, today a
   sibling `merula/` dir), and future **merula** (db), **sitta**, **grove**. Each
   needs its own config bucket; generic UI prefs are shared across products.
2. **Profiles.** A profile is an isolated environment — its own settings, its
   own installed plugins, its own repos/workspaces — that the user switches
   between (dev vs demo vs client-X). It generalizes and removes the ad-hoc
   `-dev` suffix: `dev` simply becomes a profile.

## Target layout

```
arbor/
  active-profile                  # global pointer: name of the selected profile
  git/                            # portable git binary — global, shared
  profiles/
    default/                      # one profile
      profile.toml                # GENERIC, product-agnostic, per-profile:
                                  #   theme, appearance, animations, keybindings,
                                  #   onboarding, whats_new, explorer
      corvus/                     # git product
        config.toml               #   diff, graph, gitflow, branches, mr, issues,
                                  #   ticket_links, recovery, status, missing,
                                  #   commit, cache, terminals, ide, pipelines,
                                  #   studio, git(cli), recent_repos
        workspaces.json
        repos.json
        session.json
        graph_columns.toml
        linked_worktrees.toml
        pipeline_runs/
      merula/                      # music product
        config.toml
        ...
      plugins/                    # per-profile plugin area
        installed/                #   (was arbor/plugins/)
        marketplace_cache.json    #   (was arbor/marketplace_cache.json)
        marketplace_plugins/
        themes/
        toolchains/
        settings/
    dev/
      ...                         # same shape — replaces the `-dev` suffix hack
```

## Decisions (locked)

| # | Decision | Choice |
|---|---|---|
| Scope | How much to build | Full: layout **+** profile create/select/switch (BE + FE) |
| Boundary | What a profile isolates | Settings **+** plugins **+** repos/workspaces. Credentials (OS keyring) stay **shared** — namespacing the keyring is awkward and deployment-level. |
| Generic settings | per-profile or shared | **Per-profile** (`profile.toml` inside each profile). The point of a profile is a distinct look + keybindings + plugin set. |
| Path shape | folder nesting | `arbor/profiles/<profile>/<product>/` — explicit `profiles/` namespace avoids collision with global entries (`git/`, `active-profile`). |

### Global vs per-profile vs per-product

- **Global** (`arbor/` root): `active-profile` pointer, portable `git/`, OAuth
  `client_id` overrides (deployment identity, not a user pref).
- **Per-profile, product-agnostic** (`profiles/<p>/profile.toml`): theme,
  appearance, animations, keybindings, onboarding, whats_new, explorer,
  `plugins_enabled`.
- **Per-profile, per-product** (`profiles/<p>/<product>/`): everything tied to
  the product domain. For corvus: diff, graph, gitflow, branches, mr, issues,
  ticket_links, recovery, status, missing_projects, commit, cache, terminals,
  ide, pipelines, studio, git-cli, recent_repos, workspaces, repos, session,
  graph_columns, linked_worktrees, pipeline_runs.
- **Per-profile plugins** (`profiles/<p>/plugins/`): installed set + per-plugin
  settings + marketplace cache + themes + toolchains.

## Path resolution mechanism

Path helpers are pure functions today (`arbor_config_path(sub)`). Profiles add a
runtime dimension — the active profile — without threading state through every
caller: `arbor-core` holds a process-global **active-profile cell**
(`LazyLock<RwLock<String>>`), seeded at boot from the `active-profile` file (or a
launch argument), read by the new profile-aware helpers, and updated on switch.

New helpers (additive — existing `arbor_config_*` keep meaning "global root"):

- `active_profile() -> String`, `set_active_profile(name)`,
  `init_active_profile()` (seed from disk at boot).
- `profiles_root()`, `arbor_profile_dir()`, `arbor_profile_path(sub)` (active).
- `product_dir(product)`, `product_path(product, sub)` (active).
- `*_for(name, …)` explicit-profile variants — used by migration and by profile
  management operating on a non-active profile.

## Migration

One-shot, on first boot after the upgrade: if `arbor/config.toml` exists and
`arbor/profiles/` does not, create `profiles/default/` and move the flat files
into their buckets (config.toml → split generic/corvus, workspaces/repos/session
→ corvus, plugins/marketplace/themes/toolchains → plugins/). The old `-dev`
suffixed files fold into a `dev` profile. Non-destructive where feasible (copy
then verify), idempotent (guarded by the `profiles/` existence check).

## Phases

1. **Foundation** ✅ — `arbor-core` active-profile cell + profile-aware path
   helpers + validation + tests. Additive, breaks nothing.
2. **Config model split** ✅ — `AppConfig` stays one flat in-memory aggregate
   (zero call-site / command / FE churn); only `load()`/`save()` split it across
   `profile.toml` (generic) + `corvus/config.toml` (git) + global `oauth.toml`,
   partitioned by top-level key via one `GENERIC_KEYS`/`GLOBAL_KEYS` source of
   truth. Per-field `#[serde(default)]` lets each file carry only its own
   sections; a one-shot migration folds a legacy flat `config.toml` into the
   split layout on first boot (legacy left in place as backup). `init_active_-`
   `profile()` seeds the profile cell at `AppState::new()`.
3. **Per-profile storage** — repoint satellite files onto profile/product dirs.
   - **3a** ✅ — corvus satellites (`workspaces.json`, `repos.json`,
     `session.json`, `workspace-state/`, `graph_columns.toml`,
     `linked_worktrees.toml`, `pipeline_runs/`) → `product_path(PRODUCT_CORVUS,
     …)`, with a one-shot idempotent **move** migration
     (`config::profile_migration`) so upgraded installs keep their data. Product
     names centralized in `arbor-core` (`PRODUCT_CORVUS`/`PRODUCT_MERULA`);
     `try_product_path` added.
   - **3b** ✅ — plugin area → `profile_plugins_dir()`: installed plugins
     (`installed/`), `plugin_states.json`, `plugin_data/`, `toolchains/`,
     `themes/`, and the marketplace files. The `-dev` filename hack is gone:
     debug builds now run on the `dev` profile (build default in `arbor-core` +
     a build-specific `active-profile-dev` pointer), so dev/release isolation is
     a profile, not a suffix. `plugin_data_dir()` centralized in
     `arbor-plugin-core` (was duplicated in settings-store + lifecycle). The
     flat plugin files migrate into `profiles/default/plugins/` (gated to
     `default`; old `-dev` twins are left for a fresh `dev` profile).
4. **Migration** — flat `arbor/*` → `profiles/default/…` one-shot. Config.toml
   done in Phase 2, corvus satellites done in 3a; plugin files land with 3b.
5. **Profile management (BE)** ✅ — `crate::profile` (FS CRUD over
   `profiles/<name>/` + the `active-profile` pointer) + keep-shell commands
   `list_profiles` / `get_active_profile` / `create_profile` / `rename_profile`
   / `delete_profile` / `switch_profile`, with `src/lib/ipc/profiles.ts`
   bindings. **Switch is live (no relaunch)**: `switch_profile` writes the
   pointer, flips the in-process cell, then `AppState::reload_for_active_profile`
   re-resolves the persistent per-profile caches (config, workspaces, repos,
   linked-worktrees, marketplace) + clears the per-tab/session caches, and
   `reload_runtime` reloads the plugin host. It then emits
   `arbor://profile-switched`; every window reloads its FE stores
   (`window.location.reload()`). Guards: can't rename/lose `default`, can't
   delete the active or the last profile. The legacy-config + satellite
   migrations are gated to `default` so a fresh non-default profile starts from
   built-in defaults.
6. **Profile UI (FE)** ✅ — `profileStore` + the title-bar **gear menu → Profile**
   submenu (quick-switch list with the active one checked, *New profile…*,
   *Manage profiles…*) + `ProfileManagerModal` (create/rename/delete/switch).
   DocsPanel (`Settings.svelte`) + CHANGELOG updated.

## Status note

Switching is live (no process restart): the backend swaps its in-memory caches,
then each window reloads its webview to re-derive its stores. Remaining
follow-ups: the explorer window isn't reloaded on switch (it doesn't init
`profileStore`); no Command Palette entry for switching yet; merula still lives in
its sibling namespace rather than `profiles/<p>/merula`.
