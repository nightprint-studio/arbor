# Arbor — Documentation

> Auto-generated from the in-app documentation panel.

## Table of Contents

- [Getting Started](#getting-started)
- [Initialize Repository](#initialize-repository)
- [Clone Repository](#clone-repository)
- [Workspaces](#workspaces)
- [Linked Worktrees](#linked-worktrees)
- [Repository Browser](#repository-browser)
- [Git Graph](#git-graph)
- [Stage & Commit](#stage-commit)
- [Merge Conflicts](#merge-conflicts)
- [Branches](#branches)
- [Tags & Stash](#tags-stash)
- [Submodules](#submodules)
- [Git Flow](#git-flow)
- [Ticket Links](#ticket-links)
- [Git Notes](#git-notes)
- [Worktrees](#worktrees)
- [File Tree](#file-tree)
- [Reflog](#reflog)
- [Recovery Journal](#recovery-journal)
- [Missing Projects](#missing-projects)
- [Git Executable](#git-executable)
- [Git Bisect](#git-bisect)
- [Marketplace](#marketplace)
- [Terminal](#terminal)
- [Command Palette](#command-palette)
- [Keyboard Shortcuts](#keyboard-shortcuts)
- [Statistics](#statistics)
- [Background Jobs](#background-jobs)
- [Notifications](#notifications)
- [Pipelines](#pipelines)
- [Source Export plugin](#source-export-plugin)
- [CI / CD](#ci-cd)
- [Pull / Merge Requests](#pull-merge-requests)
- [Issues (Linear / Jira)](#issues-linear-jira)
- [Security Dashboard](#security-dashboard)
- [Deep Links](#deep-links)
- [Interface & Git](#interface-git)
- [Performance](#performance)
- [Access](#access)
- [Project](#project)
- [Themes & Presets](#themes-presets)
- [Basics & Manifest](#basics-manifest)
- [Hooks & Constants](#hooks-constants)
- [API — Core](#api-core)
- [API — UI](#api-ui)
- [API — Jobs](#api-jobs)
- [API — Toolchains](#api-toolchains)
- [Plugins](#plugins)
  - [bevy-brp](#plugin-bevy-brp)
  - [ron-studio](#plugin-ron-studio)
  - [compile-action](#plugin-compile-action)
  - [cloud-storage](#plugin-cloud-storage)
  - [json-studio](#plugin-json-studio)
  - [source-export](#plugin-source-export)
  - [cipher-studio](#plugin-cipher-studio)
  - [chunk-merger-bin](#plugin-chunk-merger-bin)
  - [number-studio](#plugin-number-studio)
  - [deps-explorer](#plugin-deps-explorer)
  - [run-action](#plugin-run-action)
  - [run-monitor](#plugin-run-monitor)

---

Welcome

# Getting Started

Arbor is a Git GUI built on Tauri, Rust, and Svelte 5 — no Electron, no Node.js runtime overhead. Fast, keyboard-driven, extensible via Lua plugins.

- **Tauri 2** — Native shell
- **Rust** — libgit2 backend
- **Svelte 5** — Runes UI
- **Lua 5.4** — Plugin runtime

> **Quick Start** Press `Ctrl+O`, select any folder containing a `.git` directory, and the commit graph loads instantly.

## Opening a repository

1. Press `Ctrl+O` or click the **+** button in the tab bar
2. Select the folder containing your `.git` directory
3. The commit graph, branches, tags, and stashes all load automatically

If the selected folder has no `.git` directory, Arbor will offer to **initialize a new repository** — see the *Initialize Repository* section.

## Multiple tabs

Open multiple repositories simultaneously in separate tabs.

| Action | Shortcut |
| --- | --- |
| Next tab | Ctrl+Tab |
| Previous tab | Ctrl+Shift+Tab |
| Close tab | Ctrl+W |
| Recent repos quick-switch | Ctrl+R |

Right-click any tab for more options: reveal in explorer, copy path, rename, close others.

## Interface overview

- **Activity Bar** — Narrow icon rail on the left. Top icons toggle sidebar panels; bottom icons toggle the detail and stage panels. Plugin actions can appear here too.
- **Sidebar Ctrl+B** — Local/remote branches, tags, and stashes. Double-click a branch to check it out. Ahead/behind counts shown inline.
- **Commit Graph** — SVG lane graph with virtual scrolling — handles repositories of any size without performance degradation. Search with Ctrl+F.
- **Detail Panel** — Commit metadata, changed file list, and syntax-highlighted diff viewer. Dockable at the bottom or the right side (see Settings).
- **Stage Area Ctrl+⇧S** — Stage/unstage files, write commit messages, manage stashes. Supports hunk-level and line-level partial staging.
- **Status Bar** — Current branch, ahead/behind indicators, repo path, and quick-access buttons for fetch, notifications, docs, and settings.

## Command Palette

Press `Ctrl+K` to open the Command Palette — a unified search overlay for actions, branches, commits, and plugin commands. Everything is reachable without touching the mouse.

---

# Initialize Repository

When you open a folder that isn't a git repository, Arbor detects this automatically and offers to initialize one â€” no need to run `git init` in a terminal.

## The flow

1. Either open the hamburger menu and pick **Initialize Repositoryâ€¦**, or press `Ctrl+O` and select any folder without a `.git` directory
2. The **Initialize Repository** dialog opens automatically
3. Configure options across three tabs: **Project**, **Files**, **Remote**
4. Click **Initialize Repository** or press `Ctrl+Enter`
5. The repo is created and opens as a new tab immediately

The menu entry routes you straight into the dialog regardless of whether the folder already has a `.git` directory: a folder that's already a repo just opens normally, no destructive re-init.

## Project tab

- **Description** â€” stored in `.git/description` and added to the README if enabled
- **Default branch** â€” choose `main`, `master`, `develop`, or a custom name
- **Initial commit** â€” stages all created files and makes the first commit automatically
- **Author name / email** â€” pre-filled from the global git config (`user.name` / `user.email`)

## Files tab

- **README.md** — Generated with the repo name as H1 and the description as body text.
- **.gitignore** — Pick from built-in templates: Rust, Node/JS/TS, Python, Go, Java, C, C++, .NET/C#, Swift, Ruby, PHP, Unity.
- **LICENSE** — MIT, Apache 2.0, GPL 3, LGPL 3, AGPL 3, BSD 2/3-Clause, ISC, MPL 2.0 â€” filled with your name and the current year.

## Remote tab

Optionally create and link a remote repository at init time:

| Option | What happens |
| --- | --- |
| None | Local repo only â€” add a remote later |
| GitHub | Creates the repo via the GitHub API and adds it as origin. Requires a GitHub token in Settings â†’ Credentials. |
| GitLab | Creates the project via the GitLab API. Supports organizations/groups. Requires a GitLab token. |
| Custom URL | Adds any URL as origin without an API call (Gitea, Forgejo, self-hosted instances). |

> **API failure is non-fatal** If the provider API call fails, the local repository is still initialized. Arbor shows an error toast but the repo opens normally.

## Plugin hook: on_repo_init

Fires after a repository is successfully initialized and opened. Declare in `plugin.toml`:

```toml
[hooks]
on_repo_init = true
```

Register a handler in Lua:

```lua
arbor.events.on("on_repo_init", function(ctx)
  -- ctx.path           -- absolute path to the repo
  -- ctx.name           -- folder name
  -- ctx.default_branch -- e.g. "main"
  -- ctx.provider       -- "none" | "github" | "gitlab" | "custom"
  -- ctx.remote_url     -- "" or the configured remote origin URL
  -- ctx.has_readme     -- bool
  -- ctx.license        -- "" or SPDX id (e.g. "mit")
  -- ctx.gitignore      -- "" or template name (e.g. "rust")
  arbor.notify{ title = "Repository initialized", message = ctx.name .. " created on " .. ctx.default_branch, level = "success" }
end)
```

---

# Clone Repository

Clone any remote repository directly from the UI — no terminal required. Arbor handles authentication, progress tracking, and opens the repo automatically when done.

## Opening the clone dialog

- Click **Clone** on the welcome screen (shown when no repo is open)
- Click the **+** tab button → **Clone Repository**
- Press `Ctrl+Shift+O`

## Clone options

- **Repository URL** — Any valid Git URL: HTTPS (https://github.com/…), SSH (git@github.com:…), or a local path.
- **Destination folder** — The parent directory for the clone. Arbor appends the repo name automatically — cloning my-repo into ~/projects creates ~/projects/my-repo.
- **Branch** — Optionally specify a branch to check out after cloning. Leave empty to use the remote's default branch.

## Authentication

Arbor uses credentials from the OS keyring — the same store used by Settings → Git & Integrations:

- **HTTPS** — username/password or personal access token stored against the hostname
- **SSH** — uses the SSH agent or the key from `~/.ssh/config`

> **Missing credentials?** If the clone fails with an authentication error, add your credentials in *Settings → Git & Integrations → Credentials* and retry.

## Progress & completion

A progress bar tracks the clone operation (objects received, deltas resolved). The dialog stays open until the clone finishes or fails. On success, the repository opens automatically as a new tab.

## Common errors

| Error | Likely cause |
| --- | --- |
| Authentication failed | Wrong or missing credentials. Add/update in Settings → Credentials. |
| Repository not found | The URL is wrong, or the repo is private and your token doesn't have access. |
| Destination already exists | The target folder is non-empty. Choose a different path or remove the existing folder first. |

---

# Workspaces

A **workspace** is a named, colour-coded group of repositories.
  Switching workspace replaces the entire tab set with whatever was open the last
  time you were there, so you can jump between unrelated projects without
  losing context.

> **Workspace dropdown** The dropdown in the top bar (next to the hamburger menu) shows the active workspace and lets you switch between them. Every installation has a built-in **Scratch** workspace that collects ad-hoc opens.

## Key concepts

- **Central registry** — Every repository Arbor has ever seen lives in ~/.config/arbor/repos.json with a stable UUID. Workspaces reference that UUID, so renaming or relocating a repo is a one-place edit.
- **Membership is many-to-many** — A repo can belong to several workspaces at once — membership is just a reference. Removing it from one workspace never deletes the repo or its path on disk.
- **Tab snapshots** — Each workspace has its own tab snapshot (workspace-state/<uuid>.json). Switching saves the current snapshot and restores the target one. Panel sizes remain global.
- **Scratch** — Non-deletable fall-back. Every ad-hoc opened repo lands here until you move it to a named workspace.

## Creating and managing workspaces

1. Click the workspace dropdown → **Manage Workspaces…** (or use your keybinding).
2. Hit **+ New Workspace**. Give it a name, a palette colour, an optional group, and tick the repos that should belong to it.
3. Use the management modal to rename, reorder, move repos between workspaces, or delete workspaces you don't need any more. Deleting a workspace never removes the repos themselves.

## Groups (optional)

Groups are a purely visual organisation aid — they let you nest several
  workspaces under a single collapsible header (handy for client/project/team
  separation). Creating a group from the management modal adds a header that
  you can drag workspaces into. Deleting a group orphans its children back to
  the top level; it never cascades through to the workspaces themselves.

## Cross-workspace tabs

Opening a repo that belongs to a different workspace opens it as a **cross-workspace tab**: a small coloured dot (with the source
  workspace's initials) appears on the tab, and the tab is marked with a
  dashed left border. Cross-WS tabs live only in the current workspace's
  snapshot — they are not added to its membership. Right-click a cross-WS
  tab to **Add to active workspace** and flip it into a regular
  member.

## Import / export

**Export** a workspace from the management modal: Arbor
  copies a portable JSON blob to the clipboard containing the workspace
  name, colour, and each member's *display name and remote URL* (paths are intentionally omitted so the file works across machines).

**Import** takes that JSON and shows a preview table. For
  each repo the row proposes an action:

- **Use existing** — if Arbor already has a matching repo (matched by remote URL) locally.
- **Locate…** — pick a folder on disk where the repo already lives.
- **Clone…** — type a destination path; Arbor shells out to `git clone`.
- **Skip** — leave this one out of the imported workspace.

The **Create Workspace** button stays disabled until every row is resolved.

## Bulk operations

Each workspace header carries a small toolbar of bulk actions. They all
  share the same engine: a single aggregated background job that walks the
  workspace's repos sequentially, logging per-repo progress to the Job Output
  panel. Individual repo rows show a spinner while queued and flip back to a
  branch / ahead-behind chip when their step completes.

| Action | What it does |
| --- | --- |
| Fetch all | Runs git fetch on every member's preferred remote (origin first, then the first one configured). Never modifies the workdir. |
| Pull all | Fetch + fast-forward / merge per member. Skips repos in detached HEAD; surfaces conflicts with a distinct row badge so you know which projects need attention. |
| Tag all | Opens a modal to create the same tag (lightweight or annotated) at the current HEAD of every member, with optional push. See below. |

## Tag all (release)

The tag icon on the workspace header opens a pre-flight modal that scans
  every member and surfaces any conditions you'd want to know about *before* stamping a release tag across the whole group:

- **Detached HEAD** — the repo is not on a branch; it will be skipped (a tag at a parked commit is almost always a mistake).
- **Behind upstream** — the local branch hasn't pulled yet; the tag would land on a stale commit.
- **Local modifications** — uncommitted changes in the workdir; the tag would point at the last commit, ignoring your work-in-progress.
- **Merge in progress** — an unfinished merge / rebase / cherry-pick / revert; resolve before tagging.
- **Path missing** — the repo on disk has been moved or deleted; it is skipped.

Type a tag name and an optional message — when the message is non-empty the
  tag becomes annotated, otherwise it's lightweight. The footer carries a **split button** with two modes:

- **Create tag** — creates the tag locally on every member.
- **Create tag & push** — creates the tag, then pushes `refs/tags/<name>` to each member's preferred remote.

## Worktrees and workspace membership

A workspace lists **root repositories**, not the individual
  worktree paths underneath them. The picker in the create / edit modal
  intentionally hides linked worktrees: adding both the root and a worktree
  of the same project would be the same project listed twice.

- **Add the root once** — pick the main checkout from the create / edit modal.
- **Switch to a worktree from inside the tab** — open the worktrees sidebar and click *Switch* on a sibling. The active tab swaps its working path; the workspace membership stays put.
- **Indicators** — a member that's currently sitting on a linked worktree shows a small worktree icon next to its branch label, and the workspace header gets a tiny worktree badge so the information stays visible while collapsed.
- **Legacy members** — workspaces created before this change may already include a worktree path as a member. The edit modal still shows it (with a `worktree` tag and a softer style) so you can deselect it; new pickers won't offer worktrees again.

## Startup behaviour

- Arbor auto-restores the last active workspace, including its open tabs and active tab.
- When launching for the first time after upgrading, any repos from the legacy session are migrated into Scratch and the welcome screen offers to organise them.
- Auto-switch to Scratch happens if the active workspace is deleted while it is active.

## Command Palette

| Shortcut | Verb | Description |
| --- | --- | --- |
| Ctrl+N | Open Project | Fuzzy-open any repo from the active workspace (plus Scratch). |
| Ctrl+Shift+N | Open from Workspace | Fuzzy-open a repo from any other workspace as a cross-workspace tab. |
| Ctrl+K | Switch Workspace | Type switch workspace and pick a target. Saves the current snapshot, restores the target's. |

> **Not to be confused with…** Git *worktrees* are an unrelated feature that share history but check out different branches — see the *Worktrees* documentation. Arbor workspaces are a UI-level grouping with no git-level counterpart.

---

# Linked Worktrees

> **Experimental feature** Linked Worktrees is still being shaped — file format (`linked_worktrees.toml`), Tauri commands, plugin hooks and the edge-case behaviour around stash conflicts may change between releases. Avoid building plugins or muscle memory around the wire-format details for now; the user-facing UI (manager modal, badges, command palette verbs) is the stable surface.

A **worktree link** ties several worktrees together so that a
  branch checkout on one member is propagated to the others.  Links are
  optional and orthogonal to workspaces: a workspace groups repos for
  navigation, a link coordinates their HEADs.

> **Where it lives** Persisted in `~/.config/arbor/linked_worktrees.toml`. Members are identified by their `RepoRegistry` UUID (the same identity used by workspaces, keyed by path), so each worktree path is its own member — multiple worktrees of the same repo are independent links.

## How sync works

1. You check out a branch on a tab whose worktree is a member of a sync-enabled link.
2. The local checkout completes first.  If it fails, no propagation runs.
3. Arbor iterates the other members in a background thread, serialised: stash dirty workdir → checkout the resolved branch → stash apply.
4. Members where the branch is missing are *skipped silently*.  An aggregated notification at the end summarises updated / skipped / conflicting members.

> **Stash safety** Pre-checkout stashes use `git stash push -u` so untracked files are preserved. Re-application uses `git stash apply`, not `pop`: a clean apply drops the entry, but on conflict the entry is kept so nothing is silently lost.

## Branch aliases

When the same logical branch has different names per repo
  (`feature/X` on repo A vs `feature/Y` on repo B),
  declare an **alias group** in the link.  An alias group is an
  equivalence class — a set of `(repo_id, branch)` pairs that all
  resolve to "the same branch" during sync.

- **Resolution rule** — when checking out, Arbor looks for an alias group containing the initiator's `(repo_id, branch)`.  If found, every other member uses the branch declared in that group.  Members not present in the group fall back to identity-name.
- **Alias wins over identity** — if the alias group says repo B should use `feature/Y` but `feature/X` also exists on B, the checkout still goes to `feature/Y`.
- **Smart cleanup** — deleting a branch removes any alias entries pointing to it; renaming a branch updates the entries; if all entries in a group end up sharing the same name, the group is dropped automatically.
- **Create-branch guard** — creating a branch whose name is reserved by an alias on this repo is refused with a message pointing at the offending link.

## Managing links

1. Open the manager from **Ctrl+K → Manage Linked Worktrees** or click the link badge in the graph toolbar.
2. Create a new link; add members from the registry.  A worktree can belong to at most one link.
3. Toggle **Sync enabled** per link to pause propagation temporarily — checkouts won't propagate while disabled, and re-enabling does *not* auto-resync (the next checkout will).
4. Add or edit alias groups; pick at least 2 members per group.

## Out-of-sync indicator

Once a link has performed at least one sync, the graph toolbar shows a **link badge** whenever the active tab belongs to that link.
  An amber dot lights up when the tab's current branch differs from the
  expected one (i.e., it's out of sync with the link's last target).
  Clicking the badge opens the manager pre-selected on that link.

## Edge cases

| Situation | Behaviour |
| --- | --- |
| Branch missing on a member | Skipped; counted in the aggregated notification. |
| Member is in detached HEAD | Stashed + checked out like any other member. |
| Member's path is missing | Skipped with a "broken member" reason; visible in the manager. |
| Member already on the target branch | Skipped silently — no work to do. |
| Member is not currently open as a tab | Repo is opened in the background just for the checkout, no UI tab is added. |
| Concurrent sync on the same link | Serialised by an in-progress guard; recursive triggers are suppressed. |
| Tab swaps to a different worktree (e.g. via the Worktrees sidebar) | The badge follows the new path.  If the new worktree is not in any link, the badge disappears. |
| Checkout from the integrated terminal | Not intercepted — only checkouts via the Arbor UI / Lua API propagate. |

## Plugin API

Plugins can introspect links and toggle sync via the `arbor.linked_worktrees` table.  No create/delete operations
  are exposed — those are user-only.

| Function | Returns | Notes |
| --- | --- | --- |
| arbor.linked_worktrees.list() | array of {id, name, sync_enabled, member_count} | Sorted by name. |
| arbor.linked_worktrees.get(id) | full WorktreeLink table or nil | Includes members + alias groups. |
| arbor.linked_worktrees.set_sync_enabled(id, enabled) | bool (true on success) | Persisted immediately. |

## Hooks

| Hook | Context | Fires when |
| --- | --- | --- |
| "on_worktree_link_sync_started" | {link_id, link_name, initiator_repo_id, target_branch} | Just before propagation begins. |
| "on_worktree_link_sync_done" | {link_id, link_name, target_branch, results: [...]} | After every member has been processed.  Each result has repo_id and a status table tagged by kind. |
| "on_worktree_link_member_added" | {link_id, repo_id} | User added a worktree to a link. |
| "on_worktree_link_member_removed" | {link_id, repo_id} | User removed a worktree from a link. |

## Command Palette

| Verb | Description |
| --- | --- |
| Manage Linked Worktrees | Opens the manager modal. |
| Link this Worktree… | Opens a picker — pick an existing link or create a new one with this worktree as the first member. |
| Unlink from "<name>" | Removes the current worktree from its link (after confirmation). |
| Enable/Disable Sync for "<name>" | Toggles sync on the active tab's link. |

---

# Repository Browser

Browse, preview, and clone repositories hosted on GitHub or GitLab — without leaving Arbor. The Repository Browser gives you a full file-tree explorer with syntax-highlighted previews and one-click cloning.

## Opening the Repository Browser

- Click the **Repository Browser** button in the hamburger menu (☰ → Repository Browser)
- Press `Ctrl+Shift+R`

> **Requires connected accounts.** Go to *Settings → Git & Integrations* to add a GitHub or GitLab token before using the browser.

## Layout

- **Left panel — Repository list** — Shows all repositories for the selected account, grouped by namespace (organisation / group). Supports live search. A green dot marks repos already open as a local tab.
- **Right panel — File tree & preview** — Repo metadata (description, language, stars, size, last update) in the header. Below it: a breadcrumb navigator + file-tree. Click a file to open a syntax-highlighted preview.

## Switching accounts

The account selector at the top of the left panel lets you switch between connected GitHub and GitLab accounts. Each account shows its avatar and username. The dropdown lists all configured providers — click one to reload the repository list for that account.

## Browsing files

- Click a **directory** to navigate into it. The breadcrumb updates automatically.
- Click **← ..** (back button) or any breadcrumb segment to go up.
- Click a **file** to open it in the preview pane below the tree.
- The preview shows syntax-highlighted code, images (inline), or a download prompt for binary files.

## File preview actions

| Action | Description |
| --- | --- |
| Copy | Copies the raw file content to the clipboard (no line numbers). |
| Download | Saves the file to a folder you choose via the native file picker. |
| Close preview | Dismisses the preview pane; the file tree expands back to full height. |

## Cloning a repository

Select a repository from the list, then click the **Clone** button in the repo bar. A folder picker lets you choose the parent directory — Arbor appends the repo name automatically. Once cloning completes, the repo opens as a new tab.

If the repository is already open locally, the **Clone** button is replaced by **Open Tab**, which switches directly to the existing tab.

## Opening in browser

The external-link icon (**↗**) in the repo bar opens the repository's web page (GitHub / GitLab) in your default browser.

## Sidebar toggle

The panel-toggle button in the header collapses the left repository list, giving the file tree and preview more horizontal space. Click it again to restore the list.

## Repo list cache

"List all repos" can take 30s+ on accounts with hundreds of projects. Arbor caches the result in `localStorage` per provider so reopening the modal is instant.

- The strip below the search box shows when the list was last fetched (*Cached · 4m ago* / *Updated · just now*) and exposes a **Refresh** button that bypasses the cache.
- If the cache is past its TTL but still present, Arbor shows the stale list immediately and refetches in the background — the strip updates to *Updated* once the fresh list arrives.
- Tune the TTL (default 10 minutes) or wipe the cache from *Settings → Cache → Repository Browser*.
- Set the TTL to `0` to disable caching entirely (every open re-fetches).

> **Backend speed-ups.** Pages 2..N of the GitHub/GitLab repo list are now fetched concurrently (capped by the API's own rate limit). GitLab's slow `statistics=true` flag was dropped — the list view doesn't display repo size, so paying for it on every open isn't worth it.

## Language colours

Each repository row shows a coloured dot next to the last-update time, indicating the repo's primary language. The palette mirrors GitHub's Linguist colours so the dots match what you see on github.com. Click **Legend** at the bottom of the repo list to open the same legend inline.

Supported languages: C, C#, C++, Clojure, Crystal, CSS, Dart, Dockerfile, Elixir, Erlang, F#, Go, Groovy, Haskell, HTML, Java, JavaScript, Julia, Kotlin, Lua, Makefile, Nim, Nix, Objective-C, OCaml, Perl, PHP, PowerShell, Python, R, Ruby, Rust, Scala, SCSS, Shell, Solidity, Svelte, Swift, TeX, TypeScript, Vim Script, Vue, Zig.

> Languages not in the palette fall back to a neutral grey dot. The dot is hidden entirely if the provider didn't return a primary language for the repo.

## Supported providers

| Provider | Requirements |
| --- | --- |
| GitHub | Personal access token with repo scope (Settings → Git & Integrations → GitHub) |
| GitLab | Personal access token with read_api + read_repository scopes; supports self-hosted instances |

---

# Git Graph

The commit graph renders your entire repository history as SVG lanes with virtual scrolling — only visible rows are painted, regardless of repository size or branch count.

## Navigation

| Action | How |
| --- | --- |
| Select commit & load diff | Click any row |
| Context menu | Right-click any row |
| Load more history | Scroll to the bottom — loads automatically (when pagination is on) |
| Search commits | Ctrl+F — message, author name, or SHA |
| Next search match | Enter while search is open (or the ▼ button) |
| Previous search match | Shift+Enter while search is open (or the ▲ button) |
| Jump to HEAD | Ctrl+Home or the ↑ button in the toolbar |

## Commit node indicators

- **Avatar circle** — each regular commit shows the author's avatar (Gravatar or generated initials) clipped to a circle, with a colored lane border ring
- **Small filled dot** — merge commit with two or more parents; rendered smaller than avatar nodes (~65% radius) to mark topology without visual clutter
- **Outer glow ring** — the current HEAD commit (checked-out)
- **Dimmed avatar** — commit already pushed to the remote tracking ref
- **Dashed border** — WIP node representing working directory changes
- **Amber dot (tab bar)** — the repository has uncommitted changes

## Author avatars

For each visible commit, Arbor resolves the author's avatar using their commit email:

1. The email is hashed with **SHA-256** (via Web Crypto API — no external lib needed)
2. A **Gravatar** lookup is attempted: `gravatar.com/avatar/<sha256>`
3. If no Gravatar exists, a deterministic **colored circle with initials** is generated client-side

> **GitHub & GitLab** — both platforms associate commit emails with Gravatar accounts by default, so avatars resolve automatically for most contributors. Users who have set a custom avatar only on GitHub/GitLab (not on Gravatar) will fall back to the generated initials avatar.

Avatars are cached in memory for the session — each email is fetched at most once.

## Branch labels

Labels appear inline on each commit row:

feature/login local branch ·  origin/main remote tracking ·  v2.1.0 tag ·  HEAD checked-out commit

## Graph rendering

The lane layout is computed in Rust (`src-tauri/src/git/graph.rs`) using a gitk-style topological lane assignment. The frontend renders the result as an SVG with:

- **Virtual scrolling** — only the rows in the viewport (± 5 rows buffer) are rendered; `ROW_HEIGHT = 28px`
- **Lane width** — `LANE_WIDTH = 26px` per lane; `NODE_RADIUS = 10px` (20px avatar diameter)
- **Edges** — right-angle elbows with rounded corners; dashed lines for squash-merge ghost edges
- **SVG `<clipPath>`** — one per visible non-merge node, keyed by commit OID, clips the avatar `<image>` to a circle
- **Pushed indicator** — commits at or below the remote tracking ref row are dimmed (opacity 0.5) to distinguish pushed from unpushed

## Context menu actions

- **Branch & Tag** — Create Branch — branch from any commit Create Tag — lightweight or annotated Checkout — detached HEAD at that commit
- **History Rewrite** — Cherry-pick — apply commit to current branch Revert — create a revert commit Reset → here — soft / mixed / hard reset
- **Remote** — Push — push current branch (shown on HEAD only) Pull — fetch + fast-forward current branch
- **Clipboard** — Copy SHA — full commit hash to clipboard Copy message — commit summary text

> **Cherry-picking or reverting a merge commit** Merge commits have two parents, so both cherry-pick and revert are ambiguous on them: Git needs to know which side of the merge to keep. Arbor defaults to **parent 1** (the receiving branch) — equivalent to `git revert -m 1` / `git cherry-pick -m 1` — which targets the changes that were merged in while keeping the branch you merged onto as the baseline. This is what you want in almost every case; if you ever need the opposite, use the CLI with `-m 2`. *Reset* is unaffected — it just moves `HEAD` to the target commit and never computes a diff, so the number of parents doesn't matter.

## WIP node context menu

The **WIP node** (dashed circle at the top of the graph) represents uncommitted working directory changes. **Right-click** it to access quick actions:

- **Open Stage Area** — Loads the working directory diff and opens the Stage Area panel — the same as clicking the WIP node.
- **Stash Changes** — Saves all working directory changes (including untracked files) to the stash stack and restores a clean working tree.
- **Stash (exclude untracked)** — Same as above but leaves untracked files in place — only tracked modifications and staged changes are stashed.

Stashes appear in the sidebar under **Stashes** and can be applied, popped, or dropped at any time.

## Pagination

By default the graph loads history in batches of 500 commits (configurable in *Settings → Graph → Commits per load*).
  When you scroll near the bottom, the next batch is fetched automatically.

You can disable this behaviour entirely in *Settings → Graph → Lazy-load pagination*.
  When pagination is off, the **entire** repository history is fetched in a single request on startup.
  This is convenient for small repos but can be slow for large ones.
  The setting is persisted to `~/.config/arbor/config.toml`.

## File History Filter

Filter the graph to show only commits that touched a specific file:

1. In the diff file list, hover any file to reveal a **file-search icon** on the right
2. Click it — the graph reloads with only the relevant commits visible
3. A pill in the toolbar shows the active file name. Click **×** to clear the filter

> **Under the hood** The filter runs in Rust via `DiffOptions::pathspec` — renames, copies, and binary files are all included. Pagination (load-more) also respects the active filter.

## Commit Templates

The commit message field is auto-filled from the first available source, in priority order:

1. **Git native** — the file pointed to by `commit.template` in your repo's `.gitconfig`
2. **Global Arbor template** — set in *Settings → Repository → Commit Template*, applies to all repos

A template icon appears in the top-right corner of the message field whenever the current text differs from the template — click it to restore without losing your changes.

## Export Graph as SVG

The entire commit history can be exported as a standalone, fully-scalable SVG file — useful for documentation, pull-request overviews, or archiving a project's branching strategy.

### How to trigger

- **Toolbar** — click the ↓ *Download* button at the top-right of the graph (visible when a graph is loaded)
- **Context menu** — right-click any empty area of the graph background and choose *Export graph as SVG…*

A file-picker dialog opens so you can choose the output path and filename (default: `graph.svg`).

### What is exported

- The **full history** (up to 999 999 commits) — not just the currently visible page
- **Lane graph** — same geometry as the on-screen render: `ROW_HEIGHT=28px`, `LANE_WIDTH=26px`, `NODE_RADIUS=10px`, bezier elbows
- **Colored lanes** — the same 10-colour palette as the live graph
- **Node styles** — merge commits get an outer ring; HEAD commit gets a white border ring
- **Ref badges** — branch labels (local/remote) and tags appear inline next to each commit in colour-coded pill shapes
- **Text columns** — short SHA · ref badges · author name · commit summary (truncated at 72 chars)

### Background job

The export runs as a **background job** so the UI stays responsive even for large repositories.
  Progress is visible in the *Jobs* overlay (click the status-bar spinner or the badge count).
  A **bell notification** appears when the export completes or fails.

> The SVG is written as a streaming `BufWriter` directly to disk — the full file is never held in memory — so exports of repositories with tens of thousands of commits stay within normal memory usage.

## Branch Cleanup

The **trash icon** in the sidebar's *Local Branches* header opens the Branch Cleanup modal. It scans for all branches already merged into a target branch and lets you delete them in bulk — locally or on the remote. See the **Branches** section for full details.

---

# Stage & Commit

The stage area is your workspace for crafting commits. Stage individual files, specific hunks, or even single lines — then write your message and commit.

> **Open Stage Area** Press `Ctrl+Shift+S` or click the commit icon at the bottom of the Activity Bar.

## Basic workflow

1. Edit files in your editor — they appear in the **Unstaged** list automatically
2. Click a file to preview its diff in the detail panel
3. Click **+** next to a file (or **Stage All**) to queue it for commit
4. Write your message in the text area — the first line becomes the commit summary
5. Press `Ctrl+Enter` to commit

## File list sections

- **Unstaged** — Modified, new, or deleted files not yet queued for commit. Click to preview. Right-click for quick actions: Stage, Discard, Open in editor.
- **Staged** — Files queued for the next commit. Click to preview the staged diff. Right-click to unstage or discard changes.

File status icons: **M** modified · **A** added · **D** deleted · **R** renamed · **U** untracked

## Partial staging — hunk & line level

You don't have to stage whole files. In the diff viewer:

- **Stage hunk** — click the **+** button in a hunk header to stage that block of changes only
- **Stage individual lines** — click line checkboxes to select specific additions or deletions, then click **Stage Lines**
- **Unstage lines** — the same mechanism works in reverse on the staged diff

> **How it works** Arbor builds a custom unified diff patch from your selection and applies it to the index — the same technique as `git add -p`, but visual.

## Diff navigation

Every diff view (Stage, commit detail, branch compare, conflict modal) carries the same chunk-aware controls in its header:

- **Chunk navigation** — the `↑` / `↓` chevrons in the header (with a *n/N* counter) jump between change blocks. `F3` / `Shift+F3` do the same from the keyboard.
- **Auto-focus on open** — opening a file (or staging a line) lands you on the first remaining change instead of the top of the file.
- **Show full file** — the *file* icon next to the Unified/Split toggle expands the diff to the entire file with change highlights instead of just the N-line context. Useful for navigating a change in the surrounding code; toggleable per session, persisted in *config.toml*.
- **Virtualization** — large diffs (default: more than 200 lines) automatically switch to a windowed renderer that only paints visible rows, so scrolling stays smooth even on giant files. Threshold is configurable in *Settings → Diff & Stage*.

## Commit form

The commit requires at least one staged file and a non-empty message. The first line becomes the summary; anything after a blank line becomes the extended body — standard git convention.

Press `Ctrl+Enter` in the message field to commit immediately.

## Discarding changes

Discarding restores a file (or all files) to match the current index. **This is irreversible** — any working-directory edits are permanently lost.

- **Discard file** — click the `↩` button next to a file, or right-click → *Discard Changes*. If the confirmation dialog is enabled, a modal will appear listing the file name before proceeding.
- **Discard All** — click the `↩` (rotate-CCW) icon in the *Unstaged* section header. Always shows a confirmation modal that states how many files will be affected.

> **Confirmation dialog** By default Arbor asks you to confirm every single-file discard. You can turn this off in *Settings → Diff & Stage → Confirm before discarding*. The Discard All modal is always shown regardless of this setting.

## Stash

Click the **archive icon** in the unstaged header (or in the sidebar actions bar) to stash all working directory changes. Optionally add a description. Stashes appear in the sidebar under **Stashes** and can be applied, popped, or dropped at any time.

## Merge conflicts

During a merge or rebase, conflicted files appear in a dedicated **Conflicts** section instead of the normal unstaged list. Click **Resolve Conflicts** to open the guided three-panel resolution modal — see *Merge Conflicts* for the full workflow.

---

# Merge Conflicts

When a merge produces conflicts, Arbor detects the state automatically and surfaces a guided resolution workflow. No need to manually edit `<<<<<<<` markers in a text editor.

## Spotting a merge in progress

Three entry points appear simultaneously:

- The **WIP node** in the graph turns amber and shows a pill with the number of conflicted files. A **Resolve** button appears directly on the node.
- The **Branches & Stashes** sidebar shows an amber banner: *"N file in conflitto — Risolvi conflitti…"*
- The **Stage Area** shows a merge notice instead of the normal file lists, with a button to open the resolver.

## Resolution modal layout

Click any entry point to open the modal. It mirrors the main app's IntelliJ-style layout: a card-shaped **file sidebar** on the left and a **two-column editor** + **result panel** on the right.

nostro — your branch (HEAD)

loro — incoming branch

### File sidebar

Lists every file flagged as conflicted at any point during the session. Each row is a card with three possible states:

- ⚠**conflict**regions still need a choice.
- ✓**resolved**composed result written and staged.
- **viewed**opened but no decision yet (greyed badge).

For **modify/delete** and **add/modify** entries the row gets a coloured pill — *"added by them"* or *"deleted by them"* — populated up front via `get_conflict_presence` so the sidebar can show the state without loading every file's three-way content.

The header carries:

- **List ↔ Tree toggle**switches the file list between flat and folder-grouped (same widget pair as the Stage panel). Folders collapse/expand independently.
- **Collapse**circular chevron — hides the sidebar; when collapsed, an icon in the modal title bar reopens it.

**Right-click a file** for a fast-resolve menu:

- **Prendi nostro (<branch>)** — resolves every conflict region in that file by keeping the local side, then stages it.
- **Prendi loro (<branch>)** — same but with the incoming side.

Works on files you haven't opened yet — the conflict content is loaded on demand.

### Modify/delete & add/modify resolver

When one side *deleted* or *added* the file (so there's no overlapping content to merge line-by-line), Arbor swaps the two-column view for a dedicated resolver. The regular diff would mislead by duplicating context lines on both sides — there are no `<<<<<<<` markers in the workdir for these cases.

- **Banner**"Added on <branch>" or "Deleted on <branch>" with a triangle-alert icon.
- **Two stacked cards** *Keep file* — use the version from the side that still has it.
 *Accept deletion* — remove the file from workdir and index (danger / red button).
- **Live preview**shows either the file content that will be kept, or a "file will be removed" placeholder.

### Conflict navigation toolbar

A toolbar across the top of the editor area lets you jump between conflict blocks *inside the active file*:

- **↑ / ↓**step through regions (also bound to `F3` / `Shift+F3`).
- **Counter**"*3 / 7*" — current region over total.
- **‹ ours** / **theirs ›**resolve the active block and advance to the next.
- **"File staged"** badge appears once every region is resolved and the result is written.

### Two-column synchronized view

Each side shows numbered lines. The column header carries the branch name plus a **master checkbox**: tick it to flag every line on that side across *all* conflict regions of the file at once. The checkbox shows an *indeterminate* state when the per-line selections are mixed.

Inside each conflict region you'll find:

- A **"Conflitto N"** header with three small icon buttons on the right: **‹** — accept this region's *ours* (selects all ours lines, deselects theirs) **=** — accept both (selects every line on both sides, ours first then theirs) **›** — accept this region's *theirs* Branch labels live in the column headers above, so the per-region buttons stay compact.
- **Per-line checkboxes** — for fine-grained mixing. Click a line to toggle it.

Long context blocks (more than 30 lines) are **clipped** to the first and last 12 lines with a *"… N righe di contesto nascoste"* placeholder in the middle. Click it to expand. This keeps the modal responsive on huge files where rendering thousands of unchanged lines would otherwise freeze the UI.

### Result panel

The bottom half of the editor area shows the **computed result** from your selections, syntax-highlighted. It's a real editable `textarea` — type directly to override the computed result; a *"modificato manualmente"* badge appears, and *↩ Ripristina* reverts back to the selection-driven version. The horizontal divider between the two-column view and the result is draggable to resize.

### Full file context

The **file icon** in the modal header mirrors the global *Show full file* diff setting: when on, the conflict editor expands every collapsed context block at once instead of trimming long unchanged regions. Useful when the surrounding code matters for choosing between ours/theirs.

### Auto-staging

As soon as *all conflict regions in a file are resolved*, Arbor writes the result to disk and stages the file automatically (equivalent to `git add <file>`). A green checkmark appears in the sidebar — no manual save step needed. Resolved files are remembered for the session even if git later removes them from the conflicted list.

### File encoding

Legacy codebases (Java, PHP, `.properties` on Windows) often
  ship in `windows-1252` rather than UTF-8. Arbor sniffs the
  encoding from the working-tree bytes — UTF-8 BOM or strict UTF-8 →
  UTF-8, otherwise `windows-1252` as a lossless fallback.
  All three stages (ours / theirs / base) are decoded with the same
  encoding so the three-way view never mixes decoders mid-stream.

An **encoding pill** sits in the modal header next to the
  branch chips: it shows the active label (e.g. `UTF-8`) and is
  clickable. Pick a different encoding from the dropdown
  (*UTF-8* / *windows-1252* / *ISO-8859-1* / *ISO-8859-15* / *MacRoman* / *windows-1250* / *Shift_JIS* / *GB18030* / *EUC-KR*) and the file
  reloads with that decoder. The pill takes a warning tint when an
  override is active; *Auto-detect* clears it.

Overrides are persisted per `(repo, file)` in browser
  storage so the choice survives reloads. On save the resolved content
  is re-encoded back to the same byte representation — a windows-1252
  source stays windows-1252 on disk after resolution, never silently
  rewritten as UTF-8.

The same pill appears in **every diff viewer** (Stage Area,
  Commit Detail, Branch Compare, Stash diff) so the same override applies
  consistently across surfaces.

### Completing the merge

Once every conflicted file is resolved, the **Mergia →** button in the footer activates. The commit message input is pre-filled from `.git/MERGE_MSG` with the auto-appended `Conflicts:` section stripped (the conflicted file list is already in the modal — repeating it in the commit message is noise). Edit if needed, then click to create the merge commit.

### Aborting the merge

Click **Annulla Merge** in the footer (to the left of *Mergia*) to discard all resolution work. A confirmation prompt appears — confirm to run `git merge --abort` and restore the working tree.

> **Abort is irreversible** Aborting discards all conflict resolutions you've made so far. You'll need to start over if you re-trigger the merge.

## Blocking files (stash apply)

When a `stash apply` / `pop` can't proceed because tracked or untracked files in the workdir would be overwritten, the same modal opens in *blocking-files* mode. The sidebar shows two clearly-separated sections:

- **Conflicts**regular conflicting tracked files — resolved the usual way.
- **Blocking files**files that don't conflict but can't be applied: local-changes-overwritten, untracked-overwritten, "already exists" / "could not restore untracked". A counter (*"N/total confirmed"*) tracks how many have a decision.

Each blocking file gets a per-row decision: keep your workdir copy, replace with the stash version, or skip. Identical-bytes paths are filtered out automatically (silent apply), so only real blockers reach the user.

## Keyboard shortcut

Press `Esc` inside the modal to trigger the abort confirmation without losing keyboard focus.

---

# Branch Management

Create, switch, rename, and clean up branches without leaving the UI. Ahead/behind counts refresh in real time after every fetch.

## Creating & checking out

- **Create from a commit** — Right-click any commit in the graph → Create Branch. Checked out immediately.
- **Double-click** — Double-click a branch row in the sidebar to check it out.
- **Right-click menu** — Right-click any branch → Checkout.
- **Command Palette** — Press Ctrl+K and type the branch name for a fuzzy match.

## Drag to merge or compare

Drag any branch from the sidebar onto another branch — both local and remote branches are draggable.

1. Hold and drag a branch row — a floating label follows your cursor
2. Drop onto another branch — the target row highlights with a dashed border
3. A small context menu appears with the available actions

### Available actions

When the drop target is the **current HEAD** (local → local), the menu offers four merge strategies plus Compare:

| Field | Value |
| --- | --- |
| Merge | Default `git merge` — fast-forward when possible, otherwise a merge commit. |
| Merge (no fast-forward) | Always creates a merge commit, even when the history is linear. Equivalent to `git merge --no-ff`. |
| Squash merge | Combines all commits of the source into the index without committing — *review & commit from the Stage panel* when done. |
| Fast-forward only | Refuses to create a merge commit. Errors out (and offers no rewrite) when the source isn't a strict descendant of the target. |
| Compare | Full diff modal between the two tips. Always available — works for any local/remote combination. |

### From the Command Palette

The same four strategies are reachable without drag-and-drop. Press `Ctrl+K` and type one of the merge verbs, then pick a branch as the target — HEAD is always the recipient:

| Field | Value |
| --- | --- |
| `Merge` | Default strategy — fast-forward when possible, otherwise a merge commit. Aliases: `merge-default`. |
| `Merge (no fast-forward)` | Always produces a merge commit. Aliases: `no-ff`, `noff`. |
| `Squash Merge` | Stages the combined diff without committing. Alias: `squash`. |
| `Fast-forward Only` | Advances HEAD only when a strict fast-forward is possible. Aliases: `ff`, `ff-only`. |

Outcome toasts mirror the drag-and-drop flow, including the conflict warning that redirects you to the Stage panel.

#### Merge outcome toasts

| Field | Value |
| --- | --- |
| `already_up_to_date` | "*target* already contains *source* — nothing to merge". No commit created. |
| `fast_forward` | Plain fast-forward — branch tip advanced, no merge commit. |
| `merged` | Merge commit was written. |
| `squashed` | Changes staged but not committed — Stage panel takes over. |

> **Merge direction** Dragging `feature` onto `main` merges *feature into main*, not the reverse. The target (drop target) is always the recipient.

### Compare modal

Left panel lists all files that differ between the two tips; click one to load its diff on the right with full syntax highlighting and unified/split mode. Identical branches show a notice instead of an empty list.

### Merge with conflicts

If the merge can't complete automatically, Arbor runs it as far as possible and leaves the repo in a mid-merge state. A warning toast guides you to the **Stage** panel where the conflict resolver takes over.

## Remote operations

- **Fetch** — Download remote refs without merging. Status-bar button or sidebar.
- **Pull** — Fetch + fast-forward (or merge) the current branch. Stashes dirty changes automatically if needed.
- **Push** — Push the current branch to origin. Right-click HEAD in the graph → Push.

The sidebar shows ahead/behind counts as **▲ N** (unpushed) and **▼ N** (behind remote) indicators on each branch. Local branches with no upstream tracking ref get a purple **local** badge so it's obvious which branches still need a first push.

The status bar at the bottom shows the current branch as a clickable chip — **click it to copy the branch name** to the clipboard.

## Renaming a branch

Right-click any local branch → **Rename…**. A modal opens pre-filled with the current name.

### Name rules

- **Non-empty**Cannot equal the current name.
- **No leading**Cannot start with `-` or `.`
- **Forbidden**No spaces, no `~ ^ : ? * [ \`
- **Sequences**No `..`, no trailing `.` / `/`, no `@{}`

### Also rename the remote branch

If a remote tracking ref exists, a toggle **"Also rename remote branch"** appears. Enabled, Arbor runs:

1. Rename the local branch
2. Push the new name to the remote (`git push <remote> <new-name>`)
3. Delete the old remote branch (`git push <remote> --delete <old-name>`)

> **Irreversible — remote rename** Once the old remote branch is deleted, any teammate tracking it will have a broken upstream. The rename button turns red and shows **"Rename + Delete Remote"** as a confirmation prompt.

> **After a local-only rename** Without the remote toggle, only the local ref updates. Update the upstream manually: `git branch --set-upstream-to=<remote>/<new-name> <new-name>`

### Renaming a remote-only branch

Right-click any `origin/<branch>` row → **Rename…** to open a dedicated *Remote Branch Rename* modal. The flow is three steps and is shown progressively as it runs (push tip → delete old → optional local rename):

1. **Push** the existing remote tip to the new name (`git push <remote> <old-sha>:refs/heads/<new-name>`).
2. **Delete** the old remote ref (`git push <remote> --delete <old-name>`).
3. If a **local branch with the same short name** exists, an *"Also rename my local branch"* toggle (on by default) renames it and re-points its upstream to the new remote ref. Otherwise the toggle is hidden.

The same name-validation rules as the local rename apply (no spaces, forbidden chars, `..`, leading `-`/`.`, etc.).

> **Irreversible** Teammates tracking the old name will have a broken upstream once step 2 lands. The confirm button is red and labelled *"Rename + Delete Remote"*.

## Deleting branches

| Field | Value |
| --- | --- |
| Local | Right-click any local branch → **Delete Branch**. Current HEAD cannot be deleted. |
| Remote | Right-click any remote branch → **Delete remote branch**. Confirmation modal appears first. |
| Bulk | Use **Branch Cleanup** (trash icon in *Local Branches* header). |

> **Irreversible — pushes a delete** Deleting a remote branch runs `git push origin --delete <branch>`. Any teammate with a tracking ref will have a broken upstream. Requires credentials configured in *Settings → Git & Integrations*.

## Branch Cleanup

The **trash icon** in the sidebar's *Local Branches* header opens the Branch Cleanup modal. It scans for branches whose tip is fully reachable from a target branch (already merged).

- **Local tab** — Click Scan — all merged branches pre-selected. Deselect to keep any, then bulk-delete.
- **Remote tab** — Loads on open. Deletes push --delete refspec and remove the local tracking ref.

Both tabs share the same target selector — defaults to the current HEAD (or `main` / `master` as fallback).

## Rebase

Available via the graph context menu on the target commit. Arbor delegates rebase to the `git` CLI since the libgit2 API doesn't support interactive rebase.

---

# Tags & Stash

## Tags

The **Tags** section in the sidebar lists all tags in the repo, sorted newest-first using semantic version ordering (`v1.2.3` style). Annotated tags show an **A** badge; tags created locally and not yet pushed show a purple **local** badge.

### Local-only badge

Git itself has no notion of "tag not pushed yet" — once a tag is fetched with `--tags` it lands in `refs/tags/` indistinguishable from one created locally. Arbor tracks this distinction explicitly: tag names you create through Arbor are recorded in `.arbor/config.toml` under `local_only_tags` and removed when you push (or delete) them. The **local** badge in the sidebar reads from that list, so the state survives app restarts and is scoped per-repo.

### Nearest-tag indicator

The status bar shows a v1.2.0 chip with the nearest ancestor tag from `HEAD` — equivalent to `git describe --tags --abbrev=0`. Click it to copy the tag name to the clipboard. Works intelligently across branch types:

- **Integration branches** (`main`, `develop`) — shows the last published version tag
- **Feature branches** — shows the version tag the branch was cut from
- **Hotfix branches** (e.g. `hotfix/1.2.x`) — shows the tag being patched (e.g. `v1.2.0`)

### Interacting with tags

- **Click** a tag in the sidebar → scrolls to the tagged commit in the graph.
- **Right-click** a tag for a context menu. The available items adapt to whether the tag is still local-only or already on the remote: **Copy value** — copies the tag name to the clipboard. **Push to origin** — only shown for tags with the *local* badge. Pushes `refs/tags/<name>` and clears the badge. **Elimina localmente** — opens a confirmation modal, then removes the tag only from the local repo. **Elimina locale + origin** — only shown when the tag exists on the remote. Opens a stronger confirmation modal warning that the action is irreversible, then pushes `:refs/tags/<name>` (empty source = delete on remote) and deletes the local ref.

### Creating tags

Right-click any commit in the graph → **Create Tag…**. The modal's primary action is a **split button**:

- **Create** (left side) — creates the tag locally and flags it as *local* until pushed.
- **▾ chevron** (right side) opens a small menu with **Create & Push**, which creates the tag and pushes it to `origin` in one step.

If you provide a message in the input, an annotated tag is created (`A` badge); otherwise it's a lightweight tag.

## Stash

The stash saves your working directory changes (and staged files) onto a stack so you can switch context without committing. The **Stashes** section in the sidebar lists all entries.

### Creating a stash

There are three entry points:

- **WIP node — context menu** — Right-click the WIP node at the top of the graph. Stash Changes — includes untracked files. Stash (exclude untracked) — only tracked modifications and staged changes.
- **Stage Area — stash button** — The toolbar in the Stage Area has a stash icon. Clicking it opens a small form where you can type an optional message before stashing. Saves with include untracked enabled.
- **Sidebar — Stash button** — The RepoActions bar in the sidebar also has a stash shortcut with the same optional-message form.

### Browsing stashes

Each stash entry in the sidebar shows its message (or `stash@{N}` if no message was set). **Click** a stash to load its diff in the Detail panel — useful for reviewing what is saved before deciding whether to apply.

### Applying a stash

- **Apply (▶)** — Re-applies the stash to the working directory. The stash entry is kept on the stack — useful when you want to apply the same changes to multiple branches.
- **Pop (↵)** — Applies the stash and removes it from the stack in one step. Equivalent to git stash pop.

Both actions are available as inline hover buttons on the stash row and in the right-click context menu.

#### Apply outcomes

The toast tells you exactly what happened — no need to `git status` after:

| Field | Value |
| --- | --- |
| *Stash applied* | Default success — changes are now in the workdir. |
| *Stash popped & dropped* | Same but for `pop` — entry was removed from the stack. |
| *No changes — working tree already matches the stash* | The workdir already contained every line of the stash. Apply is a no-op; pop additionally drops the entry (toast says *"Stash dropped"*). Distinct from generic success so you don't wonder where the diff went. |

### Renaming a stash

Click the **pencil icon** on a stash row (visible on hover) or use the right-click context menu → **Rename**. An inline text input replaces the message in-place:

| Key | Action |
| --- | --- |
| Enter | Confirm rename |
| Escape | Cancel without saving |

### Dropping a stash

Click the **trash icon** (red on hover) or right-click → **Drop**. The entry is permanently removed from the stack.

### Conflict handling

Three situations can interrupt an apply or pop — all three now flow through the **same modal** so you don't bounce between dialogs:

- **Local-changes-overwritten** — Tracked files in the workdir would be replaced. Per-row choice: keep your version, take the stash version, or skip.
- **Blocking untracked files** — Untracked workdir files would be overwritten by stashed untracked content. Same per-row controls.
- **Merge conflicts** — Tracked files with overlapping edits — guided two-column + result resolver, identical to the merge flow.

Bytes that already match between workdir and stash are filtered out before the modal opens, so identical files don't show up as blockers (silent-apply path).

> **Pull auto-stash** — when you pull a branch with a dirty working directory, Arbor automatically stashes first, pulls, then pops the stash. If the pop has conflicts the same resolution modal appears with the original stash entry preserved.

---

# Submodules

Arbor shows rich status for each Git submodule — current branch, ahead/behind counts, dirty state — and lets you fetch, pull, push, and switch branches directly from the sidebar.

## Sidebar section

When the active repository contains submodules, a **Submodules** section appears in the Branches & Stashes sidebar (below Tags). It is hidden automatically for repos with no submodules.

The section badge turns amber when at least one submodule is uninitialised, has local changes, or is behind its remote tracking branch.

## Row layout

Each row shows two lines on the left and a set of badges on the right:

| Element | Description |
| --- | --- |
| Name | Submodule name in primary text. A • dot indicates a dirty working directory (uncommitted changes inside the submodule). |
| Path | Relative path from the parent repo root, shown in a smaller monospace font. |
| Branch badge | Pill showing the current branch name. If the submodule is in detached HEAD state, the short commit hash is shown in an amber badge instead. |
| ↑N Ahead | Number of commits the submodule is ahead of its remote tracking branch (green). |
| ↓N Behind | Number of commits the submodule is behind its remote tracking branch (amber). |
| ● Synced | Small green dot — visible only when the submodule has a branch and is fully in sync (ahead = 0, behind = 0). |
| ⚠ Warning icon | Shown when the submodule is not initialised / not cloned yet. |
| Spinner | Replaces all badges while a fetch / pull / push is in progress for that row. |

## Opening a submodule as a tab

An initialised submodule is itself a full Git repository. You can open it in its own tab in two ways:

- **Double-click** the row.
- Right-click → **Open as Tab** from the context menu.

Arbor checks whether the path is already open; if so it switches to the existing tab instead of opening a duplicate.

## Context menu operations

Right-click any row to open the context menu.

| Action | Git equivalent |
| --- | --- |
| Fetch | git fetch inside the submodule directory |
| Pull | git pull inside the submodule directory |
| Push | git push inside the submodule directory |
| Checkout Branch… | Opens the Checkout Branch modal (see below) |
| Open as Tab | Opens the submodule as a new tab in Arbor |

All sync operations (Fetch / Pull / Push) are disabled for uninitialised submodules. After each operation the sidebar data refreshes automatically. Errors (e.g. merge conflicts on pull, rejected push) are shown as toast notifications containing the raw `git` output.

## Checkout Branch modal

Select **Checkout Branch…** from the context menu to open a compact modal that:

- Lists all local and remote branches available in the submodule (remote branches have their `origin/` prefix stripped and are deduplicated).
- Pre-selects and marks the currently checked-out branch as *current*.
- Disables the Confirm button when the current branch is already selected.
- Shows a spinner during the branch-list fetch and during the checkout itself.

> **Adding or removing submodules** These operations are not supported from the UI. Use the integrated terminal or an external shell: `git submodule add <url> <path>` / `git rm <path>`

## Initialising submodules

Uninitialised (not-yet-cloned) submodules show a warning icon and all sync operations are disabled. To initialise them use the integrated terminal:

```
git submodule update --init
# or, for nested submodules:
git submodule update --init --recursive
```

## Technical notes

- All submodule operations spawn a `git` CLI subprocess with the submodule directory as the working directory — they do not use libgit2's submodule write API, which is incomplete.
- Ahead/behind counts are computed with `git2::Repository::graph_ahead_behind()` against the upstream tracking branch configured inside the submodule's own `.git/config`. If no upstream is configured the counts show as 0.

---

# Git Flow

Arbor includes a built-in Git Flow implementation based on the **Vincent Driessen branching model** â€” structured workflows for feature, release, and hotfix branches, with optional PR/MR integration and ticket-based branch naming.

## Opening the Git Flow panel

Click the **Git Merge** icon (second icon) in the Activity Bar to open the Git Flow sidebar panel.

## Initialization

If the repository has never been initialized with Git Flow, the panel shows an **Initialize** button. This creates the `develop` branch (if it doesn't exist) and records the prefix configuration. Branch prefixes default to:

### Non-standard flow (no develop)

Arbor works with repositories that **don't follow the standard `main`/`develop` split**. When `main` exists but `develop` doesn't, the panel is fully usable and you can still create feature/release branches â€” they are created from `main` instead of `develop`, and finishing them merges back into `main`.

- A yellow **"Non-standard flow"** banner is shown at the top of the panel in this mode. It carries an **Initialise** shortcut that creates the missing `develop` branch from `main` if you want to switch to the full Git Flow.
- The **first time** you start a feature or release in this mode for a given project, a confirmation dialog explains that the branch will be cut from `main`. Confirming the dialog stores an acknowledgement per project â€” subsequent starts go through silently.
- A toast after the start reminds you which base branch was used (e.g. *"feature 'foo' started from main"*).
- If neither `main` nor `develop` exists, the panel falls back to the standard "create main" flow before anything else can be done.

| Branch type | Default prefix |
| --- | --- |
| feature | feature/ |
| release | release/ |
| hotfix | hotfix/ |
| bugfix | bugfix/ |
| support | support/ |

## Workflows

### Feature branches

- **Start** â€” creates `feature/<name>` from `develop` (or from `main` if `develop` doesn't exist) and checks it out
- **Finish** â€” merges feature branch into `develop` with `--no-ff` (or into `main` when `develop` is missing); optionally deletes the branch after

### Release branches

- **Start** â€” creates `release/<version>` from `develop` (falls back to `main` when `develop` is missing)
- **Finish** â€” merges into `main` and, when present, into `develop`; optionally creates a version tag

### Hotfix branches

- **Start** â€” creates `hotfix/<name>` from `main` (the production branch)
- **Finish** â€” merges into both `main` and `develop`; optionally creates a tag

## PR / MR integration

Arbor supports both local merges and Pull / Merge Request workflows. The behaviour is controlled by two settings per branch type:

| Setting | What it does |
| --- | --- |
| finish.feature_use_pr | Force PR/MR â€” feature finish always pushes the branch and opens the PR/MR form (no local merge) |
| finish.feature_pr_default | When not forced, sets the default action for the primary Finish button. false (default) = merge locally; true = open PR/MR |
| finish.release_use_pr | Force PR/MR on release finish |
| finish.release_pr_default | Default primary button action for release finish |
| finish.hotfix_use_pr | Force PR/MR on hotfix finish |
| finish.hotfix_pr_default | Default primary button action for hotfix finish |

When a finish type is **not** forced, the Finish button becomes a **split button**: the primary click uses the configured default, and the chevron `â–¾` lets you choose between "Finish normally (merge locally)" and "Finish with PR/MR" for that individual operation.

Configure in **Settings â†’ Git Flow**. Each setting can be overridden per project.

## Ticket-based branch naming

When an issue tracker is configured for the project (see **Settings â†’ Repository â†’ Issue Tracker**), the "Start Feature" form shows a **Ticket** field with a picker button. Clicking it opens a full-screen modal with the same search and filter interface as the Issues sidebar â€” search bar, status / project / milestone / assignee chips â€” and issue cards with colored status icons, labels, and assignees.

Selecting a ticket closes the modal and auto-fills the branch name field with the ticket identifier, producing branches like `feature/ABO-123`.

- The ticket picker is available **by default** whenever a tracker is configured â€” no flag required.
- Enable `require_ticket_branch` to make ticket selection **mandatory** (the branch name field must be filled from a ticket).
- If `require_ticket_branch` is on but no issue tracker is configured for the project, a warning is shown and the branch name can be typed freely.
- Currently supported tracker: **Linear**. Jira coming soon.

## Configuration

Git Flow settings are stored in two layers:

- **Global** â€” in `~/.config/arbor/config.toml` under `[gitflow]` â€” applies to all repositories
- **Per-repo** â€” in `<repo>/.arbor/config.toml` under `[gitflow]` â€” overrides the global config for that repo only

Both layers are editable from **Settings â†’ Git Flow**.

```toml
[gitflow]
main_branch            = "main"      # or "master"
develop_branch         = "develop"
require_ticket_branch  = false       # force ticket-based branch names on feature start

[gitflow.prefixes]
feature = "feature/"
release = "release/"
hotfix  = "hotfix/"
bugfix  = "bugfix/"
support = "support/"

[gitflow.finish]
feature_delete_branch = true   # delete feature branch after finish
feature_squash        = false  # squash commits on feature finish
release_tag           = true   # create a version tag on release finish
release_tag_prefix    = "v"    # tag prefix, e.g. "v1.2.0"
hotfix_tag            = true   # create a tag on hotfix finish
feature_use_pr        = false  # force PR/MR on feature finish (overrides default)
feature_pr_default    = false  # default button: false = merge, true = PR/MR
release_use_pr        = false  # force PR/MR on release finish
release_pr_default    = false  # default button for release finish
hotfix_use_pr         = false  # force PR/MR on hotfix finish
hotfix_pr_default     = false  # default button for hotfix finish
```

## Plugin hooks

Plugins can react to every Git Flow operation. Declare the hooks in `[hooks]` and register handlers with `arbor.events.on()`:

| Hook constant | TOML key | Context fields |
| --- | --- | --- |
| FLOW_INIT | on_flow_init | repo |
| FLOW_FEATURE_START | on_flow_feature_start | repo, name, branch, base_branch |
| FLOW_FEATURE_FINISH | on_flow_feature_finish | repo, name, branch |
| FLOW_RELEASE_START | on_flow_release_start | repo, version, branch, base_branch |
| FLOW_RELEASE_FINISH | on_flow_release_finish | repo, version, branch |
| FLOW_HOTFIX_START | on_flow_hotfix_start | repo, name, branch, base_branch |
| FLOW_HOTFIX_FINISH | on_flow_hotfix_finish | repo, name, branch |

```lua
-- plugin.toml [hooks] section
-- on_flow_feature_start = true
-- on_flow_feature_finish = true

arbor.events.on("on_flow_feature_start", function(ctx)
  -- ctx.repo   = "/path/to/repo"
  -- ctx.name   = "my-feature"    (name part only, without prefix)
  -- ctx.branch = "feature/my-feature"  (full branch name)
  arbor.log.info("Feature started: " .. ctx.branch)
end)

arbor.events.on("on_flow_feature_finish", function(ctx)
  arbor.notify{ title = "Feature merged", message = ctx.branch .. " merged into develop", level = "success" }
end)
```

---

# Ticket Links

Arbor can associate commits with tickets from your issue tracker â€” automatically
    by parsing commit messages and branch names, or manually via right-click.
    Linked tickets appear as small chips on each graph row and in the commit detail panel.

## How it works

- **Auto-detect (message)** â€” Arbor scans each visible commit message for
      ticket IDs matching the configured tracker pattern (e.g. `ENG-123` for Linear, `#456` for GitHub / GitLab). Results are cached in memory â€” no re-scan on scroll.
- **Auto-detect (branch)** â€” Branch names pointing to a commit are also scanned
      (e.g. `feature/ENG-123-login-flow`).
- **Manual link** â€” Right-click a commit â†’ *Link to ticketâ€¦* to open the
      ticket picker and create a persistent association stored in the backing store.

## Storage backends

Manual links can be stored in one of two backends. The backend is exclusive â€”
    only one is active per repository at a time (no mixed reads).

| Backend | Location | Distributed on push? |
| --- | --- | --- |
| git_notes (default) | refs/notes/arbor/tickets in the git object store | Only if you configure the push refspec (see below) |
| links_toml | .arbor/links.toml in the repository root | Yes, if you commit and push the file |

## Configuration

Global defaults live in `~/.config/arbor/config.toml`.
    Per-repository overrides go in `.arbor/config.toml` inside the repo.
    Project settings take precedence.

### Global config (~/.config/arbor/config.toml)

```toml
[ticket_links]
enabled    = true          # master switch (also in Settings â†’ Graph)
storage    = "git_notes"   # "git_notes" | "links_toml"
auto_parse = true          # parse commit messages + branch names
warn_push  = true          # warn when notes push refspec is missing
```

### Per-repo config (.arbor/config.toml)

```toml
[ticket_links]
storage        = "links_toml"      # override the global backend for this repo
tracker        = "linear"          # "linear" | "jira" | "github" | "gitlab"
auto_parse     = true
custom_pattern = "\\b(MYCO-\\d+)\\b"  # optional â€” overrides the tracker default
```

`custom_pattern` can also be set via **Settings â†’ Repository â†’ Ticket Links** without editing the TOML file manually. The value must be a valid Rust regex with exactly
    one capture group â€” the captured text becomes the ticket ID.

**Tip:** `tracker` can also be set via the existing `issue_tracker` field in `.arbor/config.toml` â€” the
    ticket-links system inherits it as a fallback.

## Sharing git notes with teammates

By default, `git push` does not include notes.
    Add the following to your `.git/config` (or run the equivalent `git config` commands) to push and fetch notes automatically:

```
[remote "origin"]
    fetch = +refs/notes/*:refs/notes/*
    push  = refs/notes/*
```

Arbor will warn you after a push if this refspec is not yet configured.

## UI elements

- **Graph chips** â€” Colored pill badges on each row.
      Color indicates the tracker: purple = Linear / Jira, grey = GitHub, orange = GitLab.
      Click to open the issue detail. Hover a manually-added chip to reveal the âœ• remove button.
- **Commit detail panel** â€” "Tickets" row below the commit body
      showing all linked tickets. Manual links have an âœ• button to remove them.
- **Right-click â†’ Link to ticketâ€¦** â€” Opens the ticket picker
      to create a manual association.
- **Issue detail â†’ Linked Commits** â€” When viewing a ticket in the
      issues sidebar, a *Linked Commits* section loads lazily and shows every
      commit associated with that ticket (both auto-detected and manual). Each entry
      displays the short SHA, summary, author, date, and branch chips (when the
      commit is already in the graph cache). Click any entry to navigate directly to
      that commit in the graph.
- **Settings â†’ Graph â†’ Ticket link chips** â€” Toggle to disable the
      feature entirely if you experience scroll slowdowns on very large repos.

## Reverse lookup: ticket â†’ commits

The *Linked Commits* section in the issue detail provides full reverse lookup:

- **Manual links (git notes)** â€” All notes under `refs/notes/arbor/tickets` are scanned.
- **Manual links (links.toml)** â€” The full `.arbor/links.toml` file is read (served from cache when warm).
- **Auto-detected** â€” Commits already scrolled into view whose
      message or branch name matched the ticket ID are included. Commits not yet
      loaded in the graph are not covered by auto-detection (scroll more of the
      graph to widen the search).

## Ticket ID patterns

| Tracker | Default pattern | Example |
| --- | --- | --- |
| Linear | [A-Z][A-Z0-9]*-\d+ | ENG-123, PROJ-42 |
| Jira | [A-Z][A-Z0-9]*-\d+ | PROJ-456, ABC-7 |
| GitHub | #\d+ | #456, fixes #789 |
| GitLab | #\d+ | #123 |

Any tracker's default pattern can be overridden with a **custom regex** per repository.
    Set it in **Settings â†’ Repository â†’ Ticket Links** or directly in `.arbor/config.toml`:

```toml
[ticket_links]
tracker        = "jira"
custom_pattern = "\\b(MYCO-\\d+)\\b"   # must have exactly one capture group
```

When `custom_pattern` is set it takes full precedence â€” the tracker default is ignored.
    The captured text (group 1) becomes the ticket ID stored and displayed on the chip.
    Invalid regex is silently ignored and the tracker default is used instead.

---

## Git Notes

Git notes let you attach freeform text to any commit *without modifying the commit itself*.
    Notes are stored in a parallel ref (`refs/notes/<namespace>`) so the commit hash is
    never changed — useful for personal annotations, code-review remarks, or linking external context.

### Key Concepts

- **Namespace** — each note belongs to a namespace (e.g. `commits`, `review`, `jira`). The default git namespace is `commits`. Namespaces follow git ref naming rules: no spaces, no `~^:?*[\`, no `..`, and cannot start or end with `.`.
- **Local vs Remote** — notes are *not* pushed automatically with `git push`. You must push them explicitly with `git push origin refs/notes/commits`. Arbor shows the remote sync status of each note.
- **Compatibility** — notes are plain text; any git client can read them with `git log --show-notes`.

### Using Notes in Arbor

#### Adding a Note

Right-click any commit in the graph and choose **Notes…**, or click the **Notes** row in the Commit Detail panel.

In the modal, click **Add note** and fill in:

- **Namespace** — defaults to `commits`. Use a different name to separate concerns (e.g. `review`, `deploy`).
- **Content** — freeform text.

#### Graph Badge

Commits that have at least one note show a small pill (with a count) right after the commit message in the graph. Clicking it opens the notes modal directly.

#### Remote Status

When the modal opens, Arbor checks each note against its remote tracking ref (`refs/remotes/origin/notes/<namespace>`):

- **Local only** — note exists only locally; never pushed.
- **In sync** — local and remote blobs match.
- **Out of sync** — local note differs from remote (local is ahead).

Use the **refresh** icon on each note to re-check its remote status after a push.

### Plugin API — arbor.notes

Requires `git = "read"` for read operations, `git = "write"` for write operations.

| Function | Description |
| --- | --- |
| arbor.notes.list(commit_oid) | Returns an array of { namespace, content, created_at, remote_status } for the active tab's commit. created_at is a Unix timestamp (seconds). |
| arbor.notes.get(commit_oid, namespace) | Returns the note content string, or nil if no note exists. |
| arbor.notes.set{ commit_oid, namespace, content } | Create or overwrite a note. Returns (true, nil) on success, (false, err) on git failure. Fires on_note_saved hook. |
| arbor.notes.delete(commit_oid, namespace) | Delete a note. Fires on_note_deleted hook. |

#### Example

```
-- Auto-annotate commits that reference a Jira ticket
arbor.events.on("on_commit", function(ctx)
  local msg = ctx.summary or ""
  local ticket = msg:match("[A-Z]+%-%d+")
  if ticket then
    arbor.notes.set{ commit_oid = ctx.oid, namespace = "jira", content = ticket }
  end
end)
```

### Plugin Hooks

| Hook | Context fields |
| --- | --- |
| on_note_saved | tab_id, commit_oid, namespace |
| on_note_deleted | tab_id, commit_oid, namespace |

### Plugin Manifest

```
[hooks]
on_note_saved   = true
on_note_deleted = true

[permissions]
git = "write"
```

---

# Worktrees

Worktrees are Git *linked worktrees* — additional checked-out working directories that
  share the same repository. Each worktree has its own branch, working tree, and HEAD commit,
  letting you switch contexts instantly without stashing or committing.

> **Fast switch** Double-click any worktree row in the sidebar to open it as a new tab immediately.

## Sidebar panel

Expand the **Worktrees** section in the left sidebar (Layers icon).
  Each row shows the project-type emoji, branch name, and status badges:

| Badge | Meaning |
| --- | --- |
| 🏠 Home | Main worktree — the directory where .git/ lives. Cannot be removed. |
| ⊙ CircleDot | Currently open in the active tab. |
| 🔒 Lock | Locked via git worktree lock — cannot be pruned accidentally. |

## Adding a worktree

1. Click the **+** button in the Worktrees header.
2. Choose a destination folder (folder picker dialog).
3. Select an existing branch *or* enable **Create new branch** and type a name.
4. Click **Add Worktree** — Git creates the linked worktree immediately.

## Switching worktrees

- **Double-click** a row — opens the worktree path as a new tab (equivalent to *Open Recent*).
- **Right-click → Switch to this worktree** — same action from the context menu.
- **ⓘ Info modal → Switch here** — switches from the info overlay.

## Right-click context menu

Right-click any worktree row to see:

- **Switch to this worktree** — opens the worktree in a new tab (only visible when not current).
- **Worktree info** — opens the info modal.
- **Open in IDE** — sub-section listing every IDE detected on the system plus any custom IDEs.
      The IDE that matches the project-language default (or the global default) shows a *Default* badge.
- **Remove worktree** — runs `git worktree remove`. Only visible for non-main worktrees.
      Locked worktrees cannot be removed without unlocking them first.

## Info modal

Click the ⓘ button on any row, or use the context menu. The modal shows:

- **Details** — Full path, branch name, HEAD commit SHA, and detected project type (Rust, Node.js, Java Maven/Gradle, Go, Python, .NET, C++, Ruby, PHP).
- **Sync Status** — Blue ↑ N ahead chip and orange ↓ N behind chip relative to the remote upstream. Green Up to date when in sync. Purple N changes chip for local modifications; green Clean when the working tree is untouched.

The action bar at the bottom of the modal offers **Switch here** and **Open in IDE** buttons.

## Project-type detection

Arbor inspects each worktree directory for build-system markers to assign a project type.
  The emoji displayed in the sidebar reflects the detected type:

| Emoji | Type | Detected by |
| --- | --- | --- |
| 🦀 | Rust | Cargo.toml |
| 🟩 | Node.js | package.json |
| ☕ | Java (Maven) | pom.xml |
| ☕ | Java (Gradle) | build.gradle / build.gradle.kts |
| 🐹 | Go | go.mod |
| 🐍 | Python | pyproject.toml, setup.py, or requirements.txt |
| 🔷 | .NET | *.csproj or *.sln |
| ⚙️ | C++ | CMakeLists.txt or Makefile |
| 💎 | Ruby | Gemfile |
| 🐘 | PHP | composer.json |

## IDE integration

Each worktree can be opened directly in any IDE that Arbor has detected on the system.
  Configure IDE preferences in **Settings → Project → IDE Integration**.

- The **default IDE per language** setting means a Rust project opens in RustRover (or whichever IDE you chose for Rust) while a Node.js project opens in WebStorm — automatically, via the same "Open in IDE" menu entry.
- On Windows, IDEs that ship as batch scripts (`code.cmd`, `cursor.cmd`, etc.) are launched correctly through `cmd /c` — no manual workaround needed.

> **Git worktrees vs. branches** A worktree is not a clone — it shares the full Git history and object store with the main repository. Disk usage is minimal (only the working tree files are duplicated). You can have multiple branches checked out simultaneously without any stashing.

---

# File Tree

The **File Tree** panel shows every tracked file in the repository as a collapsible directory tree, with per-file last-commit metadata loaded progressively in the background.

## Opening the panel

Click the **Files** icon in the Activity Bar (folder icon) to toggle the File Tree sidebar section.

## Tree navigation

| Action | How |
| --- | --- |
| Expand / collapse a folder | Click the folder row or its chevron |
| Filter graph by file | Click a file row (click again to clear) |
| Context menu | Right-click any file row |
| Search files | Type in the search box at the top of the panel |
| Refresh | Click the ↺ refresh button in the panel toolbar |

## File & folder icons

Icons are resolved using the **VS Code Icons** set (Iconify). Resolution order for files:

1. **Exact filename match** — e.g. `Cargo.toml`, `Dockerfile`, `package.json`
2. **`.env*` prefix** — any file starting with `.env` gets the dotenv icon
3. **`.d.ts` suffix** — TypeScript definition files
4. **Extension lookup** — Rust, TypeScript, Svelte, Python, Go, Java, Kotlin, C/C++, and 30+ more
5. **Fallback** — plain text icon

Folders are also matched by name: `src`, `components`, `node_modules`, `dist`, `test`, `docs`, `styles`, `types`, and many others resolve to semantic folder icons.

## Last-commit metadata

Each file row shows a faint right-aligned column with:

- **Short commit SHA** — 7-character OID of the last commit that touched the file
- **Relative date** — e.g. *today*, *3d ago*, *2mo ago*
- **Commit summary** — one-line commit message (truncated)

Metadata is loaded **lazily**: the file list itself appears immediately (reading the git index is instant).
  The last-commit info is then streamed from a background Rust thread via batched Tauri events
  (`arbor://file-meta-batch`), so the tree remains usable while metadata fills in progressively.

> **Session cache** — completed scans are saved to `sessionStorage` keyed by repository path + HEAD fingerprint. Re-opening the panel (or switching tabs and back) is instant as long as HEAD has not moved.

## File search

The search box filters files using a **multi-tier fuzzy search**:

| Priority | Match type |
| --- | --- |
| 1 (highest) | Exact filename match |
| 2 | Filename starts with query |
| 3 | Filename contains query |
| 4 | Full path contains query |
| 5 | Fuzzy match on filename (characters appear in order) |
| 6 | Fuzzy match on full path |

Results are capped at 200 items. The search is debounced by 150 ms to avoid scoring on every keystroke.

> **Command Palette** — the *Modified Files* section in the Command Palette (`Ctrl+K`) also searches the file tree and dispatches an `arbor:navigate-to-file` event that expands all ancestor folders and scrolls the target file into view.

## Context menu actions

Right-click any file to access:

- **Git Blame** — Opens the Git Blame modal for the selected file — see below for details.
- **Filter Graph by File** — Filters the commit graph to show only commits that touched this file. A pill in the graph toolbar shows the active filter; click × to clear it. Also reachable from the Command Palette via Show Commits Touching File (aliases file-history / log-file / history) — that route lists every project file and doesn't open the File Tree sidebar.

## Git Blame

The Git Blame modal shows the full content of a file annotated line-by-line with the commit that last
  modified each line. It can be opened either from the **right-click context menu** in the
  File Tree, or from the Command Palette via the `Blame File` verb (aliases `blame` / `annotate`) — the palette route lists every tracked file in the project, so you don't need
  the File Tree sidebar to be open.

### Reading the blame view

- **Colored left border** — each distinct commit gets a consistent color from a 10-color palette, making it easy to spot which lines belong to the same change
- **SHA chip** — 7-character short OID of the responsible commit, shown only on the *first line of each group* (is_group_start)
- **Author & date** — author display name and relative date, also shown only on group-start lines
- **Commit summary** — one-line message in muted text below the author row
- **Syntax highlighting** — the code column is highlighted with Prism using the file's extension

### Interactions

| Action | How |
| --- | --- |
| Highlight all lines from the same commit | Hover any line — all lines sharing the same OID are highlighted |
| Navigate to commit in graph | Click the SHA chip — the graph scrolls to that commit and the modal closes |
| Close modal | Escape or click the backdrop |

> **Under the hood** — blame is computed by the Rust backend via `git2::Repository::blame_file()` and returned as a flat array of `BlameLine` structs (one per source line). Each `BlameLine` carries: `line_no`, `content`, `commit_oid`, `short_oid`, `author_name`, `author_email`, `timestamp`, `summary`, and a `is_group_start` flag set when the commit OID changes from the previous line.

---

# Reflog

The **Reflog** panel shows a complete history of where `HEAD` has pointed —
  every checkout, commit, merge, rebase, and reset — even for commits no longer reachable from any branch.

## Opening the panel

Click the **History** icon (clock arrow) in the Activity Bar to toggle the Reflog sidebar.

## Reading an entry

| Element | Meaning |
| --- | --- |
| HEAD@{n} badge | Position in the reflog — HEAD@{0} is the most recent |
| Hash chip (accent color) | 7-character short OID of the commit HEAD moved to |
| Action badge | Type of operation that moved HEAD (see below) |
| Message | Git's description of the operation, e.g. checkout: moving from main to feature/x |
| Relative time | When the operation occurred; hover for the full date/time |

## Action types

- **Commit** — A new commit was created — ordinary git commit, amend, cherry-pick, or revert.
- **Checkout** — HEAD was moved to a different branch or detached to a specific commit.
- **Merge** — A merge was performed — fast-forward or three-way.
- **Rebase** — HEAD moved as part of a rebase operation (one entry per replayed commit).

## Filters

The toolbar exposes two filters:

- **Type** — filter by action kind (Commit, Checkout, Merge, Rebase, Other). Multiple types can be selected simultaneously.
- **Sort** — switch between *Newest first* (default, matches git output) and *Oldest first*.

The **search box** filters by message text or hash prefix in real time. Use the **Clear** chip to reset all active filters at once.

## Pagination

Up to **200 entries** are loaded from the backend on open. The panel displays **50 at a time** — click *Show more* at the bottom to reveal the next 50.
  The count of remaining entries is shown inline.

## Context menu actions

Right-click any entry to access:

- **Checkout this commit** — Detaches HEAD to the entry's commit OID. The graph refreshes automatically.
- **Create branch here** — Opens the New Branch modal pre-filled with the entry's hash — useful for recovering commits no longer reachable from any branch.
- **Copy hash** — Copies the full 40-character OID to the clipboard.

> **Recovering lost commits** — if you accidentally reset a branch or dropped a stash, find the commit in the Reflog, right-click → *Create branch here* to restore it before Git's garbage collector runs (typically after 30–90 days).

## Under the hood

The backend reads the reflog via `git2::Repository::reflog("HEAD")` and returns a flat
  array of `ReflogEntry` structs:

| Field | Type | Description |
| --- | --- | --- |
| index | usize | Position in reflog (HEAD@{index}) |
| id | String | Full OID HEAD moved to |
| id_old | String | Full OID HEAD moved from |
| message | String | Git's reflog message |
| committer_name | String | Name from the reflog signature |
| committer_time | i64 | Unix timestamp of the operation |

---

# Recovery Journal

The **Recovery Journal** is Arbor's automatic safety net — before every destructive
  git operation, a full snapshot of your working tree and index is saved as an unreachable git
  object and logged in `.git/arbor-recovery/journal.jsonl`.
  If something goes wrong you can browse and restore any snapshot with one click.

## What triggers a snapshot

Snapshots are created automatically — no action required — before:

- **Reset —hard** — Any hard reset of HEAD or the index, including interactive rebase steps.
- **Checkout** — Branch or commit checkout that modifies tracked files in the working tree.
- **Discard changes** — "Discard file" or "Discard all changes" from the Stage panel.
- **Stash force-apply** — Force-applying a stash over conflicting untracked files.
- **Stash drop** — Dropping a stash entry manually from the Stash panel.
- **Other** — Any operation not in the above categories that may overwrite work.

> Snapshots are taken **before** the operation runs, so even if the operation fails mid-way you still have a clean restore point.

## Opening the Recovery tab

Click the **History** icon (clock-arrow) in the Activity Bar to open the Reflog
  sidebar, then switch to the **Recovery** tab at the top.

## Reading a recovery entry

| Element | Meaning |
| --- | --- |
| shield badge + kind label | Type of operation that triggered the snapshot (Checkout, Reset·hard, Discard, etc.) |
| Summary line | Human-readable description, e.g. checkout branch 'feature/x' |
| Relative time | When the snapshot was taken; hover for the full date/time |
| File-warning icon | Some files were too large or had denied extensions and were logged but not preserved |
| Consumed badge | Entry has been restored; the pinning ref has been removed |

## Preview & Restore

Click any entry to expand it and see a **preview diff** — the list of files
  that would change if you restored that snapshot from the current state.

Click **Restore** to apply the snapshot via `git stash apply`.
  Arbor always uses *apply* (never pop) so the snapshot is preserved in case the apply
  produces conflicts. Once the apply is clean, the pinning ref is automatically released.

> Restoring a snapshot overwrites your current working tree. Arbor takes a new safety snapshot *before* each restore, so the operation is always reversible.

## Deleting entries

Use the trash icon on an entry to remove it. This drops the pinning `refs/arbor/recovery/…` ref — the objects become eligible for git garbage
  collection after the standard unreachable-object grace period.

## Automatic expiry

Entries older than the configured **retention period** (default: **30 days**)
  are pruned lazily each time the recovery list is loaded. You can adjust the retention period and
  other limits in **Settings → Performance → Recovery**.

## Reflog vs. Recovery Journal

|  | Reflog | Recovery Journal |
| --- | --- | --- |
| What it tracks | Every position of HEAD — commits, checkouts, merges, rebases | Working-tree + index snapshots before destructive ops |
| Uncommitted work | Not preserved — only the committed state | Fully preserved (working dir + staged changes) |
| Managed by | Git itself | Arbor exclusively |
| When to use | Recover a lost commit after reset or force-push | Recover uncommitted work after a discard or checkout |

## Settings

Configure the journal in **Settings → Performance → Recovery**:

| Setting | Default | Effect |
| --- | --- | --- |
| Max file size | 2 MB | Files larger than this limit are logged in the journal but their content is not preserved in the snapshot. |
| Retention period | 30 days | Snapshots older than this are pruned on next load. Matches git's default unreachable-object expiry. |
| Denied extensions | zip, mp4, exe, dll, jar, psd, … | Files with these extensions are never content-preserved — only logged. Avoids bloating .git with build artifacts and binaries. |

## Under the hood

Snapshots use the same mechanism as `git stash create` — they produce a commit
  containing a tree of the working directory with a separate parent tree for the index.
  Unlike a real stash, the commit is **not** pushed to `refs/stash`.
  Instead, it is pinned under a dedicated namespace:

```
refs/arbor/recovery/<id>-<kind>
```

This keeps the objects alive through garbage collection until Arbor's TTL expires and the
  ref is explicitly removed. The journal itself is stored as an append-only JSONL file at:

```
.git/arbor-recovery/journal.jsonl
```

Each line is a self-contained JSON object with the fields below.

| Field | Type | Description |
| --- | --- | --- |
| id | u64 | Monotonically-increasing unique identifier |
| created_at | i64 | Unix timestamp of snapshot creation |
| kind | string | One of: reset_hard, checkout, discard, stash_force_apply, stash_drop, pull, other |
| summary | string | Human-readable description of the triggering operation |
| snapshot_oid | string | Full OID of the stash-create commit (null if snapshot was skipped) |
| head_oid | string | OID of HEAD at snapshot time |
| head_branch | string \| null | Branch name at snapshot time (null for detached HEAD) |
| consumed | bool | True after the entry has been successfully restored |
| skipped_files | array | Files that were logged but not preserved (too large or denied extension) |

---

# Missing & Relocated Projects

When a registered project's folder is no longer available on disk — deleted, moved, on a drive
  that's offline — Arbor keeps the tab visible in tombstone state instead of silently dropping it.
  You decide what happens next: locate the new folder, retry, or remove the project from Arbor.

## How it works

At workspace restore time and on every "Open project" attempt, Arbor classifies the path into
one of four states:

| Status | Meaning | Typical cause |
| --- | --- | --- |
| ok | Path exists and is a valid git repo | Normal case |
| missing | Path doesn't exist, but at least one ancestor does | Folder deleted or moved |
| unreachable | Neither the path nor any ancestor can be stat-ed | Drive unmounted, network share offline, VPN disconnected |
| not_a_repo | Path exists but is not a git repo | .git/ deleted or repo moved out |

Anything other than `ok` places the tab into **tombstone** state — the tab still appears in the
title bar with a warning glyph, and clicking it opens the locate UI instead of trying to read git2.

## The tombstone screen

When a tombstoned tab is active, the main area shows the missing-project panel. Available actions:

- **Locate folder…**Pick the new on-disk location for the project. Arbor validates it as a git repo, updates the registry, refreshes recents, and reopens the tab in place.
- **Retry**Re-classify the original path. Useful after remounting a drive or reconnecting to a VPN.
- **Remove from Arbor**Deregister the project: removes it from every workspace and clears its registry entry. The folder on disk is never touched.

> **Re-validate on focus** By default, Arbor re-classifies every tombstoned tab when the window regains focus, so a tab can return to a normal repo automatically once you remount the drive. You can turn this off in **Settings → Git → Missing Projects**.

## Recent projects (Welcome screen)

The "Recent" and workspace-repo lists on the welcome screen are validated in parallel on load.
Missing entries are shown with:

- A warning glyph and strikethrough name
- A `missing` badge
- Inline **Locate** and **Remove** buttons (recents) or just **Locate** (workspace repos)

Clicking a missing row never tries to open it — it goes straight to the locate picker.
You can also bulk-clean every dead recent in **Settings → Git → Missing Projects → Clean up missing recents**.

## Settings

| Setting | Default | Effect |
| --- | --- | --- |
| auto_prune_recents | off | Silently drop missing entries from the Recent list at load time. When off, they're shown with the missing badge so you can act per-entry. |
| confirm_before_remove | on | Require a second click on the tombstone screen's "Remove" button before deregistering. |
| revalidate_on_focus | on | Re-classify tombstoned tabs whenever the app regains focus. |

## Plugin hooks

Two hooks bracket the tombstone lifecycle. Both fire with a single context table.

- `on_project_missing`Fired when a registered repo's path fails validation at open time. Plugins should drop transient state tied to that project (cancel jobs, hide pinned views) but should NOT delete persistent caches — the user might recover the path.
- `on_project_relocated`Fired after the user picks a new location via the Locate flow. Plugins keyed off the absolute path (deps caches, IDE history, …) should rebase their bookkeeping from `old_path` to `new_path`.

### Context tables

```
-- on_project_missing
{
  repo_id = "uuid…",
  path    = "/old/path",
  name    = "myrepo",       -- nil if no longer in registry
  reason  = "missing" | "unreachable" | "not_a_repo",
}

-- on_project_relocated
{
  repo_id    = "uuid…",
  old_path   = "/old/path",
  new_path   = "/new/path",
  name       = "myrepo",
  remote_url = "git@…" or nil,
}
```

### Example handler

```
arbor.events.on("on_project_relocated", function(ctx)
  -- Rewrite our path-keyed cache
  local cache = arbor.settings.global.get("path_cache") or {}
  if cache[ctx.old_path] then
    cache[ctx.new_path] = cache[ctx.old_path]
    cache[ctx.old_path] = nil
    arbor.settings.global.set("path_cache", cache)
  end
end)

arbor.events.on("on_project_missing", function(ctx)
  arbor.log.warn("project missing: " .. ctx.path .. " (" .. ctx.reason .. ")")
end)
```

> ℹ Both hooks fire from the backend with the same dispatch pipeline as `on_repo_open` / `on_repo_close`, so anything you can do from those handlers works here.

## Distinguishing missing from drive-offline

The `reason` field lets plugins behave differently for "drive disconnected" vs.
"folder deleted":

- `missing` usually means the user moved the folder. A plugin might choose to remove its persistent state for that project, since the path is unlikely to come back.
- `unreachable` usually means the user is on a flaky network. Plugins should keep state and let the next focus revalidation pick up where they left off.
- `not_a_repo` means the directory is still there but the `.git/` is gone. The user may be restoring from backup; treat it like `missing` but more transient.

---

# Git Executable

Arbor uses libgit2 (via the `git2` crate) for most operations, but a handful of commands —
  rebase, stash, submodule update, recovery snapshots, fast-forward / non-FF merges — still shell out to
  the system `git` binary. This page covers how Arbor finds that binary, how to override it,
  and how to install one when you don't have it.

## Detection order

At startup (and again whenever you click **Re-detect** in Settings), Arbor resolves the path
in this order:

1. **Override path**The `executable_path` set under `[git]` in `~/.config/arbor/config.toml`. Set via **Settings → Git → Git Executable → Browse**.
2. **System `PATH`**The first `git` (or `git.exe` on Windows)
    found by walking the directories in your `PATH` environment variable.
3. **Bundled portable copy**`~/.config/arbor/git/cmd/git.exe` on Windows
    (populated by the in-app downloader). Skipped on macOS / Linux.

The **Settings → Git → Git Executable** page shows which of the three is currently active
via the *source* pill (`config`, `path`, or `portable`).

## First-launch flow

If detection turns up nothing on first launch, Arbor opens a blocking **Git Setup** modal
that you can't dismiss until the path is resolved. Three actions:

- **Download portable git (Windows only)**Grabs the latest PortableGit from `git-for-windows/git` on GitHub and unpacks it into Arbor's config folder. Around 50 MB; progress
    streams into the modal.
- **Browse for git executable…**Pick the `git` binary you want to use. Arbor
    runs `--version` against it before saving — bad paths are rejected.
- **Auto-detect**Re-scan PATH and the bundled copy. Useful when you installed git
    while Arbor was already open.

> **Why a blocking modal?** Without git, anything that depends on the CLI (rebase, stash apply, submodule update) silently fails. Forcing the user to resolve the path up-front prevents confusing partial-functionality states.

## Installing git on macOS / Linux

The in-app download is Windows-only because there's no clean cross-distro portable build of git for
Unix. Use your package manager:

| Platform | Command |
| --- | --- |
| macOS | brew install git |
| Debian / Ubuntu | sudo apt install git |
| Fedora / RHEL | sudo dnf install git |
| Arch | sudo pacman -S git |

Then click **Auto-detect** in the modal (or in **Settings → Git → Git Executable**).

## Switching between several installs

If you keep multiple git versions around (e.g. system git for daily use, a custom build for testing
a patch), use **Browse** to pin Arbor to a specific path. The *Clear override* button
falls back to PATH / portable lookup without touching the on-disk binary.

## Downloaded portable copy

On Windows, the **Download portable** button writes to `%APPDATA%\arbor\git\`. The directory contains a full PortableGit tree
(`cmd/`, `bin/`, `etc/`, …). To remove it, delete the folder — Arbor
will fall back to PATH on next launch.

> **Updating the portable copy** Re-running **Download portable** from Settings overwrites the existing extraction with the latest release. The active path is repointed automatically.

## Authentication

When Arbor shells out to git for clone, ls-remote, submodule fetch/pull/push,
  or the post-fetch step of an MR conflict resolution, it injects the OAuth
  token (or PAT) you saved under **Settings → Authentication** as a
  host-scoped HTTP header:

```
git -c http.https://github.com/.extraHeader="Authorization: Bearer …" \
    -c http.https://github.com/.helper= \
    clone https://github.com/owner/repo.git
```

The `helper=` override clears the OS-level credential chain *only for that host* so a partially-stored Git Credential Manager
  entry can't conflict with Arbor's token.  Hosts Arbor doesn't have a token
  for fall back to the normal git behaviour: SSH keys via `~/.ssh` / ssh-agent for `git@host:` URLs, and GCM /
  netrc / system helper for HTTPS URLs.

In practice this means:

- **Authenticated via Arbor**Just works.  Clone, submodule fetch, and conflict-resolution fetch all use your saved Arbor credentials.
- **Authenticated only via SSH**Use `git@host:owner/repo` URLs.  Arbor doesn't touch those — they go straight to ssh-agent.
- **Authenticated only via GCM / netrc**Continue to work for any host Arbor doesn't have a token for.  When Arbor does have a token, it wins for that host — refresh or remove it from Settings → Authentication if you'd rather defer to the OS.

> **libgit2 vs CLI.** Network operations done through libgit2 (the main repo's *fetch* and *push*) have always used Arbor's stored credentials. This page is about the CLI shell-outs, which historically deferred to the OS helper — they now align with libgit2's behaviour.

## Plugins should not shell out to git

> **For plugin authors.** `arbor.terminal.exec("git ...")` uses the system `PATH`, NOT the binary configured here. That means a plugin that shells out to `git` directly will silently bypass the user's choice — it can run a different version, miss the bundled portable copy entirely, or fail on machines where Arbor's PortableGit is the only git available. Use the built-in APIs instead (`arbor.repo.fetch_active_tab`, `arbor.repo.clone`, …). If the operation you need isn't exposed, file an issue rather than working around it with a raw shell call — Arbor doesn't auto-rewrite plugin commands by design, since that would change their semantics behind the author's back.

## Config file

The override is stored in `~/.config/arbor/config.toml`:

```
[git]
executable_path = "C:/Tools/Git/cmd/git.exe"
```

Setting `executable_path` to an empty string or removing the key falls back to the
detection chain.

---

# Git Bisect

**Git Bisect** uses binary search to find the exact commit that introduced a bug.
  You tell Arbor which commit is bad and which is good — the bisect engine narrows the range in *O(log n)* steps until it pinpoints the culprit.

> Arbor runs bisect in **no-checkout mode** — your working tree is never touched. Mark commits based on knowledge or history, and use the *Checkout* button only when you actually need to run tests against a specific commit.

## Starting a session

1. Right-click the commit you know is **bad** in the graph → *Bisect — Mark as Bad*.
2. A banner appears at the top of the graph asking you to select a good commit.
3. Right-click any commit you know was **good** → *Bisect — Mark as Good*.
4. Arbor computes the midpoint and the banner updates to show the next commit to test.

## The bisect banner

The banner changes appearance based on the current state:

| State | What you see |
| --- | --- |
| Waiting for good | Gray banner — "right-click a known good commit in the graph". No midpoint is shown yet. |
| Midpoint ready | Accent banner — shows the next commit hash and approximate remaining steps. Action buttons: Checkout, Good, Bad, Skip, Undo, Save & Pause, Reset. |
| Result found | Red banner — "First bad commit found" with the culprit hash (click to scroll to it in the graph). The session is auto-saved. |

## Action buttons

- **Checkout** — Switches your working tree to the current midpoint so you can run tests. Optional — skip it if you can judge the commit from its diff or history.
- **Good / Bad** — Mark the current midpoint. The graph scrolls automatically to the next commit to test.
- **Skip** — Skip a commit you cannot test (e.g. broken build). Available only after a good commit has been selected.
- **Undo** — Reverts the last mark by replaying the bisect log without the final command. Available as long as there is at least one mark.
- **Save & Pause** — Saves the session to .arbor/bisect/ and resets git bisect so you can do other work. Resume at any time from the sidebar.
- **Reset** — Ends the current bisect session without saving. Git restores the original HEAD.

## Graph indicators

Commits involved in the bisect session are highlighted with colored rings in the graph:

| Ring | Meaning |
| --- | --- |
| ■ Red solid | Marked as Bad. All bad commits keep their ring throughout the session. |
| ■ Green solid | Marked as Good. |
| ■ Orange dashed (pulsing) | Current midpoint — next commit to test. |
| ■ Red double-glow (pulsing) | Result — the first bad commit found. |

## Bisect sessions

Sessions are stored under `.arbor/bisect/<id>/session.json` inside your repository.
  The **Bisect Sessions** collapsible appears in the sidebar whenever at least one session exists.

| Action | Description |
| --- | --- |
| ▶ Play | Replays all marks from the session. For paused sessions this restores the midpoint and scrolls to it. For completed sessions it reloads the result state and rings into the graph. |
| ⌖ Go to result | Scrolls the graph to the result commit (completed sessions only). |
| ✎ Rename | Click the pencil icon and type a new name. Press Enter or click away to confirm, Escape to cancel. |
| ✕ Delete | Removes the session directory permanently. |

> **Auto-save on result** — when bisect finds the culprit commit, the session is saved automatically with a name like *"Found: abc1234 — commit message"*. You never lose a completed bisect result.

## Under the hood

The backend runs `git bisect start --no-checkout` and manages `BISECT_HEAD` directly. State is read from `.git/BISECT_LOG` and `.git/BISECT_HEAD`:

| File | Content |
| --- | --- |
| .git/BISECT_HEAD | Current midpoint OID (set by git after range is established) |
| .git/BISECT_LOG | Ordered list of git bisect good/bad/skip commands — parsed to reconstruct all marks |
| .arbor/bisect/<id>/session.json | Persisted session: id, name, status, bad/good hashes, result, timestamps |

---

# Marketplace

The Marketplace is a one-click browser for plugins and themes hosted in the `arbor-extensions` registry on GitHub, plus any custom git URL you choose
  to add. Open it from the **Browse** button at the top of the Plugin
  Manager.

## How it works

Arbor never bundles plugin metadata in the binary. The Marketplace fetches a
  small `index.json` pointer file from `github.com/nightprint-studio/arbor-extensions`, then resolves each
  entry's `plugin.toml` directly from the source repo. This way:

- Authors update one file (their own `plugin.toml`) — never the registry.
- The registry stays a tiny list of pointers — easy to PR-review.
- Icons, docs, screenshots come straight from the repo, always in sync with the code.

## Installing a plugin

1. Click a row in the catalog to open its detail pane.
2. Review the requested permissions in the body.
3. Hit **Install**. A confirmation modal lists the same permissions in
      human-readable form — read carefully and confirm.
4. Arbor downloads the GitHub zipball, extracts it to `~/.config/arbor/marketplace_plugins/<name>/`, and reloads the plugin
      host. The plugin lands *disabled by default*.
5. Toggle **Enabled** in the detail pane when you're ready to use it.

Plugins installed through the Marketplace get a small **Marketplace** badge in the Plugin Manager so they're visually distinct from dev / hand-copied
  plugins. The two pools live in separate directories and never collide — if a name
  collision happens, the dev plugin wins and the marketplace shadow is logged + skipped.

## Custom sources

Click **Add custom source** in the modal footer to point Arbor at any
  GitHub repo. The resolver detects the layout automatically:

1. If a `subpath` is supplied → fetches `<subpath>/plugin.toml` (subpath mode).
2. Else, looks for `plugin.toml` at the repo root → single plugin (root mode).
3. Else, looks for an `index.json` at the root → multi-plugin registry (mirror mode).

Custom sources are persisted in `~/.config/arbor/user_registry.toml` and
  re-resolved every time the catalog refreshes. Installed plugins from a custom source
  survive removing the source — they're tracked independently in the install ledger.

## Updates

When the catalog version is newer than the installed version, the row shows a
  yellow **Update** pill and the detail header switches to `v1.2 → v1.3`. The **Update to v…** button re-runs the
  install path (overwrites the existing folder + reloads the host). You're shown the
  permission confirmation again in case the new version asks for more.

## Auto-refresh scheduler

Arbor runs a small background task that polls the marketplace cache and re-fetches
  when it ages past your configured interval. Tune it from **Settings → Tools → Marketplace**:

- **Enable scheduler** — master switch. When off, the catalog only
      refreshes when you hit the **Refresh** button in the modal footer
      (or use *Settings → Tools → Marketplace → Refresh now*).
- **Refresh interval** — 1h to 7d. How long the cache may go without
      a refresh before the scheduler re-fetches.
- **Poll cadence** — 1 to 60 minutes. How often the scheduler wakes
      up to check the cache age. 10 minutes is the sensible default — finer values
      just burn cycles checking a multi-hour interval, larger values lag behind
      settings changes.

The fetch itself hits `raw.githubusercontent.com` for a handful of small
  files (typically <200 KB total). Even hourly refreshes are negligible bandwidth.

## Files on disk

- `~/.config/arbor/marketplace_plugins/<name>/` — extracted plugin folders.
- `~/.config/arbor/marketplace_installed.json` — install ledger
      (name, version, repo, ref, resolved SHA, install path, enabled state).
- `~/.config/arbor/marketplace_cache.json` — last-fetched community
      catalog. 1h TTL by default.
- `~/.config/arbor/marketplace_custom.json` — last-resolved custom
      sources. Refreshed on each network fetch.
- `~/.config/arbor/user_registry.toml` — your custom source pointers.
- `~/.config/arbor/themes/<id>.json` — installed marketplace themes
      (same dir the host's theme picker reads).

Dev builds use `-dev` suffixes (e.g. `marketplace_plugins-dev/`)
  so a side-by-side prod Arbor's data stays untouched.

---

# Terminal

Arbor includes a built-in multi-tab terminal emulator powered by **xterm.js** and native PTY — ConPTY on Windows, POSIX PTY on Linux/macOS. No window-switching required.

## Opening the terminal

| Action | How |
| --- | --- |
| Toggle terminal panel | Ctrl+` or the terminal icon in the Activity Bar |
| Open a new tab immediately | Ctrl+Shift+` |
| New tab in default shell | Click + in the terminal tab bar |
| Pick a shell from the list | Click the ▾ dropdown next to + |
| Close tab | Click × on the tab, or type exit in the shell |

## Shell picker

The **▾** dropdown lists every shell that is actually installed and usable on this
  machine, plus any custom terminals you have defined. Shells that aren't found are hidden — you
  won't see `zsh` on a fresh Windows install, and you won't see `cmd` on Linux.

Custom terminals are tagged with a *custom* badge. The footer of the dropdown links straight
  to **Settings → Terminals** for adding more or tweaking detection.

## Built-in shell catalogue

Arbor probes for the following shells at startup. Anything missing from `PATH` is also checked at well-known install locations (e.g. Git Bash under `C:\Program Files\Git`).

| Shell | Default executable | Platform |
| --- | --- | --- |
| Command Prompt | cmd.exe | Windows |
| Windows PowerShell | powershell.exe | Windows |
| PowerShell 7+ | pwsh | Any |
| Bash | bash | Any |
| Git Bash | bash.exe (Git for Windows) | Windows |
| WSL | wsl.exe | Windows |
| MSYS2 | msys2_shell.cmd | Windows |
| Cygwin | Cygwin.bat | Windows |
| Zsh | zsh | Linux / macOS |
| Fish | fish | Any |
| Nushell | nu | Any |
| Xonsh | xonsh | Any |
| Elvish | elvish | Any |
| tcsh | tcsh | Linux / macOS |
| sh | sh | Linux / macOS |

Anything not in this list can still be reached as a **custom terminal** (see **Settings → Terminals**).

## Configuring shells

Open **Settings → Terminals** (under the *Project* group) to:

- Pick the **default shell** opened by the bare **+** button.
- Override the **executable path** for any built-in shell — useful when the binary isn't on `PATH` or you want to pin a specific install.
- Add **custom terminals** (any executable + arguments) such as `nu --no-config`, a containerised dev shell, or a remote SSH helper.
- Re-run **shell detection** after installing a new shell — the picker updates without restarting Arbor.

Settings are stored under `[terminals]` in `~/.config/arbor/config.toml`.

## Features

- **Full colour support** — 256-colour and true-colour ANSI sequences rendered correctly. Works with tools like bat, lazygit, and rich terminal UIs.
- **Repo-aware tabs** — Each tab opens in the working directory of the active repository. A small badge in the tab shows the project name.
- **Auto-close on exit** — The tab closes automatically ~400 ms after the shell process ends — no need to manually close finished sessions.
- **Clickable URLs** — Links in terminal output open in the default browser. Works with http://, https://, and file paths.
- **5 000-line scrollback** — Per-tab scrollback buffer — enough for most build outputs and test runs.
- **Dynamic resize** — The terminal reflows automatically when you drag the panel divider. The panel height persists across sessions.

## Resizing the panel

Drag the divider between the commit graph and the terminal panel to resize. Height is saved in `localStorage` and restored on next launch.

---

# Command Palette

The Command Palette (`Ctrl+K`) is a strictly **verb-first** launcher: you always pick an action first, then (when the action takes a target) refine to a specific branch / tag / commit / file. Ambiguity is removed by design â€” the palette always shows what will happen on `Enter`.

## Opening & navigating

| Key | Action |
| --- | --- |
| Ctrl+K | Open / close the palette |
| â†‘ / â†“ | Move selection up / down |
| Enter | Pick the highlighted command (Phase 1) or run it on the highlighted target (Phase 2) |
| Tab | Accept ghost-text autocompletion |
| Backspace | On empty input, in Phase 2: remove the verb chip and go back to Phase 1 |
| Esc | Close the palette |

## Two phases: pick a command, then a target

The palette is a two-step flow. In **Phase 1** you autocomplete a command; in **Phase 2** the command becomes a chip at the left of the input, and the list filters to the targets for that command.

### Phase 1 â€” Commands

With an empty input the list shows every runnable command, grouped by category. Verbs (which open a target picker) always come first; leaf actions follow, grouped by area:

- **Branch** â€” *Checkout*, *Merge*, *Delete Branch*, *Rename Branch*, *Push Branch*, *Focus Branch in Graph*
- **Navigate** â€” *Go to Commit*, *Go to Tag*, *Blame File*, *Show Commits Touching File*
- **Commit** â€” *Cherry-pick*, *Revert Commit*, *Reset Soft / Mixed / Hard*, *Create Branch Here*, *Create Tag* (Enter on empty input tags HEAD), *Copy Commit SHA*
- **Stash** â€” *Apply Stash*, *Pop Stash*, *Drop Stash*
- **Tag** â€” *Delete Tag*, *Push Tag*
- **Remote** â€” *Fetch from Remote*, *Pull from Remote*, *Push Branch to Remote*
- **Tabs** â€” *Switch Tab*, *Close Tab*
- **Repository** â€” *Open Recent Repository*
- **Merge Requests** â€” *Open Pull / Merge Request* (opens the create MR/PR modal)
- **Appearance** â€” *Switch Theme*
- **Repository actions (leaves)** â€” Open / Init / Clone / Reload Repository
- **Workspaces** â€” *Switch Workspace*, *Open Project*, *Open from Workspace*, Manage Workspaces, Create Workspace
- **Worktrees** â€” *Worktree Info*, *Switch Worktree*
- **Deep Links** â€” *Copy arbor:// Link to Commit / Checkout Branch / Branch Worktree / MR* (the *Open Repository* link is a leaf action under **Copy**)
- **Linked Worktrees** â€” Manage Linked Worktrees, Link this Worktreeâ€¦, Unlink from "<link>", Enable / Disable Sync for "<link>" (latter four shown only when applicable to the current repo)
- **Tabs (leaves)** â€” Close Current Tab, Next / Previous Tab
- **Git (leaves)** â€” Pull, Push, Fetch All Remotes, New Branch, Stash Changes
- **Stage & Commit** â€” Commit, Amend Last Commit, Stage All, Unstage All, Discard All, Undo Last Commit
- **Rebase / Merge** â€” Continue / Skip / Abort Rebase, Abort Merge (visible only while the repo is in that state)
- **Panels** â€” Toggle Stage / Detail / Terminal / Jobs / Notifications / Sidebar; Show Branches / Git Flow / MRs / Issues / Files / Reflog / Stats / Pipelines
- **Copy** â€” Copy Current Branch Name, Copy Current SHA, Copy `origin` URL, *Copy arbor:// Link to Open Repository*
- **System** â€” Settings, Plugin Manager, Reload Plugins, Documentation, About Arbor
- **Submodules** â€” Update All Submodules
- **Navigation** â€” Jump to HEAD, Open in IDE
- **Open With** â€” one entry per detected / custom IDE (only when a repo is open)
- **Plugin Commands** â€” registered via `arbor.command.register()`

Verb commands show a `â€º` chevron on the right to indicate they open a target picker. Leaf commands execute immediately. Conditional leaves (e.g. *Continue Rebase*, *Unstage All*) only show when the action is applicable.

### Phase 2 â€” Target picker

Selecting a verb inserts a coloured chip at the start of the input (e.g. `âŒ¥ Checkout â€º`) and the list becomes the verb's target set. The input placeholder flips to match the verb's target â€” *"Filter branchesâ€¦"*, *"Filter stashesâ€¦"*, *"Filter remotesâ€¦"*, etc. `Enter` runs the verb on the highlighted row; clicking the chip (or `Backspace` on empty input) removes it and returns to Phase 1.

Target kinds: `branch`, `tag`, `commit`, `file`, `stash`, `remote`, `tab`, `recent` (repository), `mr`, `theme`, `worktree`.

## Command reference

### Branch verbs

| Command | Aliases | What it does |
| --- | --- | --- |
| Checkout | co, switch, sw, ck | Checks out the branch; opens the conflict modal if the workdir is dirty |
| Merge | â€” | Merges the branch into HEAD |
| Delete Branch | del, rm, delb | Removes the local branch (with confirm) |
| Rename Branch | ren, mv | Opens the branch-rename modal with remote-rename toggle |
| Push Branch | pushb | Pushes refs/heads/<branch> to origin |
| Focus Branch in Graph | focus, goto, go, show | Centers the graph on the branch HEAD |

### Navigation & Commit verbs

| Command | Aliases | Target | What it does |
| --- | --- | --- | --- |
| Go to Tag | tag, tags | tag | Centers the graph on the tag target |
| Go to Commit | commit, commits | commit | Full-text commit search (summary, author, hash) â€” min. 2 characters |
| Blame File | blame, annotate | project-file | Opens the Git Blame modal for any file in the project â€” does not touch the File Tree sidebar |
| Show Commits Touching File | file-history, log-file, history | project-file | Filters the graph by a file picked from the full project â€” does not open the File Tree sidebar |
| Cherry-pick | cp, pick | commit | Applies the commit onto HEAD; routes to Stage on conflicts |
| Revert Commit | rv | commit | Creates a new commit that undoes the target |
| Reset Soft | rs | commit | Move HEAD only; keep index and workdir |
| Reset Mixed | â€” | commit | Move HEAD + reset index; keep workdir |
| Reset Hard | rh | commit | Destructive â€” requires confirmation. Resets HEAD, index and workdir |
| Create Branch Here | bf | commit | Opens the new-branch modal pre-seeded at the commit |
| Create Tag | th, tag-here, create-tag | commit | Top entry here (selected by default â€” Enter creates a tag at HEAD); type a commit term to pre-seed the modal elsewhere |
| Copy Commit SHA | sha | commit | Copies the full OID to the clipboard |

### Stash / Tag / Remote verbs

| Command | Aliases | Target | What it does |
| --- | --- | --- | --- |
| Apply Stash | apply | stash | Applies a stash without dropping it |
| Pop Stash | pop | stash | Applies and drops the stash |
| Drop Stash | drop | stash | Deletes the stash (with confirm) |
| Delete Tag | delt, rmt | tag | Removes the local tag (with confirm) |
| Push Tag | pusht | tag | Pushes refs/tags/<name> to origin |
| Fetch from Remote | fr | remote | Fetches refs from a specific remote |
| Pull from Remote | pr | remote | Pulls current branch from the chosen remote |
| Push Branch to Remote | ptr | remote | Pushes the current branch to the chosen remote |

### Tabs / Repository / Theme verbs

| Command | Aliases | Target | What it does |
| --- | --- | --- | --- |
| Switch Tab | tab | tab | Activates the selected repo tab |
| Close Tab | closet | tab | Closes the selected repo tab |
| Open Recent Repository | recent, open | recent | Opens one of the recently-used repositories in a new tab |
| Switch Theme | theme, colors | theme | Applies a built-in or custom theme (persists across restarts) |

### Worktree verbs

| Command | Aliases | Target | What it does |
| --- | --- | --- | --- |
| Worktree Info | wt, wtinfo, worktree | worktree | Opens the info panel for any worktree of the active project â€” same modal as the sidebar list, but reachable without expanding the Worktrees section |
| Switch Worktree | wts, switch-wt | worktree | Swaps the active tab's context to the chosen worktree (or focuses an existing tab on that path) â€” same logic as double-clicking a row in the sidebar |

Both verbs lazy-load the worktree list the first time they activate, then cache it for the lifetime of the palette open.

### Deep Link verbs

Build a shareable `arbor://` URL and copy it to the clipboard. The active tab's first remote is embedded as `?url=`, so the link resolves on any machine that has access to the same remote. If the repository has no remote configured, the palette toasts a warning rather than producing a non-shareable link â€” see the *Deep Links* doc page for the full URL schema.

| Command | Aliases | Target | Produces |
| --- | --- | --- | --- |
| Copy arbor:// Link to Commit | linkc, dl-commit | commit | arbor://commit/<sha>?url=<remote> |
| Copy arbor:// Link to Checkout Branch | linkb, dl-checkout | branch | arbor://branch/<name>?url=<remote>&checkout=1 |
| Copy arbor:// Link to Branch Worktree | linkw, dl-worktree | branch | arbor://branch/<name>?url=<remote>&worktree=1 |
| Copy arbor:// Link to MR | linkmr, dl-mr | mr | arbor://mr/open/<number>?url=<remote> |

The *Open Repository* variant has no target, so it lives as a leaf entry under **Copy** (*Copy arbor:// Link to Open Repository*) and produces `arbor://repo/open?url=<remote>`.

## Auto-promote shortcut

Typing a verb name (or any alias) followed by a space â€” or a colon â€” promotes it to a chip immediately and keeps whatever you typed after as the target filter. This lets power users skip the list entirely:

- `co main` â†’ chip `Checkout`, filter *main*
- `merge develop` â†’ chip `Merge`, filter *develop*
- `tag:v1` â†’ chip `Go to Tag`, filter *v1*
- `rm feature/old` â†’ chip `Delete Branch`, filter *feature/old*
- `cp fix` â†’ chip `Cherry-pick`, filter commits containing *fix*
- `apply WIP` â†’ chip `Apply Stash`, filter stashes containing *WIP*
- `tab:docs` â†’ chip `Switch Tab`, filter tabs whose name contains *docs*
- `wt feature/api` â†’ chip `Worktree Info`, filter worktrees whose branch contains *feature/api*
- `linkc bug-fix` â†’ chip `Copy arbor:// Link to Commit`, filter commits matching *bug-fix*

The verb chip is always visible, so there is no hidden state: the palette shows exactly what `Enter` will do.

## Destructive actions & confirmations

A handful of commands require explicit confirmation because they cannot be undone or affect stashed work:

- *Delete Branch*, *Delete Tag*, *Drop Stash* â€” native `confirm()` prompt
- *Reset Hard* â€” lists the target SHA in the prompt
- *Discard All Changes* â€” same
- *Undo Last Commit* â€” shows the parent SHA that HEAD will move to

## Open With â€” launching an IDE

The **Open With** section is populated from your IDE configuration in *Settings â†’ IDE Integration*:

- All built-in IDEs detected at startup (or with a custom *executable path* set) are listed automatically
- Custom IDEs added in settings appear alongside the built-ins
- The IDE is launched **detached** â€” closing Arbor does not close the IDE

For a quick one-click launch with your default IDE, use the *Open in IDE* entry in the **Actions** section. For a specific IDE, pick from **Open With**.

## Ghost-text autocompletion

As you type, the palette shows a dimmed ghost suffix in the input box when the first result title starts with your current query. Press `Tab` to expand the input to the full suggested title, or keep typing to refine.

## Fuzzy scoring

Each item is assigned a score based on how well its title and subtitle match the query:

- Exact match â†’ 100
- Prefix match â†’ 85
- Word-boundary match â†’ 70
- Substring match â†’ 55
- Fuzzy (all characters present in order) â†’ 30
- No match â†’ hidden

Sections with no matching items are hidden entirely.

## Plugin commands

Plugin-registered commands appear in the palette under the **Plugin Commands** section. They fire `command:<id>` on the owning plugin when selected.

Currently registered by enabled plugins:

| Title | Description | Plugin | Action |
| --- | --- | --- | --- |
| Cipher Studio: open… | Encode / decode text with classical ciphers and old-school encodings. | cipher-studio | command:open |
| Cloud Storage · Manage connections… | Open the connections manager (add / edit / delete). | cloud-storage | command:manage-connections |
| Cloud Storage · Sync down (cloud → local) | Mirror a remote prefix onto a local folder. | cloud-storage | command:sync-down |
| Cloud Storage · Sync up (local → cloud) | Mirror a local folder onto a remote prefix. | cloud-storage | command:sync-up |
| Workspace Security Dashboard | Aggregate severity counts, risk and findings across the active workspace. | group-security-dashboard | command:open_active |
| Open JSON / JSONC file in Studio… | Pick a .json or .jsonc file and explore it as a lazy tree (or pretty-printed text) with JSONPath query. Files larger than 1 MB open in stream mode (navigation-only). | json-studio | command:open-file |
| Paste JSON in Studio… | Paste a JSON document and open it in the Studio modal. | json-studio | command:paste |
| Number Studio: open… | Convert integers between numeral systems (bases, Roman, Chinese, Devanagari, …). | number-studio | command:open |
| Open .properties file in Studio… | Pick a .properties file and explore it as a lazy dotted-key tree with JSONPath query, lossless edit and cross-refs. | properties-studio | command:open-file |
| Paste .properties in Studio… | Paste a .properties document and open it in the Studio modal. | properties-studio | command:paste |
| Open RON file in Studio… | Pick a .ron file and explore it as a tree, edit text directly, diff against the original, save in place, or convert to JSON. | ron-studio | command:open-file |
| Paste RON in Studio… | Paste a RON document and open it in the Studio modal. | ron-studio | command:paste |
| Open TOML file in Studio… | Pick a .toml file and explore it as a lazy tree with JSONPath query. Edits are lossless — comments and formatting survive a round-trip. | toml-studio | command:open-file |
| Paste TOML in Studio… | Paste a TOML document and open it in the Studio modal. | toml-studio | command:paste |
| Open YAML file in Studio… | Pick a .yaml / .yml file and explore it as a lazy tree with JSONPath query. Read-only in Phase 5.a. | yaml-studio | command:open-file |
| Paste YAML in Studio… | Paste a YAML document and open it in the Studio modal. | yaml-studio | command:paste |

## Plugin API â€” arbor.command

Plugins can register and remove command palette entries at runtime using `arbor.command.register()` and `arbor.command.unregister()`. Call `register` during `on_plugin_load` so the commands are available as soon as the plugin loads.

```lua
-- Register a command palette entry
-- Fields: id (required), title (required), description?, icon?, group?
arbor.command.register({
  id          = "run-tests",
  title       = "Run Tests",
  description = "Execute the test suite",
  icon        = "Play",          -- Lucide icon name
  group       = "My Plugin",
})

-- Handle execution: the action name is  "command:<id>"
arbor.events.on("command:run-tests", function(_ctx)
  arbor.job.spawn({ id = "tests", cmd = "cargo", args = {"test"} })
end)

-- Remove a command at runtime (e.g. when a feature is unavailable)
arbor.command.unregister("run-tests")
```

### Fields

| Field | Type | Description |
| --- | --- | --- |
| id | string | Unique identifier within the plugin. Used as the action name command:<id>. |
| title | string | Display title shown in the palette. |
| description | string? | Subtitle shown below the title (e.g. short description or plugin name). |
| icon | string? | Lucide icon name (e.g. "Play", "GitBranch", "Settings"). Defaults to "Zap" if omitted. |
| group | string? | Optional category label; currently used for internal grouping only. |

### Hook convention

When the user selects a plugin command the palette fires `fire_plugin_action(plugin_name, "command:<id>", "{}")`. Register the handler with `arbor.events.on("command:<id>", fn)` â€” the same mechanism used for activity-bar actions and keybindings.

### Full example

```lua
-- plugins/my-plugin/main.lua

arbor.events.on("on_plugin_load", function(_ctx)
  arbor.command.register({
    id    = "open-dashboard",
    title = "Open Dashboard",
    icon  = "LayoutPanelLeft",
    group = "My Plugin",
  })
  arbor.command.register({
    id          = "deploy-prod",
    title       = "Deploy to Production",
    description = "Runs deploy.sh on the active repo",
    icon        = "Upload",
    group       = "My Plugin",
  })
end)

arbor.events.on("command:open-dashboard", function(_ctx)
  arbor.notify{ message = "Dashboard opened!", level = "info" }
end)

arbor.events.on("command:deploy-prod", function(_ctx)
  local repo = arbor.repo.current()
  if not repo then
    arbor.notify{ title = "Deploy", message = "No active repository", level = "error" }
    return
  end
  arbor.job.spawn({
    id   = "deploy",
    cmd  = "bash",
    args = { repo .. "/deploy.sh" },
  })
end)
```

---

# Keyboard Shortcuts

Arbor is designed to be fully keyboard-navigable. Most actions have a default shortcut, and every built-in binding is rebindable from Settings → Keybindings.

## Global shortcuts

| Shortcut | Action |
| --- | --- |
| Ctrl+ O | Open repository |
| Ctrl+ Shift+ O | Clone repository |
| Ctrl+ Shift+ I | Initialize repository in folder |
| Ctrl+ R | Recent repos quick-switch |
| Ctrl+ Shift+ R | Browse remote repositories (GitHub / GitLab) |
| Ctrl+ K | Open Command Palette |
| Ctrl+ N | Open project in active workspace |
| Ctrl+ Shift+ N | Open project from another workspace (cross-WS tab) |
| Alt+ Shift+ W | Open Workspace Manager |
| Ctrl+ , | Open Settings |
| F1 | Toggle Documentation |
| Escape | Close current panel / search / modal |

## Tabs & navigation

| Shortcut | Action |
| --- | --- |
| Ctrl+ Tab | Next tab |
| Ctrl+ Shift+ Tab | Previous tab |
| Ctrl+ W | Close active tab |
| Ctrl+ Home | Jump to HEAD commit in graph |
| Ctrl+ F | Search commits (message / author / SHA) |

## Panels

** `Ctrl`+ `B`** / ** `Ctrl`+ `Shift`+ `B`** / ** `Ctrl`+ `J`** are *generic* visibility
  toggles — they collapse whatever section is open or restore the last-used one. The
  numbered Alt+Shift shortcuts below pick a specific section directly (IntelliJ-style).

| Shortcut | Action |
| --- | --- |
| Ctrl+ B | Toggle left sidebar visibility |
| Ctrl+ Shift+ B | Toggle right sidebar visibility |
| Ctrl+ J | Toggle bottom panel visibility |
| Ctrl+ Shift+ S | Toggle Stage area |
| Ctrl+ ` | Toggle terminal panel |
| Ctrl+ Shift+ ` | Open new terminal tab |
| Alt+ Shift+ L | Toggle Plugin Logs console |
| Alt+ Shift+ K | Toggle the Keyboard Inputs overlay (demos, screencasts) — works even inside modals |

## Sidebar Sections

IntelliJ-style numbered tool-window shortcuts. Each shortcut is silently a no-op when the
  matching ActivityBar button has been hidden via **Settings → Customize Activity Bar**.

| Shortcut | Action |
| --- | --- |
| Alt+ Shift+ 1 | Toggle Branches & Stashes |
| Alt+ Shift+ 2 | Toggle File Tree |
| Alt+ Shift+ 3 | Toggle Git Flow |
| Alt+ Shift+ 4 | Toggle Issues (Linear / Jira) |
| Alt+ Shift+ 5 | Toggle Pipelines panel |
| Alt+ Shift+ 6 | Toggle Reflog |
| Alt+ Shift+ 7 | Toggle Repository Statistics |
| Alt+ Shift+ 8 | Toggle Security / Vulnerability Dashboard |
| Ctrl+ Shift+ M | Toggle Pull / Merge Requests |

## Git

| Shortcut | Action |
| --- | --- |
| Ctrl+ Shift+ F | Fetch all remotes |
| F5 | Refresh graph (same as the fetch button in the status bar) |
| Ctrl+ Shift+ L | Pull current branch |
| Ctrl+ Shift+ P | Push current branch |
| Alt+ Shift+ B | Create new branch |
| Ctrl+ Shift+ H | Stash changes |
| Ctrl+ Shift+ A | Stage all changes |
| Ctrl+ Shift+ U | Unstage all changes |

## Stage area

| Shortcut | Action |
| --- | --- |
| Ctrl+ Enter | Commit (when focus is in message field) |

## Diff viewer

| Shortcut | Action |
| --- | --- |
| F3 | Jump to next change chunk |
| Shift+ F3 | Jump to previous change chunk |
| Alt+ 1 | Split view |
| Alt+ 2 | Unified view |

## File / Folder picker

Shortcuts available inside the file/folder picker dialog (Open, Clone destination,
  Save As, plugin file fields, etc.). Most are dialog-scoped — they only fire while
  the picker is open.

| Shortcut | Action |
| --- | --- |
| Ctrl+L | Edit the path directly (address bar) — type with ghost-text autocompletion |
| Tab in address bar | Accept the ghost-text autocomplete suggestion |
| Ctrl+N | Create a new file in the current folder |
| Ctrl+Shift+N | Create a new folder in the current folder |
| Ctrl+ B | Collapse / expand the picker sidebar (same global shortcut) |
| Alt+← / Alt+→ | Back / Forward through navigation history |
| Backspace | Go up one folder |
| ↑ / ↓ | Move selection in the file list |
| F2 | Rename the selected entry |
| Delete | Delete the selected entry (asks for confirmation) |
| Enter | Open folder · open file · confirm pick · confirm delete |
| Type any letter | Type-ahead — keystrokes route into the filter field automatically |
| ↓ in filter field | Jump focus to the first matching entry |

## Context menus

| Target | How to open |
| --- | --- |
| Commit (graph) | Right-click commit row |
| Branch (sidebar) | Right-click branch item |
| File (stage area / diff list) | Right-click file entry |
| Tab (tab bar) | Right-click tab |

## Where shortcuts surface in the UI

Built-in shortcuts are rendered live next to the action wherever it appears:

- **Main menu** (hamburger top-left) — IntelliJ-style right-aligned hint on each row.
- **Command Palette** (`Ctrl+K`) — small `kbd` badge at the right of the row.
- **Right-click context menus** — branch, commit, tab, stage entries.
- **Tooltips** on Activity Bar, Status Bar and TitleBar buttons (e.g. hovering the Fetch button shows *Fetch from remote (Ctrl+Shift+F)*).

All bindings flow from a single source of truth, so a remap in **Settings → Keybindings** updates every hint in place — no restart required.

## Customizing shortcuts

All built-in shortcuts are rebindable via **Settings → Keybindings**. Click any shortcut chip to record a new key combination; press `Escape` while recording to cancel. A reset icon appears next to modified bindings.

## Plugin shortcuts

Plugins can register their own keybindings using `arbor.keybinding.register()`. Plugin shortcuts also appear in a read-only **Plugins** section at the bottom of Settings → Keybindings. They fire the associated Lua action directly and take priority if no built-in binding is mapped to the same combination.

Currently registered by enabled plugins:

| Shortcut | Action | Plugin |
| --- | --- | --- |
| Ctrl+ F9 | Build selected configuration | compile-action |
| Shift+ F9 | Debug selected application configuration | run-action |
| Ctrl+ Shift+ F9 | Debug Tomcat without building (catalina + JPDA) | run-action |
| Shift+ F10 | Run selected application configuration | run-action |
| Ctrl+ Shift+ F10 | Start Tomcat without building (no debug) | run-action |
| Ctrl+ Shift+ E | Source Export: edit configurations | source-export |

---

# Repository Statistics

Arbor can compute a detailed statistical profile of any open repository —
  commit activity, contributor breakdown, file hotspots, and more.
  All computation runs in a background thread so the UI stays responsive.

## Opening Statistics

- Click the **Bar Chart** icon in the Activity Bar (left rail) to open the **Stats sidebar panel**.
- Click **Full Statistics** at the bottom of the panel to open the full-screen overlay.
- The overlay has three tabs: **Overview**, **Contributors**, **Files**.

## Stats Sidebar Panel

Shows a compact at-a-glance summary while you work:

- **Summary Cards** — Four cards in a 2×2 grid: total commits, contributors, repository age, and active days. Each card has a coloured icon for quick identification.
- **Commits / Week Sparkline** — A 12-week bar chart showing weekly commit frequency. Includes a Y-axis scale (peak → 0) and a timeline (12w ago → now).
- **Top Contributor** — The all-time leader by commit count, with a percentage bar.
- **Highlights** — Four quick highlights: • This week — top author in the last 7 days • This month — top author in the last 30 days • Most lines changed — author with the highest total line churn • Longest streak — consecutive days with at least one commit

## Overview Tab

Summary cards available in the full overlay:

Total Commits

All commits reachable from HEAD.

Contributors

Number of unique author emails.

Repository Age

Time between first and last commit.

Active Days

Calendar days that had at least one commit.

Avg / Week

Commits per calendar week over the project lifetime.

Longest Streak

Longest run of consecutive days with at least one commit.

Avg Commit Size

Average lines changed (insertions + deletions) per commit, sampled from the first 500 commits.

First / Last Commit

Dates of the oldest and newest commits.

Busiest Day

The calendar date with the most commits, with count.

The Overview tab also includes:

- **Commit Activity Heatmap** — GitHub-style 52×7 calendar for the last 12 months. Hover a cell to see the exact count for that day.
- **Commit Timing** — two bar charts: commits by hour of day (0–23) and commits by day of week (Mon–Sun). Both support hover tooltips on every column, even very small bars.

## Contributors Tab

Two ranked lists:

- **By Commits** — top 10 authors by commit count, with avatar initials, percentage bar, and commit count. Avatars use a deterministic hue derived from the author's email.
- **By Lines Changed** — top 10 authors by total lines touched (insertions + deletions), sampled from the first 500 commits. Each row shows `+additions` and `−deletions` colour-coded pills and a two-tone bar split between adds (green) and deletes (red).

## Files Tab

- **By File Type** — top 10 extensions by cumulative change count, shown as coloured horizontal bars.
- **Most Changed Files** — top 20 individual files by change count, sampled from the first 500 commits.

> **Performance note:** File-level and contributor line stats are sampled from the **first 500 commits** for performance. Commit-level stats (totals, contributors by count, timing, heatmap) scan the full history.

## Caching

Results are cached in memory keyed by the current HEAD SHA *and* your exclusion settings.
  The cache is invalidated automatically when you push a new commit or change the exclusion config.
  Click **Recompute** (↻ button in the panel header) to force a fresh calculation.

## Exporting Statistics

The statistics overlay header provides two export buttons — **JSON** and **HTML** — visible whenever stats have been computed.
  Click either button to open a file-picker dialog and choose the output location.

- **JSON Export** — Pretty-printed JSON file mirroring the full RepoStats struct. Includes all numeric arrays (commits by hour/weekday, heatmap), top contributors, top files, and file type breakdown. Useful for scripting or importing into other tools.
- **HTML Report** — A fully self-contained HTML file — no external dependencies. Includes the Arbor logo, inline dark-theme CSS, and inline SVG charts: commit heatmap, hour/weekday distributions, contributor bars (by commits and by lines), and file type breakdown. All charts support hover tooltips (date + count on heatmap cells; label + count on hour and weekday bars). Opens in any browser.

> The export runs as a **background job** so the UI stays responsive. Progress and completion status are visible in the *Jobs* overlay, and a bell notification appears when the export finishes or fails. If statistics have already been computed for the current HEAD, the cached data is used directly — no re-computation is needed.

## Excluding Files from Statistics

Go to **Settings → Project → Statistics** to configure per-repository exclusions.
  Excluded paths are ignored in the file-level charts (Most Changed Files, By File Type) but do not affect commit-level stats.

Extensions

File extensions to ignore — e.g. `ron`, `lock`. Enter without the leading dot.

Folders

Folder prefixes to exclude — e.g. `assets/generated`, `vendor`. All files whose path starts with the prefix are skipped.

Files

Exact file names or relative paths — e.g. `Cargo.lock`, `src/generated/schema.rs`.

Exclusions are stored in `.arbor/config.toml` inside the repository. After saving, click **Recompute** to apply them.

---

# Background Jobs

The Jobs system lets plugins run long-running processes in the background — builds, tests, deploys — without blocking the UI. Output is streamed line by line in real time.

## Job lifecycle

1. Plugin calls `arbor.job.spawn(config)` — a background thread starts the process immediately
2. Each stdout/stderr line fires a Tauri event — the frontend appends it to the job's output buffer in real time
3. When the process exits, the `on_done_action` Lua hook is called and the job status is updated
4. If cancelled by the user, the process is killed (`SIGTERM` on Unix, `taskkill /T` on Windows)

## Status bar badge

While jobs are running, a badge appears in the status bar (right side). Click it to open the Jobs overlay.

- **Spinning ● N** (accent colour) — N jobs are currently running
- **● N** (green dot) — all done, N total since last clear

## Jobs overlay & output panel

- **Jobs Overlay** — Floating panel anchored above the status bar. Lists all jobs with status, elapsed time, and plugin name. Each job has a cancel button and an "open output" button (↗).
- **Job Output Panel** — Read-only terminal-like view docked in the bottom area. Real-time streaming with colour-coded lines (stderr in red, warnings in yellow). Auto-scrolls to latest output; "Jump to latest" pill appears when you scroll up manually.

## Job categories

Pass a `category` string to `arbor.job.spawn()` to group related jobs into collapsible sections in the overlay. Leading/trailing whitespace is trimmed automatically.

- Jobs in the same category are shown under a shared collapsible header.
- The header turns accent-coloured and shows a spinner badge when any job in the group is running.
- Running jobs also display a **LIVE** badge next to their name.
- Jobs without a category are listed below all named groups.

Recommended conventions: `"Builds"` for compilation tasks, `"Services"` for long-running processes (dev servers, application runners).

## Non-cancellable jobs

Some jobs are marked as **non-cancellable** — they are system tasks that Arbor
  manages internally and that must not be interrupted by the user.
  For these jobs the cancel / stop button is hidden in both the Jobs overlay and the output panel.

- They still appear in the overlay and output panel like any other job, with a real-time output stream.
- They can finish naturally (*Completed*) or fail (*Failed*); they are never *Cancelled*.
- Plugin `reload_plugins` skips non-cancellable jobs — they are not affected by plugin reloads.

> **Reserved category: `"system"`** The category `"system"` (case-insensitive) is reserved for Arbor's own internal background jobs. Calling `arbor.job.spawn()` with this category from a plugin raises a Lua error. System jobs are also **automatically dismissed** from the overlay once they complete successfully — they are designed to run silently and leave no trace on a clean exit.

## Hidden jobs

Jobs spawned with `hidden = true` are excluded from the default Jobs overlay and Job Output panel listings,
  and from the status-bar running-job badge. They are intended for jobs owned by a domain-specific panel (for example a
  Services panel that manages long-running app servers like Tomcat) where the host Jobs UI would be redundant.

- The job still runs, streams output, and fires `on_done` hooks normally.
- A **Show hidden** toggle in the Jobs overlay and Job Output panel headers reveals them when needed (for example, to kill a zombie service). The toggle state is shared between both panels and persisted in `localStorage`.
- When the toggle is on, hidden jobs are also counted by the status-bar badge.
- If only hidden jobs exist, the overlay shows a hint instead of the empty state.

## Output ring buffer

Each job stores the last **2 000 lines** of output in memory (oldest lines dropped when exceeded) and on disk — so you can view output after reopening the overlay or restarting the app.

> **Background jobs vs. terminal** Background jobs are designed for automated tasks triggered by plugins. For interactive work (running a dev server, using a REPL) use the built-in **Terminal** instead.

## Job sequencing

Jobs can be chained by attaching `:ok` / `:err` on the returned `JobHandle`, by passing an `on_done` sugar callback, or by awaiting inside `arbor.async.run`. Common patterns:

- **Build → run**: spawn the build, then chain `build:ok(function(_) spawn_service() end)`.
- **Queue**: if a build is already running, record the pending run in plugin state; the build's `:ok` starts it when done.
- **Mutual exclusion**: track `active_build_id` in state — reject or queue conflicting jobs.

The compile-action plugin uses all three patterns: pressing **F5** while a build is running queues the run automatically; pressing **F9** while a service is running stops the service first, then builds.

## Plugin API

| Function | Description |
| --- | --- |
| arbor.job.spawn(config) | Launch a background job. Returns (JobHandle, nil) on success or (nil, err) on a spawn-side failure. The handle is a Promise (:ok / :err) with extra .id and :cancel(). Config: name, command, cwd?, env?, category? (string), hidden? (boolean — hide from Jobs panels and badge by default), on_done? (callback — sugar), on_done_action? (hook name — sugar) |
| arbor.job.cancel(job_id) | Kill a running job (SIGTERM / taskkill /T). No-op if already finished. Useful to stop long-running processes (servers, watchers) before re-launching them. |
| arbor.job.list() | Returns a Lua table of all job records with fields: id, name, status, started_at |

See the **Plugin Development** section for full examples.

---

# Notifications

The notification center collects in-app alerts from plugins. Notifications persist until explicitly dismissed — so you never miss a build result or error.

## Bell badge (status bar)

A bell icon in the status bar (right side) shows the current notification count. Click it to open the **Notifications overlay**.

## Notification overlay

A floating panel anchored above the status bar. Each notification shows:

- A coloured left border and icon matching its level (info / success / warning / error)
- **Title** and **message** from the plugin that fired it
- Source plugin name badge
- Relative timestamp (*"2s ago"*, *"5m ago"*…)
- An optional **action button** if the notification was emitted with one — clicking it runs the associated side-effect (e.g. opens the Linked Worktrees manager) and dismisses the notification
- **×** to dismiss the individual notification

The **trash icon** in the header clears all notifications at once.

## Notification actions

Plugins can attach a click action to a notification.  Built-in action kinds:

| Kind | Required fields | Effect |
| --- | --- | --- |
| open-link-manager | label, link_id | Opens the Linked Worktrees manager pre-selected on that link. |
| open-tab-by-repo-id | label, repo_id | Activates the matching open tab; no-op if not currently open. |
| open-url | label, url | Opens the URL in the user's default browser. Use open-path instead for local files (file:// URLs are silently ignored by the opener plugin). |
| open-path | label, path, reveal? | Hands the path to the OS' default handler (folder → Explorer/Finder, file → default editor). Set reveal = true to open the file's parent folder instead — the cross-platform "reveal in Explorer". |
| plugin-action | label, plugin, action, ctx? | Fires arbor.events.on(action, …) in the named plugin with the optional ctx table — round-trip back to a plugin handler from the click. |

## Plugin API

Plugins emit notifications through the table-config form of `arbor.notify`:

```
arbor.notify{
  message = "Build failed (exit 1) — see log",   -- required
  title   = "compile-action",                   -- optional
  level   = "error",                            -- "info"|"success"|"warning"|"error"
  action  = { kind = "open-link-manager", label = "View link", link_id = "abc" },
}
```

See the **Plugin Development** section for the full API reference.

---

# Pipelines â€” Plugin Pipelines

Arbor's pipeline system lets plugins define and run multi-stage CI/CD-style workflows directly inside the app. Each pipeline is a sequence of **stages**, each containing one or more **steps** (shell commands). Progress is shown in a live node graph.

## Opening the Pipelines panel

Click the **Workflow** icon in the Activity Bar (bottom group). Toggle the panel to show/hide it as a resizable bottom section. The two sub-views (**Local Pipelines** â€” plugin-defined â€” and **CI / CD** â€” GitHub Actions / GitLab CI) are inline tabs in the panel header next to the title.

## Panel layout

The Local Pipelines tab is a two-column IntelliJ-style Run window:

- **Left toolbar** (36 px column) â€” global pipeline-level actions:
    a primary **Run** button that re-launches the most recently
    launched pipeline (sticky), then icon-only **Stop all running**, **Resume last failed**, and **Clear history** (terminal runs only). To launch a different pipeline, right-click one
    of its run cards in the list â€” the context menu has a Run entry that
    fires the same routed launch flow. Plugins can contribute additional
    toolbar buttons via the `arbor:pipelines:toolbar` contribution
    point.
- **Right column** â€” a filter row with a multi-select dropdown
    (*All pipelines* by default) and a live run-count summary, then the
    scrollable run list below it. Each card shows status pill, duration, the
    pipeline-definition badge (with an **orphan** tag when the
    def is no longer registered), step count, and a timestamp.

## Running a pipeline

Click the **Run** icon in the left toolbar to replay the most recently launched pipeline. To run a different one, right-click any of its existing run cards in the list and pick **Run â€œâ€¦â€** from the context menu â€” the menu's other entries (Open detail, Cancel, Resume, Discard) mirror the row's hover buttons. The orchestrator spawns a background thread that executes each step sequentially. The node graph updates in real time with status colours:

**Self-contained replay vs. plugin-routed launch.** A
  pipeline def with non-empty `stages` is treated as
  self-contained: every step has its command / op / cwd already resolved
  (variable substitution baked in by whatever flow produced it â€” combo
  button, sequence runner, â€¦), so Play replays it directly via `arbor.pipeline.run` without involving the owning plugin.
  This means a def compiled in a previous tab keeps replaying correctly
  from the panel even after the user switches repos.

A def with empty `stages` is a *stub* the plugin
  registered upfront so the panel has something to show on first open.
  Stubs cannot be replayed verbatim â€” Play asks the owning plugin to
  materialise stages via the `on_pipeline_run_request` hook
  (typically by compiling a profile or resolving a build configuration)
  and the plugin then calls `arbor.pipeline.run` itself. If a
  plugin registers stubs but doesn't implement the hook, Play surfaces a
  clear error pointing the user to the plugin's own launch UI.

| Colour | Meaning |
| --- | --- |
| Green | Success â€” step / stage / run completed with exit code 0 |
| Red | Failed â€” non-zero exit code (pipeline stops unless allow_failure = true) |
| Blue (accent) | Running â€” currently executing |
| Grey | Pending / Cancelled |

## Viewing step output

Click any step node in the graph to expand an output pane at the bottom of the detail area. It shows captured `stdout` and `stderr` (stderr lines are highlighted in red). Up to 1 000 lines are retained per step.

## Cancellation, resume and discard

The cancel/resume/discard affordances live in two places: per-card icon buttons on the right of each run row (cancel for running, resume for failed, trash for terminal), and the bulk equivalents in the left toolbar (**Stop all running**, **Resume last failed**, **Clear history**). All of them are also reachable from Lua via `arbor.pipeline.cancel(run_id)`, `arbor.pipeline.resume(run_id)` and `arbor.pipeline.discard(run_id)`. Cancellation stops the pipeline after the *current step* finishes â€” it does not kill a running process mid-execution.

When a step fails (non-zero exit code, `allow_failure=false`), the
  run enters status `failed` but remains **resumable**:
  its output, log buffer and a `resume_cursor` pointing at the exact
  failing steps are persisted to disk. Call `arbor.pipeline.resume(run_id)` (or use the Resume button in the
  UI) to restart the run from that cursor â€” already-successful steps are
  skipped, only the failed ones are re-executed. A resume requires the
  pipeline's lock to be free.

Use `arbor.pipeline.discard(run_id)` to drop a terminal run
  permanently (removes the persisted JSON file). Discard refuses to act on a `running` run â€” cancel it first.

## Concurrency & locking

Every pipeline has a `lock_key` (default `"<plugin>:<id>"`). Only one run per lock key may be in `running` state at a time â€” a second attempt fails immediately with
  a descriptive log entry. **Terminal runs (failed / cancelled / success)
  do NOT hold the lock**: they remain resumable but a new run of the same
  pipeline can start freely. When another run is active, a resume of an older
  failed run is rejected until the active one finishes.

You can check lock state with `local owner = arbor.pipeline.is_locked(lock_key)` which returns
  the `run_id` currently holding the lock, or `nil` when
  free. Override the default key by passing `lock_key = "..."` to `arbor.pipeline.define` â€” useful when different pipelines compete
  for the same external resource (e.g. a deploy target).

## Parallel steps inside a stage

Stages are always executed **sequentially** (top-to-bottom), but
  inside a stage steps can run in parallel. Set `mode = "parallel"` on the stage and optionally cap concurrency with `max_parallel = N`. All steps of a parallel stage are awaited
  before the next stage starts; an early failure doesn't cancel its siblings
  (GitLab-CI semantics). Resume re-runs only the failing step(s) of a parallel
  stage, leaving the successful ones alone.

## Logging & log level

The orchestrator auto-logs pipeline / stage / step lifecycle events. Each
  run has its own capped log buffer (5 000 entries) plus a live stream via the `arbor://pipeline-log` event. Events are filtered by the run's
  configured `log_level` (default `info`) â€” set `log_level = "debug"` on `arbor.pipeline.define` to
  also capture the per-line step output and resolved parameters. Available
  levels: `debug`, `info`, `warn`, `error`.

## Defining pipelines from a plugin

Two equivalent shapes â€” pick whichever reads better for your case:

- `arbor.pipeline.define(table)` â€” declarative table config (good when you build the pipeline programmatically from data).
- `arbor.pipeline("id"):...:commit()` â€” chainable builder (good for static, hand-written pipelines). Compiles down to the same table on `:commit()`.

### Builder DSL

```lua
arbor.pipeline("build")
  :name("Build & Test")
  :description("Compile, lint and run unit tests")
  :icon("Hammer")
  :lock("my-plugin:build")
  :log_level("info")
  :stage("Prepare")
    :shell("npm install")
  :stage("Verify"):mode("parallel"):max_parallel(2)
    :shell({ id = "lint", name = "Lint", command = "npm run lint", allow_failure = true })
    :shell({ id = "test", name = "Unit tests", command = "npm test" })
  :commit()
```

Builder methods: `:name` Â· `:description` Â· `:icon` Â· `:lock` (alias `:lock_key`) Â· `:log_level` Â· `:stage(name|cfg)` Â· `:mode` Â· `:max_parallel` Â· `:run(op, params)` Â· `:shell(cmd|cfg)` Â· `:step(cfg)` Â· `:commit()`.
  Steps go to the most recently opened stage; `:run` takes `(op_name, params)` or a single `{op, params, plugin?, id?, name?, allow_failure?}` table; `:shell` takes a string or a `{command, cwd?, ...}` table.
  Step ids default to `s1`, `s2`, ... when omitted.

### Table config

Equivalent to the builder above; call from your plugin's `on_plugin_load` handler (or at module level):

```lua
arbor.pipeline.define({
  id          = "build",
  name        = "Build & Test",
  description = "Compile, lint and run unit tests",
  icon        = "ðŸ”¨",
  log_level   = "info",              -- debug | info | warn | error
  lock_key    = "my-plugin:build",   -- optional; default "<plugin>:<id>"
  stages = {
    {
      id   = "prepare",
      name = "Prepare",
      -- mode defaults to "sequential"
      steps = {
        {
          id      = "install",
          name    = "Install dependencies",
          command = "npm install",
        },
      },
    },
    {
      id           = "verify",
      name         = "Verify",
      mode         = "parallel",   -- lint + test run concurrently
      max_parallel = 2,            -- optional cap (omit = unlimited)
      steps = {
        {
          id             = "lint",
          name           = "Lint",
          command        = "npm run lint",
          allow_failure  = true,
        },
        {
          id      = "test",
          name    = "Unit tests",
          command = "npm test",
        },
      },
    },
  },
})
```

## Running a pipeline from Lua

```lua
-- Start a run; returns (run_id, nil) on success or (nil, err) on failure.
local run_id, err = arbor.pipeline.run{ pipeline_id = "build" }

-- Override the working directory for all steps:
local run_id, err = arbor.pipeline.run{ pipeline_id = "build", cwd = "/path/to/project" }

-- Cancel a running pipeline (stops after the current step):
arbor.pipeline.cancel(run_id)

-- Resume a failed run from the steps that halted it.
-- Returns (false, err) when the lock is held by another run.
local ok, err = arbor.pipeline.resume(run_id)

-- Drop a terminal run (removes the persisted JSON file).
local ok, err = arbor.pipeline.discard(run_id)

-- Check who holds the concurrency lock (nil when free).
local owner = arbor.pipeline.is_locked("my-plugin:build")

-- List definitions registered by this plugin:
local defs = arbor.pipeline.list()

-- Look one up by id (scoped to this plugin); returns nil when missing.
-- Useful in re-define paths to inherit the existing display name set by
-- a previous stub registration.
local def = arbor.pipeline.get("build")
if def then arbor.log.info("currently named: " .. def.name) end
```

## Toolbar contribution (arbor:pipelines:toolbar)

Plugins can add extra icon-only buttons to the panel's left toolbar.
  Contribute to `arbor:pipelines:toolbar` with a payload describing
  a single button; the host renders one button per active contribution and
  fires the action when the user clicks it. Use it for plugin-specific
  ops like "Re-run failed steps", "Open Source Export profile", "View dashboard".

```lua
-- main.lua
arbor.contribute("arbor:pipelines:toolbar", {
  payload = {
    icon            = "Zap",          -- lucide icon name
    tooltip         = "Re-run failed steps from the current filter",
    accent          = false,           -- optional: use accent color
    success         = false,           -- optional: green tint
    danger          = false,           -- optional: red tint on hover
    divider_before  = false,           -- optional: 1px separator above the button
    disabled        = false,           -- optional
  },
  action = function(ctx)
    -- Your plugin's logic here. The toolbar is non-modal, so prefer
    -- a notify+job pattern (toasting "Startedâ€¦") over a blocking call.
  end,
})
```

Buttons appear after the built-in Run / Stop / Resume / Clear cluster, in
  registration order. The host swallows errors thrown by the action so a
  buggy plugin can't break the toolbar.

## Pipeline hooks

Declare hooks in `[hooks]` in your `plugin.toml` and register handlers with `arbor.events.on()`:

| Constant | TOML key | Context fields |
| --- | --- | --- |
| "on_pipeline_run_request" | on_pipeline_run_request | pipeline_id, tab_id? â€” fired on the def's owning plugin when the user presses Play on a stub def (empty stages). Defs with non-empty stages are replayed directly without invoking this hook. The handler must compile stages and call arbor.pipeline.run itself. |
| "on_pipeline_started" | on_pipeline_started | run_id, pipeline_id, plugin |
| "on_pipeline_step_done" | on_pipeline_step_done | run_id, pipeline_id, plugin, stage_id, step_id, step_name, status, exit_code |
| "on_pipeline_done" | on_pipeline_done | run_id, pipeline_id, plugin, status |

```lua
-- Map the panel's Play click back into the plugin's own launch flow.
-- The id we registered (e.g. "profile:abc") encodes whatever lookup key
-- the plugin needs.
arbor.events.on("on_pipeline_run_request", function(ctx)
  local def_id = ctx.pipeline_id or ""
  if def_id:sub(1, 8) ~= "profile:" then return end
  local profile = pcfg.find(def_id:sub(9))
  if not profile then return end
  compile.run(profile)        -- materialises stages then arbor.pipeline.run
end)
```

```toml
-- plugin.toml
[hooks]
on_pipeline_started   = true
on_pipeline_step_done = true
on_pipeline_done      = true
```

```lua
arbor.events.on("on_pipeline_done", function(ctx)
  if ctx.status == "success" then
    arbor.notify{ title = "Pipeline done", message = ctx.pipeline_id .. " succeeded", level = "success" }
  else
    arbor.notify{ title = "Pipeline failed", message = ctx.pipeline_id .. " â€” status: " .. ctx.status, level = "error" }
  end
end)
```

## Pipeline options

| Field | Type | Description |
| --- | --- | --- |
| id | string | Unique pipeline identifier within the plugin |
| name | string | Human-readable label |
| description | string? | Tooltip on the Run dropdown entry and the per-card definition badge |
| icon | string? | Emoji or icon identifier |
| lock_key | string? | Concurrency key. Default "<plugin>:<id>" |
| log_level | string? | debug \| info (default) \| warn \| error |
| stages | array | Array of StageDef |

## Stage options

| Field | Type | Description |
| --- | --- | --- |
| id | string | Unique stage identifier within the pipeline |
| name | string | Label |
| mode | string? | sequential (default) \| parallel |
| max_parallel | integer? | Cap concurrency when mode=parallel. Omit = unlimited |
| steps | array | Array of StepDef |

## Step options

A step is one of four **kinds**, picked by which field is set
  (precedence top-to-bottom): `if_block` â†’ `builtin` â†’ `lua_op` â†’ `command`. The
  remaining fields (cwd / env / allow_failure / capture) apply across kinds.

| Field | Type | Description |
| --- | --- | --- |
| id | string | Unique step identifier within the stage |
| name | string | Human-readable label shown in the graph node |
| command | string? | Shell command (run via sh -c / cmd /C). ${var} references are resolved before the process spawns. |
| lua_op | table? | Invoke a plugin-registered Lua handler instead of spawning a shell. Shape: { op = "name", params = {...}, plugin? = "..." }. ${var} in params string fields is resolved before dispatch. |
| builtin | table? | Run a built-in op (file_exists / file_read / env / json_get / path_join / set_var / echo / match). See the dedicated section below. Resolved by the runtime â€” no shell, no Lua VM. |
| if_block | table? | Conditional control step. Evaluates each branch's condition in order and runs the chosen branch's nested steps. See If / elif / else blocks. |
| cwd | string? | Working directory. nil = active repo root. ${var} resolved. |
| env | table? | Extra env vars for shell steps. ${var} resolved per value. |
| allow_failure | bool | If true, the stage continues even if this step fails. Default: false |
| capture | table? | After the step finishes, extract a value from its outcome and store it in the run's variable bag. See Variables & capture. |

## Variables & capture

Every pipeline run owns a typed **variable bag** (empty at
  start). Steps populate it via `capture`; later steps
  reference its values via `${var}` syntax in any string field â€” `command`, `cwd`, `env` values, `lua_op.params`, `builtin` params, and `if_block` conditions all run through the same resolver
  before they execute. `$$` escapes a literal `$`; `${name:-fallback}` supplies a default for missing names.

A `capture` spec has three pieces:

- `var` â€” name to store under (no `$` prefix).
- `source` â€” what part of the step's outcome to capture: `"stdout"` (default), `"stderr"`, `"exit_code"`, `"success"` (boolean: exit_code == 0),
    or `"return_value"` (Lua/builtin's typed return â€” falls back
    to stdout for shell steps).
- `transforms` â€” optional ordered list of *declarative
    transforms* applied left-to-right to massage the captured value
    before storing it.

| Transform | Effect |
| --- | --- |
| { kind="trim" } | Strip leading/trailing whitespace |
| { kind="lower" } Â· { kind="upper" } | ASCII case folding |
| { kind="lines" } | Split a string on \n â†’ list (drops trailing empty lines) |
| { kind="split", sep="," } | Split on a literal separator â†’ list |
| { kind="join", sep=", " } | Join a list with sep â†’ string |
| { kind="first" } Â· { kind="last" } Â· { kind="nth", n=2 } | Index a list (negative n counts from end) |
| { kind="regex", pattern="v(\\d+)", group=1 } | Match a regex; with group returns that captured group |
| { kind="matches_bool", pattern="^OK" } | Same as regex but returns a boolean |
| { kind="json_parse" } Â· { kind="json_get", path="a.b.0" } | Parse a JSON string; walk a dotted path |
| { kind="to_bool" } Â· { kind="to_number" } | Coerce to boolean / number (null on failure) |
| { kind="default", value="N/A" } | Replace empty / null with a fallback string |

Failures inside a transform chain don't fail the step â€” the variable
  becomes `null` and the trace is logged. Use the run log
  panel (debug level) to see each transform's input/output preview.

```lua
-- Capture the first version line emitted by 'mvn -v' and store
-- it as ${maven_version} for later steps.
{
  id      = "detect-mvn",
  name    = "Detect Maven",
  command = "mvn -v",
  capture = {
    var    = "maven_version",
    source = "stdout",
    transforms = {
      { kind = "lines" },
      { kind = "first" },
      { kind = "regex", pattern = "Apache Maven (\\d+\\.\\d+\\.\\d+)", group = 1 },
    },
  },
}

-- Use it in a downstream shell step:
{ id = "log", name = "Log version", command = "echo 'Building with mvn ${maven_version}'" }
```

## Built-in ops

Built-in ops are tiny side-effect-free helpers the runtime executes
  directly â€” no shell, no Lua VM. Use them mostly to seed the variable
  bag (with `capture`) so `if_block` conditions
  and later steps can branch on file presence, environment vars,
  parsed JSON fields, and so on.

| Kind | Fields | Returns |
| --- | --- | --- |
| file_exists | path | bool |
| file_read | path, max_bytes? | string (file contents) |
| env | name, default? | string (env var or default) |
| json_get | source (JSON string), path | typed value at the dotted path |
| path_join | parts (array of strings) | string |
| set_var | value (any JSON) | the value verbatim â€” pair with capture.var |
| echo | message | string (also written to the run log) |
| match | target, pattern? (substring) or regex? | bool |

```lua
-- Capture whether 'docker-compose.yml' exists into a flag.
{
  id      = "check-compose",
  name    = "Detect compose file",
  builtin = { kind = "file_exists", path = "docker-compose.yml" },
  capture = { var = "has_compose", source = "return_value" },
}
```

## If / elif / else blocks

An `if_block` step is a *control step*: instead of
  running a command, the orchestrator evaluates each branch's condition
  in order and runs the chosen branch's nested `steps`. The
  child outcomes appear under the parent step in the run viewer
  (`StepRun.children`) and the picked branch label
  (`"if"`, `"elif #1"`, `"else"`) lands in `StepRun.branch`. The step's overall status is `success` when every chosen child succeeded (honoring `allow_failure`) and `failed` otherwise.

Conditions are **structured values** â€” there's no DSL or
  parser. Each leaf is a small object with a `kind` tag and
  the operands it needs. Operands are `${var}`-resolved before
  comparison.

| Kind | Fields | Notes |
| --- | --- | --- |
| compare | left, op, right | op âˆˆ eq, ne, i_eq, contains, starts_with, ends_with, matches (right = regex), gt/lt/gte/lte (numeric) |
| truthy | value | True for non-empty / non-zero / non-"false". A bare "${var}" reference uses the variable's typed truthiness. |
| defined | var | True when the variable is present and not null. |
| empty | value | True when the resolved value is the empty string. |
| all_of Â· any_of Â· not | conditions / condition | Logical combinators. |
| always Â· never | â€” | Constants. always is the natural condition for the catch-all else branch (or just leave else_steps set). |

```lua
-- Build differently depending on whether a 'pom.xml' is present.
{
  id   = "smart-build",
  name = "Smart build",
  if_block = {
    branches = {
      {
        condition = { kind = "compare",
                      left = "${has_pom}", op = "eq", right = "true" },
        steps = {
          { id = "mvn", name = "mvn package", command = "mvn -B package" },
        },
      },
      {
        condition = { kind = "compare",
                      left = "${has_gradle}", op = "eq", right = "true" },
        steps = {
          { id = "gw", name = "gradlew build", command = "./gradlew build" },
        },
      },
    },
    else_steps = {
      { id = "fail", name = "No build tool", command = "exit 1",
        allow_failure = false },
    },
  },
}
```

Nested `if_block` steps inside a branch's `steps` work â€” drilling deep is supported, the run viewer shows the
  parent/child tree, and resume from a failure re-runs the entire
  parent `if_block` (re-evaluating the condition on the
  fresh variable bag).

**Pipeline editor.** The generic `PluginPipelineEditor` component supports drilling into an
  if-block step via the small "open" arrow on its row â€” the breadcrumb
  above the sequence column tracks the path and lets the user pop back
  with one click. Plugins drive this by implementing the `enter_step` action (push current location onto a stack,
  re-emit a filtered `stages` list and a `breadcrumb`)
  and `navigate_to` (pop back to a given level).

## LuaOp steps

A **LuaOp** step calls a Lua function registered by a plugin
  instead of spawning a process. This is the right choice when you need
  structured file edits (JSON / YAML / TOML / XML), want access to the `arbor.*` API from within a step, or simply want to skip the
  shell round-trip for performance / portability.

Register a handler, then reference it from a step:

```lua
-- Register once (typical: in on_plugin_load)
arbor.pipeline.register_op("bump-config", function(params, ctx)
  -- params is the table from the step; ctx.cwd is the step's working dir.
  arbor.fs.json_set{ path = params.path, jpath = "$.version", value = params.version }
  return { exit_code = 0, stdout = "bumped " .. params.path }
end)

-- Use it in a pipeline def:
arbor.pipeline.define({
  id = "deploy", name = "Deploy", stages = {
    { id = "s1", name = "Bump",
      steps = {
        {
          id   = "b1", name = "Bump config.json",
          lua_op = { op = "bump-config",
                     params = { path = "config.json", version = "2.0.0" } },
        },
      } },
  },
})
```

Handler return shapes (all accepted):

- `nil` / `true` â†’ exit_code = 0 (success)
- `false` â†’ exit_code = 1
- `<number>` â†’ that exit code
- `<string>` â†’ stdout, exit_code = 0
- `{ exit_code?, stdout?, stderr? }` â†’ structured

Raising an error fails the step with the message captured in stdout/stderr.

## Built-in op catalog (arbor.core.*)

Two ready-made op modules ship inside every plugin sandbox: structured
  edits and assertions. They cover the bulk of pipeline plumbing â€” opt in
  per module; each one a plugin doesn't `require` stays unloaded.
  File / text ops aren't shipped here: they're trivial wrappers over `arbor.fs` / `arbor.text`, so plugins keep a local
  copy when they need them (see `plugins/source-export/pipeline_ops/` for the canonical reference).

| Module | Ops |
| --- | --- |
| arbor.core.edit | json_edit, yaml_edit, toml_edit, xml_edit |
| arbor.core.assert | assert_file_exists, assert_file_not_contains, assert_glob_matches, assert_version_bump |

Every op has the signature `function(params, ctx) -> { exit_code, stdout, stderr? }` and logs structured trace lines on stdout (`[op_name] key = value`)
  that the pipeline panel renders verbatim.

Two usage patterns â€” pick whichever fits:

```lua
-- Pattern 1: register every op in the module so pipeline
-- StepDefs can refer to them by bare name.
arbor.events.on("on_plugin_load", function()
  require("arbor.core.assert").register()
end)

arbor.pipeline.define({
  id = "deploy", name = "Deploy", stages = {
    { id = "verify", name = "Verify", steps = {
      { id = "war-exists", name = "WAR present",
        lua_op = { op = "assert_file_exists",
                   params = { path = "target/app.war" } } },
    } },
  },
})

-- Pattern 2: cherry-pick a single op without registering the whole module.
local assert_glob_matches = require("arbor.core.assert").assert_glob_matches
arbor.pipeline.register_op("assert_glob_matches", assert_glob_matches)

-- Plugin-local op for everything else â€” wrap arbor.fs / arbor.text directly:
arbor.pipeline.register_op("delete_war", function(params, ctx)
  local p = arbor.fs.join(ctx.cwd, params.path)
  if arbor.fs.exists(p) then arbor.fs.delete(p) end
  return { exit_code = 0, stdout = "removed " .. p }
end)
```

Permissions: every op routes filesystem access through `arbor.fs.*`,
  so the calling plugin's own `fs` level (`"none"` / `"read"` / `"write"`) and `fs_scope` apply. Requiring `arbor.core.assert` in a sandboxed plugin does NOT
  grant extra access.

## Structured file edits (arbor.fs.*_set)

Rust-backed helpers available from inside a LuaOp handler (or anywhere else
  with `fs_write` permission):

| Function | Backend | Path syntax |
| --- | --- | --- |
| arbor.fs.json_set{ path, jpath, value, pretty? } | serde_json | $.foo.bar, foo.bar, items.0.name, servers[1].host |
| arbor.fs.yaml_set{ path, ypath, value } | serde_yaml â†’ json walker | dotted path, same as JSON |
| arbor.fs.toml_set{ path, tpath, value } | toml crate | dotted path; comments are NOT preserved on rewrite |
| arbor.fs.xml_set{ path, xpath, value } | quick-xml | minimal XPath: /a/b/c, //c, /a/@attr, /a/b[@k='v']/c |

Intermediate nodes are auto-created for missing keys. `value` can
  be any serialisable Lua value (string / number / boolean / table) for JSON /
  YAML / TOML; XML takes a string (attribute value or element text).

## Live log stream

Subscribe to `arbor://pipeline-log` from the frontend (or via `arbor.events.on` in Lua) to receive log events as they happen.
  Payload shape: `{ run_id, ts, level, scope, message }` where `scope` is `"pipeline"`, `"stage:<stage_id>"` or `"step:<stage_id>.<step_id>"`. Only events at or above
  the run's `log_level` are emitted.

## Permissions

No special permissions are required to define or trigger pipelines â€” any plugin can call `arbor.pipeline.define()` and `arbor.pipeline.run()`. The commands run under the same OS user as Arbor itself. Plugins do *not* need the `terminal` permission for pipeline steps (that applies only to `arbor.terminal.exec`).

---

# Source Export Plugin

A Lua plugin that ships with Arbor. It exports the **source of your
  current repo** to a customer-visible copy, applying a declarative
  sequence of transformations (rename, delete internal files, bump versions,
  patch config, …) before handing the result off to their remote.

The plugin compiles a **profile** (stored per-repo) into a
  live Arbor pipeline and runs it through the standard orchestrator — so you
  get the same streaming output, logging, resume and lock semantics
  as any other pipeline.

## Opening the editor

Click the **Share2** icon in the RepoActions bar (the split
  button with the dropdown). The primary click runs the selected profile; the
  dropdown is split in two:

- **Profiles** (top) — selectable list, one per configured profile for the active repo. Clicking one sets it as the active selection (the Share2 button then runs it on click).
- **Actions** (below a separator, footer) — open a modal directly and *do not* change the selected profile: **New profile…** — empty or from template. **Edit configurations…** — open the full editor modal. **Plugin settings…** — global settings (output folder, cleanup policy, templates, `ju` binary).

*Note:* this is the same pattern used by the Workspace dropdown —
  footer items never become the combo's active value, so you can pick
  "Edit configurations…" without losing track of which profile was selected.

**Sequences** (multi-export meta-runs) live in a separate
  sidebar on the RIGHT ActivityBar — see the Sequences section below. The
  toolbar combo is intentionally per-repo only.

## Profile shape

| Field | Meaning |
| --- | --- |
| branch_src | Branch / tag to clone from (empty = current HEAD). Autocomplete from local branches + tags. |
| branch_dest | Destination branch (optional). Placeholder for a git_push step. |
| remote_url | Destination remote URL (optional). |
| auto_clone | When true (default), prepend an auto-clone stage: git clone $SOURCE_PATH $OUTPUT_PATH before any user step. |
| log_level | debug / info / warn / error. Debug prints every resolved command before execution. |
| variables | User-defined $KEY / ${KEY} placeholders usable inside any string field. |
| stages | Ordered list of groups; each group has a mode (sequential / parallel) and a list of steps. |

## Variable expansion syntax

Any string field in any step parameter runs through the expander before
  execution. The same resolver covers profile variables, sequence globals,
  per-item overrides, `set_variable` rebinds and the built-ins
  below — all in one namespace (built-ins win on name collision).

| Form | Meaning |
| --- | --- |
| $NAME | Greedy match on [A-Za-z0-9_]. Unresolved → left as-is for debuggability. |
| ${NAME} | Explicit brace form — required when the var is followed by letters / underscore (${FOO}bar vs $FOObar). |
| ${NAME:default} | Fallback when NAME is unset OR empty (bash ${VAR:-default} semantics). The default runs verbatim to the next } — it can contain : (URLs, paths); nesting is not supported. |
| ${NAME:} | Default is an empty string — forces empty when unset. |
| $$ | Literal $ escape. |

The expansion applies to the profile's `branch_src` too — you
  can write `${RELEASE_BRANCH:main}` and have the auto-clone
  stage resolve it at run time against the sequence's variable set.

## Built-in variables

Always available inside any string field (override user vars on name collision):

| Name | Value |
| --- | --- |
| $SOURCE_PATH | Absolute path of the active repo (cloned as the source). |
| $OUTPUT_PATH | Absolute path of the auto-clone destination. This is the default cwd of every step. |
| $BRANCH_SRC | Resolved source branch / tag. |
| $BRANCH_DEST | Destination branch from the profile (may be empty). |
| $PROFILE | Profile name. |
| $RUN_ID | Unique id for this run (stable across retry/resume). |
| $TIMESTAMP | ms since epoch at run start. |
| $COMMIT_SHA | HEAD sha of the source, if known. |
| $REPO_NAME | Source folder's basename. |

## Sequences (cross-repo meta-runs)

A **Sequence** is an ordered list of profile runs — optionally
  across different repositories — that share a single output folder and a
  matrix of variable overrides. Use it when a nightly build has to export
  several projects in a specific order, or when the same profile needs to run
  with several variable combinations.

Sequences live in the **right-side ActivityBar** under the *Workflow* icon. The sidebar is a clean list of sequences; the
  editor and the run history are full modals opened from there.

### The editor (3-column Items tab)

- **Info** — name, description, fail-fast toggle, output root override, and the sequence-level **Global variables** (kv_list).
- **Items** — 3-column layout: *Palette* (left): collapsible card per known repo; click any profile to append it to the sequence. *Sequence items* (middle): ordered list of picked items; click to focus one. *Detail* (right): move up / down / remove, *Profile* identity card with a click-to-copy repo path, a *Runtime* card (enabled / allow-failure), and *Variable overrides for this item* — the kv_list that layers on top of the sequence globals.
- **History** — this sequence's runs, newest first, with colored status glyphs and the output folder inline (click to copy, trailing button opens it in the OS file manager).

### Matrix variables — merge order

1. Profile-defined variables (tab Info of the profile itself)
2. Sequence global variables
3. Per-item variable overrides

All merged into one namespace; last writer wins. Built-ins
  (`$OUTPUT_PATH`, `$SOURCE_PATH`, …) always dominate on
  name collision. Use `${NAME:default}` for optional vars —
  see Variable expansion syntax above.

### Output folder

Every item in a sequence writes its output under `<output_root>/NN_profile/…`. If `output_root` is
  empty, the runtime auto-creates `<plugin.output_folder>/sequence_<name>_<ts>`. This
  override wins over the profile's own output logic for the duration of the
  sequence run — the profile stays untouched.

### Fail-fast vs continue-on-error

Off by default. When off, every enabled item runs even if a previous one
  failed, and the final status is `success` / `partial` / `failed` based on the mix. When on, the first failure marks
  the run `failed` and the rest are marked `skipped`.

### Deep-linking into a run

Each item row in the History modal shows the profile name as a clickable
  ghost button with an `ExternalLink` glyph. Click opens the
  standalone **Pipeline Run Detail** modal (z-index above the
  history modal) with the PipelineRunGraph + per-step output log — no need
  to open the bottom Pipelines panel.

### Persistence

Sequences are **global** (stored in `~/.config/arbor/plugin_data/source-export/global.json`) — they
  can fan out across repos from any workspace. Per-profile data (profiles
  themselves) remains per-repo as before. Runs are capped at the last 50
  entries and survive restarts; orphaned "running" runs left by a crash are
  swept to `failed` at plugin load.

## Operations catalog

### File

| Op | Purpose |
| --- | --- |
| create_file | Write a new file with literal content (multi-line safe via base64). |
| touch_file | Create an empty file, or update mtime if already present. |
| copy_file | Copy file or directory to a new location. |
| move_file | Move / rename. |
| delete_file | Delete one or more exact paths. |
| delete_pattern | Delete by glob pattern. Windows limitation: patterns are reduced to basenames (**/*.tmp → *.tmp) because PS -Include matches basenames only. Scope via the step's cwd or split into multiple steps. |
| append_file | Append content to an existing file (multi-line safe). |
| prepend_file | Prepend content (e.g. license headers). |

### Content

| Op | Purpose |
| --- | --- |
| replace_in_file | Find & replace inside one file. plain = literal, else regex. Multi-line find/replace are base64-encoded so quoting and newlines round-trip intact. |
| replace_on_glob | Same, applied to every file matching a glob. Logs every file it mutates. |
| insert_at_anchor | Insert a block before/after the first line matching a regex anchor. |
| properties_edit | Upsert key=value entries in a Java .properties file. Existing keys are replaced in place; missing ones appended. |
| env_merge | Same, for .env files. |
| template_render | Render a .tmpl file by substituting {{VAR}} placeholders with profile + built-in variables. Writes the output to a new path. |
| json_edit | Set a value at a dotted path ($.database.host). Parsed & written via serde_json (native LuaOp — cross-platform, no PowerShell). Value is parsed as JSON when possible (42, true, "x", {"y":1}), otherwise stored as string. |
| yaml_edit | Dotted-path set on YAML files via serde_yaml. Intermediate maps are auto-created, and scalars are parsed with JSON semantics so numbers / booleans / nested objects round-trip correctly. |
| toml_edit | Dotted-path set on TOML files via the toml crate. Same semantics as json_edit / yaml_edit. |
| xml_edit | Set InnerText on a node, or value on an attribute via a minimal XPath subset (//foo/@attr, /root/child[@k='v']). Native LuaOp powered by quick-xml — no PowerShell, Unix friendly. |

### Git

`git_init`, `git_clone`, `git_commit`, `git_tag`, `git_push`, `git_checkout`, `git_cherry_pick`, `git_merge`, `git_submodule_update`. Every op logs the resolved args
  (`cwd`, `branch`, `ref`, …) before running.
  Git operations default their `cwd` to `$OUTPUT_PATH` so they act on the clone, never on the source.

### Build / Dep

| Op | Behaviour |
| --- | --- |
| mvn_set_version | mvn versions:set -DnewVersion=… -DgenerateBackupPoms=false. Prefers local mvnw wrapper when present. |
| mvn_deploy | mvn deploy [-P<profile>] <extra>. Again prefers mvnw. |
| gradle_task | gradlew <tasks> when the wrapper exists, else gradle. |
| gradle_offline | gradlew dependencies --refresh-dependencies then copies ~/.gradle/caches to dest. Basic implementation — production-grade offline bundles usually need extra config. |
| npm_install | npm ci (strict lockfile). |
| pnpm_install | pnpm install --frozen-lockfile. |
| npm_pack | npm pack. |
| m2_offline_ju | Runs the external ju tool (path set in plugin settings) to extract Maven dependencies into an offline m2. |
| docker_build | docker build -t <tag> -f <dockerfile> <context>. |
| docker_push | docker push <tag>. |

### Validation

| Op | Check |
| --- | --- |
| assert_file_exists | File must exist. NOT toggle inverts — file must NOT exist. |
| assert_cmd_exit_zero | Command must exit 0. NOT toggle — must exit non-zero. |
| assert_env_set | Env var must be defined. NOT — must NOT be defined. |
| assert_branch_clean | Working copy must have no uncommitted changes. NOT — must be dirty. |
| assert_file_not_contains | Pattern must NOT appear. NOT — pattern MUST appear. |
| assert_glob_matches | Number of files matching the glob must be within [min, max] (max empty = unlimited). |
| assert_version_bump | Current version in pom.xml / package.json / Cargo.toml must be less than new_version (semver-ish comparison; prerelease tags ignored). |

### Execution & Flow

| Op | Behaviour |
| --- | --- |
| shell_command | Arbitrary shell one-liner. Variables are expanded before execution. |
| log_message | Print a log line at a given level. |
| notify_toast | Surface a toast via echo [NOTIFY] …. |
| set_variable | Compile-time rebind: mutates ctx.vars so every subsequent step's command uses the new value. Note: it can't capture another step's stdout — use static values or previously-set vars in value. |

## Not implemented (yet)

Listed below so you see the gap at a glance when building a profile. Adding
  any of these to a profile makes the compiler refuse to start the run with a
  clear error pointing at the step.

- **chmod_file**, **normalize_eol**, **strip_bom**, **strip_comments**.
- **lua_inline** — inline Lua evaluated inside the pipeline. Requires orchestrator hooks.
- **try_on_error** — control-flow op. Requires orchestrator-level policy changes.
- **`set_variable` capture** — the op currently takes a static `value`. Capturing another step's stdout needs orchestrator support for step-result chaining.

## Shell vs LuaOp

Most ops are implemented as **LuaOp handlers** — pure Lua in
  the Arbor process (no shell round-trip). They run faster, avoid the whole
  class of cmd.exe / PowerShell quoting traps, and have identical semantics
  on every OS. The remaining shell-bound ops are the ones that wrap external
  tools or run arbitrary user commands — no advantage to reimplement those
  in Lua.

The 22 generic LuaOp ops live in the `arbor.core.*` built-in
  modules (shipped inside every plugin sandbox). Source Export opts them in
  at load time with four `require(...).register()` calls; any
  other plugin can do the same and use the same catalog.

| Kind | Category | Who runs it |
| --- | --- | --- |
| LuaOp | create_file · touch_file · copy_file · move_file · delete_file · delete_pattern · append_file · prepend_file · replace_in_file · replace_on_glob · properties_edit · env_merge · template_render · insert_at_anchor · json_edit · yaml_edit · toml_edit · xml_edit · assert_file_exists · assert_file_not_contains · assert_glob_matches · assert_version_bump | In-process via arbor.pipeline.register_op handlers from arbor.core.{file,content,edit,assert} |
| Shell | shell_command · log_message · notify_toast · git_* · mvn_* · gradle_* · npm_* · pnpm_* · docker_* · m2_offline_ju · assert_cmd_exit_zero · assert_env_set · assert_branch_clean · set_variable (log-only stub) | Spawned process via cmd /C / sh -c |

## Safety guarantees

Every destructive op (`delete_pattern`, `replace_on_glob`,
  …) uses paths *relative* to `$OUTPUT_PATH` by default. The
  clone lives under the plugin's `output_folder` setting (defaults
  to `%TEMP%\arbor-source-export` / `/tmp/arbor-source-export`),
  suffixed with the profile name + a ms-precision timestamp so every run starts
  from a fresh, unique directory. The source repo is never touched.

Only change `output_folder` to a sensitive parent dir (e.g. your
  home) if you understand the blast radius: a `delete_pattern` inside a run would then see any files there.

## Import / export

Profiles and individual stages can be exported as JSON. Use the Upload icon
  in the toolbar of the Configurations modal (whole profile) or in a stage
  header (single stage). Import opens a native file picker so you can re-import
  anywhere. IDs are refreshed on import so imported stages never collide with
  existing ones.

---

# Pipelines — CI / CD

The **CI / CD** tab in the Pipelines panel connects to GitHub Actions or GitLab CI and shows real pipeline runs fetched directly from the API. This works for any repository whose remote URL points to `github.com` or a GitLab instance.

## Authentication

An OAuth token is required. Connect your account in **Settings → Access → Git & Integrations** before using the CI tab:

- **GitHub Actions** — connect your GitHub account via Device Flow. Arbor requests the `repo` + `read:user` scopes.
- **GitLab CI** — connect via GitLab Device Flow. Arbor requests the `api` + `read_user` scopes. Self-hosted instances use a host-based credential stored with **Settings → Additional Git Credentials**.

If no token is found, the CI tab shows a banner directing you to Settings rather than an error.

## CI run list

Each row in the CI run list shows:

- A **status pill** (Passed / Failed / Running / Cancelled / Pending) with colour coding.
- The **wall-clock duration** (computed from API timestamps).
- The **workflow / pipeline name** and its provider ID.
- The **branch chip** (accent colour) and short **commit SHA**.
- A human-readable **time-ago** label.

Click anywhere on a run card to open the **Pipeline Detail** modal.

## Pipeline detail modal

Clicking a run opens a full-screen modal showing:

- Header: provider icon, run name, branch/commit/duration chips, status badge.
- A **stage/job graph** — horizontal columns, one per stage (GitLab) or "Jobs" (GitHub). Each column lists job cards with their status icon, name, and duration. Clicking a job card opens its log page in the browser.
- Jobs with `allow_failure: true` are shown slightly dimmed with an **!** badge when they fail.
- **Re-run** and **Open in browser** buttons in the modal header.

For GitLab, jobs are grouped by their native `stage` name. For GitHub, all jobs appear in a single "Jobs" column since GitHub Actions does not expose a first-class stage concept in the jobs API.

## Creating a new pipeline run

Click the **Run** button in the CI / CD header (only visible when a token is configured) to open the *New Pipeline Run* modal:

- **Branch** — dropdown pre-filled with the current HEAD branch. All local branches are listed.
- **Workflow** (GitHub only) — dropdown listing active workflows that have `on: workflow_dispatch` configured. If no dispatch-enabled workflows are found, a hint is shown.
- **Variables** — dynamic key/value table. Add as many variables as needed; blank-key rows are ignored on submit. For GitLab these become `env_var` variables; for GitHub they become `workflow_dispatch` inputs.

After clicking **Run Pipeline**:

- **GitLab** — the new pipeline is created synchronously and the run list refreshes immediately.
- **GitHub** — a `workflow_dispatch` event is fired (HTTP 204). GitHub queues the run asynchronously, so the list refreshes automatically after a 3-second delay.

## What you can do

| Action | How |
| --- | --- |
| View recent runs | Switch to the CI / CD tab — the last 30 runs are fetched automatically |
| Create a new run | Click the Run button in the CI header — opens branch/variable picker |
| Refresh the list | Click the  button in the panel header |
| View stage/job graph | Click any run card to open the detail modal |
| Re-trigger a run | Click  in the run card or inside the detail modal |
| Open run in browser | Click  in the run card or modal header |
| Open a specific job's logs | Click a job card inside the detail modal |

## Run status mapping

| Arbor status | GitHub | GitLab |
| --- | --- | --- |
| ✅ Passed | completed / success | success, passed |
| ❌ Failed | completed / failure, timed_out | failed |
| ⏳ Running | in_progress, queued | running |
| ⭕ Cancelled | completed / cancelled, skipped | canceled, skipped |
| 🔵 Pending | waiting, requested | pending, created, scheduled |

## Self-hosted GitLab

Self-hosted GitLab instances are auto-detected from the remote URL (any host containing `gitlab.`). Store a personal access token via **Settings → Additional Git Credentials** using the instance hostname as the key. Arbor will use that token for all API calls to that host.

---

# Pull / Merge Requests

Browse, review, and merge GitHub Pull Requests and GitLab Merge Requests from the sidebar. Reuses the same OAuth tokens as the CI/CD panel â€” no separate setup.

- **Pull Requests** — Merge Â· Squash Â· Rebase Â· CI checks panel Â· Reopen.
- **Merge Requests** — Default strategy Â· Self-hosted instances supported Â· Reopen.

## Authentication

Connect your accounts in **Settings â†’ Git & Integrations**. The same tokens used for CI/CD are reused â€” no extra setup. Click the **GitPullRequest** icon in the Activity Bar to open the sidebar.

## Sidebar

- **Search bar**Client-side fuzzy filter over the loaded list â€” matches title, `#number`, source/target branches, author display name & login, and label names. Clear with the **Ã—** button. The query resets on tab switch.
- **Filter tabs**Switch between *Open* and *Merged* PRs/MRs. Backend reload â€” the search bar then narrows whichever set is loaded.
- **Row content**Status icon Â· title Â· number Â· source â†’ target Â· author Â· time-ago Â· comment count Â· labels.
- **Click row**Opens the detail modal.
- **Header +**Create a new PR/MR.
- **Refresh**Reload the list from the API.

## Detail modal

Four tabs across the top: **Overview**, **CI**, **Files**, **Commits**. Press `Esc` to close. The header has a **refresh** button that reloads detail + list + every tab that's already been opened.

- **Header** — State badge (Open / Merged / Closed) Â· Draft flag Â· title Â· branches Â· author Â· time-ago Â· labels Â· refresh Â· open in browser.
- **Overview** — Markdown description Â· CI Checks summary (when available) Â· Assignees Â· Reviewers Â· Activity timeline.

#### Markdown rendering

PR/MR bodies, descriptions, and comments share a single sanitised renderer. Dependabot, ReleaseDrafter, and other bots that pack large amounts of structure into the body render correctly now:

- **Inline HTML safelist**`<details>` / `<summary>` (collapsible cards with a chevron), `<p>`, `<blockquote>`, `<code>`, `<ul>` / `<ol>`, `<table>` and friends survive verbatim. Scripts, styles, iframes, event handlers, and raw `<a>` tags are stripped or rewritten.
- **Fence language auto-detect**fenced blocks without an explicit language (````` without a tag) are sniffed (*Rust, TOML, JSON, YAML, bash, TS/JS, markup*) and highlighted deterministically â€” no more wall-of-grey for bot-generated diffs.
- **Markdown also applies to inline contexts**same renderer is wired into the Issues detail modal so Linear / Jira (when ADF returns markdown) get the same treatment.

- **CI** — Pipeline runs targeting the source branch â€” status pill, duration, retrigger, click to open the stage/job graph.
- **Files / Commits** — Per-file diff view and commit-by-commit drill-down with syntax highlighting.
- **Actions** — Merge (split button) Â· Reopen (merged) Â· Close (with confirmation dialog).

### Activity timeline

The Overview tab renders comments and timeline events on a GitLab-style vertical rail. Three filter chips at the top toggle each category â€” counts always reflect what's loaded, regardless of visibility:

| Field | Value |
| --- | --- |
| Comments | Human-authored comments â€” large avatar nodes, accent-blue strip on the left edge of each card. Body rendered as Markdown (headings, lists, fenced code blocks with Prism syntax highlighting, blockquotes, links). |
| Bots *2* | Comments from automated accounts. Heuristic: GitHub login ending with `[bot]` or `github-actions`; GitLab login/name containing "bot". Bot cards get a soft yellow tint and full-height accent strip; the rail node is dashed-bordered yellow. |
| Activity *4* | System events: state changes (closed/merged/reopened/draft toggles), label edits, assignments, review requests, force-pushes, renames. Compact one-line rows with kind-colored icon nodes (state=red/purple/green by sub-type, commit=purple, label=blue, assign=green, review=orange, rename=yellow). |

#### Sanitisation

- **HTML comments stripped**`<!-- policy_violation_comment -->` and other invisible markers (used by the GitLab Security Bot, dependabot, etc.) are removed before rendering, so they no longer surface as literal text.
- **Emoji shortcodes**`:warning:` â†’ âš ï¸, `:white_check_mark:` â†’ âœ…, `:x:` â†’ âŒ, etc. ~90 shortcodes resolved (covers GitHub, GitLab and the common ecosystem aliases). Unknown shortcodes are left intact.
- **Activity body trimming**GitLab system notes that ship with an HTML expansion ("added 83 commits`<ul>â€¦</ul>`") are truncated at the first tag â€” the timeline shows just the human-readable lede.

#### Default visibility

Configure which chips start visible from **Settings â†’ Access â†’ Merge Requests**. Defaults are stored in `~/.config/arbor/config.toml` under `[mr]`:

```toml
[mr]
default_show_comments = true
default_show_bots     = true
default_show_activity = true
```

Toggling a chip inside an open modal is session-only â€” it never writes back to the config. Use Settings to change the global default.

### Closing a PR / MR

The **Close** button (visible when the PR/MR is open) asks for explicit confirmation in a centred dialog before sending the close request â€” no more "I clicked it thinking I was closing the modal" mistakes. The dialog spells out which number is about to be closed.

### CI tab

Reuses the same GitHub Actions / GitLab CI integration as the **Pipelines** panel, scoped to the source branch of this PR/MR.

| Field | Value |
| --- | --- |
| Provider header | Shows the detected provider (*GitHub Actions* or *GitLab CI*), the source branch chip, and a refresh button. Hidden when no remote is detected or no token is connected. |
| Run cards | Status pill (Passed / Failed / Running / Cancelled / Pending), wall-clock duration, run name + numeric id, short commit SHA, time-ago. |
| PR HEAD pill | The run whose commit SHA matches the current PR head is marked with an accent *PR HEAD* pill and an accent border, so the run that built the latest push stands out. |
| Re-trigger | Per-row button. Calls `POST /actions/runs/{id}/rerun` on GitHub or `POST /pipelines/{id}/retry` on GitLab, then reloads the list. |
| Open in browser | Per-row link to the run's web page on the provider. |
| Detail modal | Click a card to open the full stage / job graph â€” same modal used from the Pipelines panel. The provider icon is brand-tinted (orange for GitLab); stages render left-to-right in execution order; `Esc` closes. |

> ℹ Authentication is shared with the CI/CD panel â€” connect your GitHub or GitLab account once in **Settings â†’ Authentication** and the CI tab picks the same token up. Self-hosted GitLab instances are supported.

#### How runs are discovered

Both providers can attach pipeline runs to a PR/MR via paths a plain branch filter would miss. To catch all of them Arbor hits two endpoints per provider in parallel and deduplicates by run id (newest first).

| Field | Value |
| --- | --- |
| GitHub | **Branch query**`GET /actions/runs?branch={source_branch}` â€” push and `pull_request` runs whose `head_branch` matches. **Head-SHA query**`GET /actions/runs?head_sha={head_sha}` â€” fork PRs, `pull_request_target` workflows, and `workflow_dispatch` runs pinned to the SHA. These don't always tag the source branch on the run. |
| GitLab | **Detached MR pipelines**`GET /merge_requests/:iid/pipelines` â€” required for pipelines whose `ref` is `refs/merge-requests/{iid}/head`. These are the ones GitLab shows at the top of the MR page as *"Merge request pipeline #..."* and would otherwise be invisible to a plain branch filter. **Branch pipelines**`GET /pipelines?ref={source_branch}` â€” regular pushes to the source branch. |

### Merge options

The merge button is a split button. Click the main area for the default strategy, or the chevron for:

| Field | Value |
| --- | --- |
| Merge commit | Creates a merge commit `default` |
| Squash and merge | Squashes all commits into one |
| Rebase and merge | Rebases onto the target branch `GitHub only` |

Two checkboxes sit next to the split button and apply to whichever strategy you pick:

| Field | Value |
| --- | --- |
| Squash | Collapse all commits of the branch into one before merging. |
| Delete branch | Remove the source branch on the remote and also clean up the local copy.
    Local deletion is guarded: if the source branch is currently checked out
    (in this repo or any worktree) Arbor first tries to switch to the target,
    and when that's not possible it keeps the branch and posts a warning in
    the notifications bell explaining what to do. |

#### Local-cleanup safety rules

When **Delete branch** is ticked, Arbor only removes the local copy of the source branch after all these conditions are met:

- **Branch exists locally**Nothing to do if you never had it â€” the step is a no-op.
- **No worktree is using it**A linked worktree holding the branch blocks deletion. Arbor notifies with the worktree path so you can remove it first.
- **HEAD switched away**If the source branch is the current branch, Arbor checks out the target before deleting. A dirty workdir or a missing local target aborts the cleanup with a warning.

> ℹ The button is disabled while the merge status is being checked. When conflicts are detected it is replaced by *Resolve Conflicts*, which fetches `origin`, checks out the source branch, and merges the target into it so you can finish the merge locally.

## Creating a PR / MR

Click **+** in the sidebar header.

| Field | Value |
| --- | --- |
| Title `Req` | Summary shown in the list. |
| Source branch | Defaults to the current branch of the active repo. |
| Target branch | The branch to merge into (e.g. `main`). |
| Description | Optional markdown text. |
| Labels | Comma-separated label names. |
| Draft | Mark the PR/MR as a draft / work in progress. |
| Auto-merge | Arm the platform's auto-merge when the PR/MR is opened. The platform
    merges once required checks pass â€” GitHub uses *auto-merge* (requires branch protection on the target branch), GitLab uses *merge
    when pipeline succeeds*. If it can't be armed, a notification is
    posted in the bell; the PR/MR itself is still created. |

Merge strategy and source-branch deletion are chosen at merge time from the detail modal, not here.

## Supported features

| Feature | GitHub | GitLab |
| --- | --- | --- |
| List open / closed / merged | âœ“ | âœ“ |
| Sidebar search (client-side) | âœ“ | âœ“ |
| Markdown description & comments | âœ“ | âœ“ |
| Emoji shortcodes (:warning: â†’ âš ï¸) | âœ“ | âœ“ |
| Activity timeline (state / labels / assigns / â€¦) | âœ“ via /events | âœ“ via system notes |
| Bot detection (filterable) | âœ“ [bot] suffix | âœ“ name heuristic |
| Create PR / MR | âœ“ | âœ“ |
| Auto-merge on creation | âœ“ branch protection req. | âœ“ merge-when-pipeline-succeeds |
| Merge | âœ“ merge / squash / rebase | âœ“ merge / squash |
| Delete source branch on merge | âœ“ | âœ“ |
| Close (with confirmation) / Reopen | âœ“ | âœ“ |
| Add comment | âœ“ | âœ“ |
| Draft / WIP flag | âœ“ | âœ“ |
| Labels | âœ“ | âœ“ |
| Assignees / Reviewers | âœ“ | âœ“ |
| CI checks summary (Overview) | when available | â€” |
| Pipeline runs tab (filtered by source branch) | âœ“ | âœ“ |
| Re-trigger run from PR/MR | âœ“ | âœ“ |
| Self-hosted instance | â€” | âœ“ |

## Plugin hooks

Declare the hook booleans in `[hooks]` and register handlers in Lua.

```toml
# plugin.toml
[hooks]
on_mr_opened  = true
on_mr_merged  = true
```

```lua
-- main.lua
arbor.events.on("on_mr_opened", function(ctx)
  arbor.notify{ title = "PR opened", message = "#" .. ctx.number .. ": " .. ctx.title, level = "info" }
end)

arbor.events.on("on_mr_merged", function(ctx)
  arbor.notify{ title = "PR merged", message = "#" .. ctx.number .. " was merged", level = "success" }
end)
```

### Hook reference

| Hook | Constant | Context |
| --- | --- | --- |
| on_mr_opened | hooks.MR_OPENED | number, title, source_branch, target_branch, author, provider, web_url |
| on_mr_merged | hooks.MR_MERGED | number, provider |
| on_mr_updated | hooks.MR_UPDATED | number, provider future use |

---

# Issues â€” Linear & Jira

Browse, filter, and act on issues directly from the sidebar without switching context. Each repository can independently use either tracker.

- **OAuth Â· Personal API Key** — Full read/write access. Attach issues to branches, transition statuses, post comments from plugins.
- **Cloud Â· Data Center Â· Server** — Email + API token, PAT for DC/Server, OAuth 2.0 (3LO) for Cloud. Self-signed certs accepted.

## Setup

Open the **Issues** sidebar and pick a tracker â€” or configure credentials in **Settings â†’ Git & Integrations â†’ Issue Trackers**. Each repository stores its own selection.

### OAuth Recommended

1. Register a **Public OAuth application** at `linear.app â†’ Settings â†’ API â†’ OAuth applications`
2. Add `http://127.0.0.1:7729/callback` as the redirect URI
3. Click **Connect â†’ OAuth** in settings and approve in the browser
4. Arbor completes the PKCE flow and stores the token in the OS keychain

### Personal API Key

1. Generate a key at `linear.app â†’ Settings â†’ API â†’ Personal API keys`
2. Click **Connect â–¾ â†’ Personal API Key** and paste the `lin_api_â€¦` token

### API Token â€” Jira Cloud Recommended

Generate an API token at `id.atlassian.com â†’ Security â†’ API tokens`, then click **Connect â†’ API Token** and fill in:

| Field | Value |
| --- | --- |
| Subdomain | The part before `.atlassian.net` (e.g. `mycompany`) |
| Email | Your Atlassian account email |
| API token | The token just generated |

### Personal Access Token â€” Data Center / Server

Generate a PAT at `Jira â†’ Profile â†’ Personal Access Tokens`. Use the **API Token** form with the full hostname as the subdomain (e.g. `jira.internal.example.com`) plus email and PAT.

> ℹ Arbor automatically accepts self-signed or internal-CA certificates common in on-premise Jira installations.

### OAuth 2.0 (3LO) â€” Jira Cloud only

Click **Connect â–¾ â†’ OAuth 2.0** and follow the browser prompt. Arbor auto-discovers your site and stores access + refresh tokens in the OS keychain. Token refresh is transparent.

### Jira compatibility matrix

| Edition | Auth | API | Notes |
| --- | --- | --- | --- |
| Cloud *.atlassian.net | Token Â· OAuth 2.0 | v3 | Full feature set |
| Data Center â‰¥ 8.4 | Email + PAT | v2 | Self-signed certs OK |
| Server / DC < 8.4 | Email + PAT | v2 | Uses /project endpoint |

## Sidebar

Same UI for both providers. Filters combine freely.

- **Search** Debounced 350 ms. Two modes: `PROJ-42` **Default** â€” matches the ticket code *and* any text that mentions it. Free-form text (e.g. `login bug`) falls back to text matching across title / description / comments. `~PROJ-42` **Text-only** â€” the `~` prefix bypasses the code lookup. Finds only descriptions / comments / titles that *mention* `PROJ-42`, never the ticket whose key is `PROJ-42`. Useful for tracing references without the noise of the ticket card on top. On Jira the text side searches `summary + description + comments` (the JQL `text ~` operator). On Linear it searches `title` only â€” the GraphQL filter doesn't expose body / comment search.
- **Me**Show only issues assigned to you.
- **Status**Multi-select grouped by type: backlog / unstarted / started / completed / cancelled. Falls back to statuses derived from loaded issues when the API returns none.
- **Team / Project**Linear team or Jira project. Search box appears when more than 5 options exist. Jira fetches all paginated pages alphabetically.
- **Issue Type**Jira only. Multi-select by type (Bug, Story, Task, Epic, Sub-taskâ€¦) with per-type colour indicators.
- **Milestone**Linear project milestone or Jira fix version.
- **Sprint / Cycle**Jira active sprint or Linear cycle.
- **+**Open the Create Issue form.

### Issue card

Priority emoji Â· Identifier (`ARB-123`) Â· Title Â· Labels Â· Status badge Â· Assignee avatar Â· Time-ago Â· Comment count. **Click** opens the detail modal, **right-click** the context menu.

### Ticket picker

Appears when creating a branch via GitFlow or the graph context menu. Uses the active repo's tracker automatically â€” no need to open the sidebar first. Same filters as the sidebar; selecting an issue populates the branch name.

## Detail modal

Click an issue card to open the full detail view: metadata sidebar, description, attachments, linked commits, and threaded comments.

### Description & comments rendering

Bodies are rendered with full styling â€” headings, lists, code blocks, tables, blockquotes, panels, mentions, status lozenges:

- **Linear**Markdown rendered in-app via the shared sanitised renderer (same used by PR/MR bodies). Inline HTML safelist supports collapsible `<details>` / `<summary>`, tables, blockquotes and code; fenced blocks without an explicit language are auto-detected (Rust, TOML, JSON, YAML, bash, TS/JS, markup) and highlighted with Prism.
- **Jira**Server-rendered HTML via `expand=renderedFields` (covers ADF on Cloud and wiki markup on Server / Data Center). HTML is sanitized with `ammonia` before display â€” scripts, iframes, event handlers and inline styles are stripped; `class` survives so syntax highlighting and panel chrome land correctly.

### Attachments

Jira issues with attached files show a grid of cards between the description and the linked commits. Each card has a type-aware icon (image / video / audio / pdf / archive / text / generic), filename, size, and MIME type.

- **Click to download**Opens Arbor's in-app save picker with the original filename pre-filled. The fetch only starts after you confirm a destination â€” cancelling the picker is a true no-op.
- **Authenticated & streamed**The download runs on the Tokio runtime off the UI thread, and the body is streamed chunk-by-chunk straight to disk â€” no whole-file buffering in RAM, no UI freeze.
- **Host-locked**The backend rejects download URLs whose host doesn't match the configured Jira instance, so the IPC command can't be coerced into acting as a generic authenticated proxy.
- **Status feedback**The card icon becomes a spinner while downloading and a green âœ“ on success; failures show a red border and a toast.

## Jira field mapping

| Arbor concept | Jira field | Notes |
| --- | --- | --- |
| Teams | Projects | Project key used for JQL (project = "KEY") |
| Issue Type | Issue Type | Bug / Story / Task / Epic / Sub-task; colour per type |
| Status | Status | Status category â†’ type (unstarted / started / completed) |
| Labels | Labels | Plain strings; colour deterministic |
| Priority | Priority | Highest â†’ Urgent, High, Medium, Low/Lowest |
| Cycle | Sprint | Active sprints via Agile API (Jira Software only) |
| Milestone | Fix Version | First fix version on the issue |
| Estimate | Story Points | customfield_10016 |

## Create Issue

Two-column form â€” title/description left, metadata right.

- **Linear fields** — Team Â· Status Â· Priority Â· Project Â· Milestone Â· Assignee (self) Â· Labels Â· Due date Â· Estimate.
- **Jira fields** — Project Req Â· Issue Type (default: Task) Â· Priority Â· Labels Â· Assignee (self) Â· Fix Version Â· Due date Â· Story Points.

## Plugin API â€” arbor.issues

Works identically for Linear and Jira â€” the active provider for each repo is resolved transparently.

| Field | Value |
| --- | --- |
| `issues = "read"` | Enables `arbor.issues.search()` and `arbor.issues.get()` |
| `issues = "write"` | Enables `arbor.issues.transition()` and `arbor.issues.comment()` `implies read` |

```lua
local issues = arbor.issues.search({
  query        = "login",
  assigneeMe   = true,
  statusIds    = { "10001", "10002" },    -- Jira status IDs or Linear workflow-state UUIDs
  labelIds     = { "bug" },               -- Jira: label name; Linear: label UUID
  issueTypeIds = { "Bug", "Story" },      -- Jira only (ignored on Linear)
  teamId       = "PROJ",                  -- Jira: project key; Linear: team UUID
  limit        = 25,
})

for _, issue in ipairs(issues) do
  print(issue.identifier, issue.title, issue.status.name)
end

-- Transition issue (Jira resolves status ID â†’ workflow transition automatically)
arbor.issues.transition(issue.id, status_id)

-- Add a comment
arbor.issues.comment(issue.id, "Deployed to staging âœ“")

-- Branch name slug
local branch = arbor.issues.branch_name(issue)
-- Linear: "arb-123-fix-login-bug"
-- Jira:   "proj-456-fix-login-bug"
```

## Plugin hooks

| Constant | Event | Context fields |
| --- | --- | --- |
| hooks.ISSUE_LINKED | on_issue_linked | issue_id, identifier, sha, branch |
| hooks.ISSUE_TRANSITIONED | on_issue_transitioned | issue_id, identifier, from_status, to_status |

---

# Security Dashboard

GitLab- and GitHub-native security posture inside Arbor: severity counters,
  risk score, vulnerabilities-over-time chart, and a virtualized findings
  modal â€” gated automatically per repo so it shows up only where the provider
  has data.

- **Vulnerability Report** — GraphQL: severity counts Â· time series Â· risk score Â· per-finding metadata. Ultimate-only fields degrade gracefully.
- **GHAS Â· Dependabot Â· Secret Scanning** — Three REST sources merged into one finding list. Time series unavailable (GitHub doesn't expose it).

## Authentication & visibility

No extra setup â€” the same OAuth token used by the MR/CI panels is reused.
  When the active tab's repo has a remote on a supported host, Arbor fires
  a lightweight provider probe (`supports_security`); the
  Activity Bar icon and the StatusBar chip become live as soon as it
  resolves. Tokenless repos and providers that don't expose the dashboard
  see a clear "not available" state instead of a broken icon.

## Activity Bar entry

Click the **ShieldAlert** icon in the left Activity Bar
  (top group, after Branches). The icon is always rendered; the panel
  itself decides what to show:

- **Probing**Spinner + "Checking providerâ€¦" while the support probe is in flight.
- **Not available**Static copy explaining likely causes (no GitHub/GitLab remote, missing token, plan without scanning), with a Re-check button.
- **Loading summary**Standard spinner for the headline fetch.
- **Loaded**Filter bar Â· 6 severity counter cards Â· risk-score gauge + vulns-over-time chart Â· truncation note when the cap is hit.

## Headline counters

Six cards: **Critical / High / Medium / Low / Info / Unknown**.
  Each shows the *active* count and the median age in the band's
  colour (e.g. *9 mo*, *113 days*). Clicking a non-zero card
  opens the detail modal at that severity tab.

### What "active" means

The dashboard **always excludes resolved and dismissed findings**.
  Both backends enforce this: GitLab passes `state: [DETECTED, CONFIRMED]` to `vulnerabilitySeveritiesCount`; GitHub already filters via `?state=open`. Closed findings live behind the modal's scope toggle.

## Risk score & time series

The risk gauge renders a 0â€“100 score with bands (Low / Medium / High /
  Critical). The score is a host-side heuristic `(criticalÃ—10 + highÃ—5 + mediumÃ—2 + lowÃ—0.5)` capped at 100.

The vulnerabilities-over-time chart pulls 30/60/90 day windows.
  GitLab Ultimate exposes `vulnerabilitiesCountByDay`; GitHub
  doesn't, so the chart is hidden on GitHub repos.

When the panel is narrow, the gauge and chart automatically stack
  vertically â€” the layout uses a CSS container query, so it tracks the
  panel width rather than the viewport.

## Detail modal

Opened from a counter card click. Layout:

- **Header**Shield icon Â· risk pill Â· "Open in <provider>" external link.
- **Tabs**Per-severity strip (`All | Critical | High | â€¦`) â€” counts dynamic, zero tabs disabled.
- **Scope toggle**Two-button segmented control beside the tabs: `Active` (default) shows Detected + Confirmed, `Closed` shows Resolved + Dismissed. Persisted in `localStorage`.
- **Progress bar**Indeterminate sliding bar at the top of the list region â€” shows during fetches AND during tab/scope swaps to mask the DOM-thrash on large severities.
- **Virtualized list**Each row is a fixed 64px so the list can render 300+ findings as ~20 DOM nodes. Severity desc â†’ age desc sort.
- **Footer**"*Showing N of M findings*" plus a truncation hint when the host-side cap kicked in.

### Finding-detail modal

Click any row in the list to open a dedicated **per-finding** modal that lifts the full payload above the aggregate view. Layout:

- **Header**Severity chip Â· title Â· CVE / report-type chips Â· "Open in <provider>".
- **Remediation** Prominent *"How to fix"* block when the provider exposes one.
      GitLab â†’ `Vulnerability.solution` as-is. GitHub Dependabot â†’
      synthetic hint built from `first_patched_version`: *"Upgrade `pkg` to `X` or later (vulnerable range: `R`)"*.
      Markdown-rendered, so links and code-fences in vendor advisories render
      correctly.
- **Metadata grid**Identifiers (CVE, CWE), file/line, package + version, age, last-detected, state history.
- **Description & references**Long-form Markdown body + outbound links to advisories/CVE pages.

### Active vs Closed scope

The toggle *only* affects the modal â€” the dashboard's counter
  grid, gauge, chart, and the StatusBar chip always stay on the active
  scope. Switching to `Closed` refetches with `state: [RESOLVED, DISMISSED]` and lets you audit the
  finding hygiene without polluting the headline numbers.

## Filter bar

Above the counter grid:

- **Search**Host-side substring match on title + file path. 250 ms debounce.
- **Severity multiselect**Narrows counters, chart, and the modal list.
- **Type multiselect**Auto-populates from the loaded findings â€” `sast`, `dependency_scanning`, `secret_detection`, â€¦ etc.
- **Clear**Resets severity / type / search but *preserves the state scope* â€” the user's scope choice is treated as a view mode, not a narrowing filter.

## StatusBar chip

Left side of the footer, right after the branch chip. Shield icon with
  a corner badge showing the total active finding count
  (`99+` when capped). Tooltip carries the per-severity
  breakdown. Click â†’ floating Quick Overlay anchored to the left of the
  footer with the full severity rundown plus *Open dashboard* / *Open in provider* shortcuts.

## Caching

The store dedupes concurrent IPC calls and persists the user-facing knobs:

- **Probe cache**Per-tab support result kept in memory for the session. Refreshing the panel invalidates it.
- **In-flight dedup**`loadSummary` / `loadFindings` share a single Promise per tab â€” the AppShell pre-load and the panel mount fetch can fire concurrently without racing each other into a stuck loading state.
- **localStorage**Range (30/60/90), severity filter, report-type filter, state scope.

## Lua API

Plugins read posture data via `arbor.security.*`. The token
  never leaves the host â€” provider permission gate is the same `provider = "read"` flag used by `arbor.mr.*` and `arbor.ci.*`.

```lua
-- Cheap probe â€” false for tokenless repos / providers without a dashboard
local ok, err = arbor.security.supports({ repo_id = "myrepo" })

-- Headline summary (active findings only). Same shape the panel renders.
local summary, err = arbor.security.summary({
  repo_id    = "myrepo",
  range_days = 90,            -- optional, clamped to [7, 90], default 30
})
-- summary.counts        : { critical, high, medium, low, info, unknown }
-- summary.median_age_days
-- summary.risk_score    : { value: number, label: string } | nil
-- summary.time_series   : { points = {...}, range_days } | nil
-- summary.web_url       : provider-native dashboard URL

-- Findings list. Defaults to active scope; opt into closed by passing states.
local list, err = arbor.security.findings({
  repo_id      = "myrepo",
  severities   = {"critical", "high"},   -- optional
  states       = {"resolved", "dismissed"}, -- optional, default {detected,confirmed}
  report_types = {"sast", "secret_detection"},
  search       = "deserialization",
  limit        = 200,
})
for _, f in ipairs(list) do
  -- f.solution is non-nil on GitLab and on GitHub Dependabot (synthetic
  -- "Upgrade ... to X" hint from first_patched_version). On code-scanning
  -- and secret-scanning it stays nil.
  arbor.log.info("[%s] %s â€” %s%s",
                 f.severity, f.title, f.web_url or "no url",
                 f.solution and (" Â· fix: " .. f.solution) or "")
end
```

## Hooks

Two hooks contribute to the `security` category:

| Field | Value |
| --- | --- |
| `on_security_summary_loaded` | Fired by the host after every successful summary fetch. Payload: `{ tab_id, provider, counts, total, risk_label?, web_url? }`.
    All `counts` values are active-only. Use it for
    notifications when posture worsens, or to mirror counts to an external
    dashboard. |
| `on_security_finding_state_changed` | A plugin-cooperation channel: when a plugin observes a finding moving
    between active and closed states (e.g. a periodic rescan), it can
    emit this hook so other plugins can react. The host itself does NOT
    emit it on every fetch â€” keeps the channel signal-only.
    Payload: `{ tab_id, finding_id, severity, from_state?, to_state, title?, web_url? }`. |

### Example: notify on new Critical findings

```lua
-- plugins/security-watch/main.lua
local last_critical = {}   -- repo_id â†’ previous critical count

arbor.events.on("on_security_summary_loaded", function(ctx)
  local prev = last_critical[ctx.tab_id] or 0
  local now  = (ctx.counts and ctx.counts.critical) or 0
  if now > prev then
    arbor.notify({
      title   = "New critical vulnerabilities",
      message = string.format("%s: %d new (was %d) â€” open the dashboard.",
                              ctx.tab_id, now - prev, prev),
      level   = "warning",
    })
  end
  last_critical[ctx.tab_id] = now
end)
```

## Permissions & manifest

All `arbor.security.*` calls require the `provider` permission at `read` level (or higher) in `plugin.toml`:

```toml
# plugin.toml
[permissions]
provider = "read"   # read-only access to MR/CI/security
```

## Provider differences (cheat sheet)

| Capability | GitLab | GitHub |
| --- | --- | --- |
| Dashboard probe | GraphQL (vulnerabilitySeveritiesCount + vulnerabilities) | REST x3 (code-scanning Â· dependabot Â· secret-scanning) |
| Severity counts | Server-side, state-filtered | Computed host-side from open alerts |
| Time series | Ultimate-only via vulnerabilitiesCountByDay | Not exposed â†’ chart hidden |
| Risk score | Heuristic (host-side) | Heuristic (host-side) |
| Self-hosted | Host-keyed PAT in keychain | n/a |

---

# Deep Links (arbor://)

Arbor registers the `arbor://` URI scheme on your OS so links shared by colleagues,
  CI bots, browser extensions, or desktop shortcuts can drop you straight into the right place
  inside Arbor — no copy-pasting branch names or commit SHAs.  Arbor brings the existing window
  to the foreground (single-instance), or starts cold if it isn't running yet.

> **Off by default.** Deep links are disabled out of the box and every action kind is individually opt-in. An incoming URL on a fresh install is intercepted and shown as a "Deep Link Blocked" modal. Turn the master switch on in **Settings → Tools → Deep Links → Master switch**, then enable each action you want to accept under **Enabled actions**.

## URL shape

Every URL identifies the **repository** with a `?url=` query parameter
  carrying the *remote git URL* (HTTPS or SSH).  Local paths would be useless across
  machines, so they're never used as deep-link identifiers.  Arbor looks the URL up against your
  registered repositories using a fuzzy host/owner/repo key — `https://github.com/foo/bar.git`, `git@github.com:foo/bar`, `ssh://git@github.com/Foo/Bar.git/` all match the same clone.

| URL | Action |
| --- | --- |
| arbor://repo/open?url=<url> | Open the repository (or clone it) |
| arbor://commit/<sha>?url=<url> | Switch to the repo and jump to a commit in the graph |
| arbor://branch/<name>?url=<url>&checkout=1 | Stash-safe checkout of the named branch |
| arbor://branch/<name>?url=<url>&worktree=1 | Open the "Add worktree" dialog pre-filled with the branch |
| arbor://mr/open/<number>?url=<url> | Open the merge / pull request detail modal |
| arbor://pipeline/<run-id>?url=<url> | Open the CI pipeline run detail modal |

## Generating links from inside Arbor

Every action above has a **"Copy arbor:// link"** entry point in the UI:

- **Repository**The link icon next to the commit count in the graph toolbar copies `arbor://repo/open`.
- **Branch**Right-click any local or remote branch in the sidebar → "Copy arbor:// checkout link".
- **Worktree**Right-click any worktree row in the sidebar (or use the link icon in the worktree info modal) → "Copy arbor:// worktree link".
- **Merge / pull request**Right-click any row in the MR sidebar, or use the link button in the MR detail modal header.
- **CI pipeline run**Right-click any row in the Pipelines panel, or use the link button in the CI run detail modal header.
- **Commit**Right-click any node in the graph, or use the link button in the commit detail panel.

All of these embed the active repository's **origin remote URL** in the `?url=` parameter (falling back to the first remote when no `origin` exists).  When the repo has no remotes configured, the copy buttons stay enabled but show a
  warning toast — there's no shareable URL to embed.

## The three gates

Every incoming `arbor://` URL passes through three gates before anything happens:

1. **Master enable**If off (default), the dispatcher short-circuits to a "Deep Link Blocked" modal that names the feature and points the user at Settings.  Nothing else runs.
2. **Per-action enable**Even with the master on, each action kind (*open repo*, *jump to commit*, *checkout branch*, *create worktree*, *open MR*, *open pipeline*) has its own toggle, all default off.  A blocked action shows the same disabled modal but names the specific kind.
3. **Per-action confirm**If both gates above let the URL through, the dispatcher shows the action-confirm modal explaining what will happen.  Confirms can be disabled per-action for trusted flows (e.g. read-only commit jumps).

## Confirmation prompts

Every deep-link action shows an **"Are you sure?"** modal by default
  before doing anything — Arbor never executes a shared link without an explicit click.
  The prompt names the action ("Check out branch `feature/x`") and shows
  the target git URL so you can sanity-check who's asking.

In **Settings → Tools → Deep Links → Confirmations** you can disable the prompt
  per-action for the cases you trust (e.g. *commit jump*, which is read-only).
  The clone-confirm dialog is independent of these toggles: if the local copy is missing,
  Arbor always asks before cloning — it has to, you need to pick the destination folder.

## Routing rules

The dispatcher resolves the target repo using the registry, then applies these rules:

- **In the active workspace**Activate the existing tab (open one if the repo is registered but not currently a tab).
- **In another workspace**Apply your **Cross-workspace target** setting — either switch workspace and activate the tab, or surface it as a cross-workspace tab in the workspace you're already in.
- **Registered but not in any workspace**Add it to the active workspace and open the tab.
- **Not in the registry**Show the clone-confirm dialog (folder picker + Clone & Continue button).  If you cancel, nothing happens.
- **Local copy missing on disk**Same clone-confirm dialog, but the wording explains the local copy is gone.  Re-cloning replaces the missing folder transparently and the action proceeds.

## Checkout links → worktrees

The **Checkout links create a worktree** setting silently rewrites incoming `arbor://branch/<name>?checkout=1` URLs to the worktree variant before
  dispatch.  Useful when your workflow is "every shared branch becomes its own worktree" —
  the shared link never moves HEAD on your main checkout.  The Add Worktree dialog opens
  pre-filled with the branch, you pick the destination, nothing happens to disk until you
  click **Add**.  Links you copy out of Arbor still embed the literal action
  they were built from — the rewrite only applies to incoming links you receive.

## Cross-workspace strategy

Configurable in **Settings → Tools → Deep Links**:

- **Switch to that workspace** (default)Changes the active workspace to the first one that owns the target repo (in the order shown in your workspace dropdown), then activates the tab inside it.
- **Open here as cross-workspace tab**Adds the repo to the workspace you're currently focused on, marked with the cross-workspace dot.  Doesn't disturb your current focus.

## Cold start vs. warm path

Arbor uses single-instance mode for deep links: clicking `arbor://…` while Arbor is
  already running brings the existing window to the foreground and forwards the URL to it.
  Clicking it while Arbor is closed launches the app and processes the URL after the UI has had a
  chance to mount — URLs received in the boot window are buffered, then drained the moment the
  frontend is ready.  In both cases you don't see a blank window or a missed action.

## Dev-mode toggle

Deep-link support is always on in release builds.  In debug builds it's gated behind the `deep-link-dev` Cargo feature, which is currently in `default` so it works
  out of the box.  Drop it from the default features (and rerun with `--features deep-link-dev`) to test how the app behaves without it.

---

# Settings — Interface & Git

Open Settings with `Ctrl+,` or via the gear icon in the title bar. Settings are organised into groups in the left sidebar — Interface, Git, Performance, Access, and Project.

## Interface

### Appearance

- **Font scale** — scales all UI text from 0.8× to 1.4× in 5 % increments. Useful on HiDPI or small screens.

### Animations

Controls the speed and behaviour of every transition and motion effect in the UI.
  Settings are stored in `localStorage` and take effect immediately.

- **Enable animations** — master toggle. When off, all transitions play at
    zero duration: panels appear instantly, toasts pop in without sliding, modals open without
    scaling. Useful for accessibility (reduced-motion preference) or low-powered hardware.
- **Speed** — three presets that scale every duration proportionally: *Snappy* — ~55 % of default durations. Tight, fast feel. *Normal* — 100 % (default). Balanced and polished. *Relaxed* — ~165 % of default durations. Smoother, more fluid motion.
- **Preview** — hit *Replay* to see an animated chip using the current speed
    setting without leaving the panel.

Animations that are controlled by this setting include: sidebar slide-in, bottom/right panel
  slide-in, modal and command-palette entrance, toast slide-in/out, overlay fade, settings section
  fade, and all CSS `transition` properties on interactive elements (hover states, toggles, buttons).

### Keyboard Inputs

An on-screen overlay that displays every key, chord and (optionally) mouse click as it
  happens — paired with the human-readable name of the action the shortcut triggers.
  Designed for demos, screencasts, pair-programming sessions, and any moment someone
  needs to see what you just pressed and what it does.

Each captured chord is shown as an IDE-style key cap card with the modifier keys in
  accent and the printable key in solid. When the chord matches a known built-in or
  plugin binding, its action label slides in underneath — so viewers learn the
  shortcut *and* understand the command in a single glance. Rapid repeats
  collapse into a single card with a `×N` counter.

- **Show keyboard inputs** — master toggle. Can be flipped from anywhere
    with the global shortcut  `Alt`+ `Shift`+ `K`, which works
    even while a modal is open (useful when you suddenly decide to start recording).
- **Position** — pick one of five anchors via a live mini-window preview:
    top-left, top-right, bottom-left, bottom-center, bottom-right. The selected spot lights
    up in accent.
- **Size** — Small, Medium or Large. Scales every pill, key cap and counter
    proportionally.
- **Accent tone** — pick the colour applied to the side stripe, modifier
    keys and the `×N` counter. *Accent* follows your theme; *Neon*, *Aqua* and *Amber* are fixed bright hues good for screencasts; *Mono* drops the saturation entirely for a purely typographic look. Mouse-click
    badges always keep their cool-blue tone regardless of this setting.
- **Edge offset** — distance in pixels between the overlay and the anchored
    screen edge (8 – 120 px). Bump it up if the overlay is being covered by another tool's
    HUD, drop it down for a more flush look.
- **Compact layout** — collapses the chord and action label onto a single
    line separated by a thin dot, instead of stacking them vertically. Great for
    minimalist demos.
- **Show action label** — toggles whether the human-readable action name
    appears under (or next to, in compact mode) each chord. Turn off for a purely
    typographic look.
- **Visibility** — how long each keystroke stays on screen (0.5 s – 6 s).
    Rapid repeats of the same chord collapse into a single pill with a `×N` counter instead of stacking.
- **Opacity** — overall transparency of every pill (40 % – 100 %, default
    100 %). The card has a fully solid background by default so nothing underneath
    bleeds through. Lower the slider only if you deliberately want the overlay to fade
    into a busy screen.
- **Only show shortcuts** — hides plain printable keys so only chords with
    Ctrl, Alt, Shift or Meta show up. Great for tutorials that focus on commands rather
    than typing.
- **Capture while typing** — off by default, so the overlay stays quiet
    while you write commit messages. Modifier chords inside text fields (e.g. `Ctrl+S`) are always captured regardless.
- **Show mouse clicks** — adds a small badge for Left / Middle / Right
    clicks. Off by default.
- **Group rapid repeats** — when on, the same chord pressed within ~600 ms
    bumps a counter instead of pushing a new pill. Turn it off if you specifically want to
    show every individual press.
- **Try it out** — the settings panel has live preset buttons
    (*Ctrl + K*, *Esc*, *F5*, …) so you can preview the look without
    needing to actually press keys.

The overlay uses `pointer-events: none` and never preventDefaults any event,
  so clicks and keystrokes pass straight through to the underlying app. The capture
  listener detaches automatically the moment you toggle the feature off — no idle cost.
  No `backdrop-filter` or runtime blur is used anywhere in the overlay: it
  paints to solid colours only, so even at 60+ entries per second on the busiest
  CommitGraph there is zero per-frame compositor work for the background.
  All settings persist to `localStorage` and survive restarts.

### Graph

- **Commits per load** — how many commits are fetched each time the graph loads or is scrolled to the end (100 – 2000, default 500). Only applies when lazy-load pagination is on.
- **Show remote branches** — toggle remote-tracking refs (e.g. `origin/main`) in the lane graph.
- **Show tags** — toggle annotated and lightweight tags in the lane graph.
- **Lazy-load pagination** — when *on* (default), commits are loaded in batches as you scroll; when *off*, the entire repository history is loaded at once. Disable only on small repos — loading tens of thousands of commits at startup can be slow. Persisted to `~/.config/arbor/config.toml`.

### Diff & Stage

- **Diff algorithm** — Myers (default), Patience, or Minimal. Myers is a good general-purpose default; Patience tends to produce cleaner hunks on refactors; Minimal produces the smallest diff.
- **Context lines** — number of unchanged lines shown around each hunk (0 – 20, default 3).
- **View mode** — Unified (single column) or Split (side-by-side).
- **Word wrap** — wraps long lines instead of scrolling horizontally.
- **Show full file** — render the entire file with diff highlights instead of just changed hunks. Useful for reading a change in its full surrounding context. The same toggle is available as a button (file icon) in the diff viewer header. Persisted to `~/.config/arbor/config.toml` under `[diff]`.
- **Virtualization threshold** — when a file's diff has more than this many lines, the renderer switches to a windowed mode that only paints visible rows (default 200). Lower values keep huge files snappier; word wrap forces the simple renderer regardless.
- **Confirm before discarding** — when enabled (default), a confirmation dialog appears before discarding a single file's changes. The *Discard All* confirmation is always shown regardless of this setting.

### Diff viewer controls

The diff header carries a few extra controls on top of the Unified/Split toggle:

- **Chunk navigation** — `↑` / `↓` chevrons (with a *n/N* counter) jump between change blocks. `F3` / `Shift+F3` do the same from the keyboard.
- **Show full file** — the file icon mirrors the global setting; toggling it here rebuilds the visible diff immediately.
- **Auto-focus** — opening a file (or staging a line) lands the view on the first remaining change instead of the top of the file.

### Keybindings

Click any shortcut chip to record a new key combination. Press `Escape` while recording to cancel.
  Use the reset icon to restore a single binding to its default. **Reset all** restores every binding at once.

The **Plugins** group at the bottom of the list shows keybindings registered by plugins — these are read-only.

### Activity Bar

The Activity Bar can be customised without touching any setting panel.
  Click the **gear icon → Customize Activity Bar…** in the title bar to open the layout editor.

- **Visibility** — each item has an eye icon. Click it to show or hide the button.
    Items marked with a lock icon (*Branches*, *Stage*, *Detail*) are mandatory and cannot be hidden.
- **Order** — drag items by their handle to reorder them within their section.
    A blue indicator line shows the insertion point as you drag.
- **Two sections** — *Sidebar* (top half, controls which panel opens on the left)
    and *Panel* (bottom half, controls the bottom panel: stage, commit detail, terminal, jobs, pipelines).
    Items can only be reordered within their own section.
- **Plugin items** — actions, combo buttons, and separators registered by plugins also
    appear here and can be reordered or hidden like built-in items.

The layout is persisted to `~/.config/arbor/config.toml` and restored on next launch.
  Hidden items are still active in the background — hiding the Stage button does not disable staging.

## Git

### Git Flow

See the dedicated **Git Flow** section in the sidebar for full documentation.

### Experimental

Features that depend on external data, are still maturing, or may produce unexpected results
  in edge cases. All flags default to **on** and are stored in `localStorage` — they never alter local Git state and can be toggled at any time.

#### Squash-merge ghost edges

When a Pull Request or Merge Request is merged via *squash*, Git creates a single new
  commit on the target branch whose only parent is the previous tip — there is no topological link
  to the original feature commits. The commit graph therefore shows the feature branch as a dangling
  strand with no visible connection to the merge point.

When this flag is on, Arbor queries the GitHub / GitLab API on every graph load to retrieve the `merge_commit_sha` of each closed PR / merged MR, then draws a dashed ghost edge
  connecting that commit to the feature branch tip.

- **Ghost edge style** — semi-transparent dashed line (45 % opacity, `5 3` dash pattern) in the feature branch's lane colour.
- **Fallback anchor** — if the merge commit hasn't been fetched locally yet, the ghost edge connects the feature tip to the target branch tip *before* the merge. Once you `git fetch`, native edges appear and the ghost is suppressed automatically.
- **No token / no remote** — degrades silently; graph loads normally without ghost edges.
- **Performance** — adds one API call per graph load (up to 50 closed PRs/MRs). May add latency on repos with many closed PRs.

---

# Settings — Performance

## Cache

The cache stores each tab's graph, branch, CI/CD, and MR data in memory for the duration of the
  session. Switching to a tab whose data is already cached is **instant** — no round-trip
  to the backend is needed. Data is cleared when you close the app.

- **Enable cache** — master toggle. When off every tab switch re-fetches data from the backend. Useful for debugging.
- **Max cached tabs** — maximum number of tabs whose snapshots are kept simultaneously.
    When exceeded, the least-recently-used tab's snapshot is evicted (LRU). Default: 10.
- **Clear all** — discards every in-memory snapshot and commit-detail cache immediately,
    and evicts the backend stats and ticket-link caches for every tab. The next access
    re-fetches from the backend and repopulates the cache.

#### What is cached

- Commit graph (page 0)
- Local and remote branches, stashes, tags, submodules, nearest tag
- CI/CD provider info and run list
- Plugin pipeline definitions and runs
- Open MR/PR list
- Squash-merge ghost-edge hints
- Individual commit details (global cache by SHA — commits are immutable)

#### What is never cached

- Working-tree status (staged / unstaged files) — always fetched live
- File diffs — always fetched live (see *Lazy commit diffs* below)
- Issue tracker / ticket data
- Paginated graph pages beyond page 0 ("Load more")
- Graph loads with a file filter active

#### Lazy commit diffs

When you click a commit in the graph (or pick a stash), Arbor fetches only the **file list** with
  +/− stats first, then loads each file's hunks **on demand** as you open it in the diff viewer.
  Files you never click are never parsed. This keeps clicking a large commit responsive even when *Show full file* is on, because libgit2 only walks the bytes of files you actually look at.

Inside the visible diff, files with hunks not yet loaded show a small *Parsing…* badge in the
  file list. Selecting one queues its parse; clicking another commit before it returns discards the
  in-flight fetch so stale hunks never overwrite the new file list. The loaded hunks are kept in memory
  only for the currently selected commit — switching commits re-fetches metadata and parses on demand
  again.

#### Cache invalidation

The cache for a tab is discarded automatically after any write operation on that tab:
  committing, staging, discarding, checking out a branch, pushing, pulling, fetching,
  resetting, cherry-picking, rebasing, creating/deleting branches or tags, GitFlow operations,
  MR/PR mutations, and CI pipeline triggers.

The status bar shows a **last refreshed** timestamp (e.g. *2m ago*) next to
  the branch name, indicating when the cached data was last fetched from the backend.

## Memory Management

Controls whether evicting a tab's cache also frees the underlying git handle held by the backend.

- **Free git handle on eviction** — when enabled (default), dropping a tab's cache also
    releases the `git2::Repository` object. This frees libgit2's internal caches: pack-file
    indexes, loose-object cache, reference cache, and config cache. The repository is transparently
    re-opened the next time any command accesses that tab, with a small one-time latency (~50 ms).
    Disable this only if you notice lag when switching back to evicted tabs.

## Auto-Refresh Scheduler

The scheduler runs in the background and periodically checks whether the active repository has
  changed since the cache was last populated.

- **Enable scheduler** — toggle the background checker on or off.
- **Check interval** — how often the scheduler wakes up (seconds, minimum 5). Default: 60 s.
- **Focus-gated** — the scheduler only runs while the app window is focused. If you switch away and come back, it resumes from where it left off.

#### Change detection

On each tick, the scheduler calls `get_repo_fingerprint` — a lightweight command that reads the current HEAD SHA
  and all ref names from libgit2. Fingerprints are compared; when a change is detected the tab's cache is discarded and the
  graph reloads automatically.

## Idle Cache Eviction

Automatically frees memory by evicting the cache of background tabs that have not been accessed
  for a configurable amount of time. Useful when many repositories are open simultaneously for
  extended sessions.

- **Enable auto-eviction** — off by default. When enabled, a background scheduler periodically scans all cached tabs and discards those that have been idle too long.
- **Minimum tabs to keep** — the N most-recently-used tabs are always kept in cache,
    regardless of idle time. The currently active tab counts toward this total. Default: 1 (active tab only).
    Set to 3 to always keep the active tab plus the 2 most recently visited ones.
- **Idle threshold** — seconds of inactivity before a tab's cache is cleared (minimum 30, default 300 s / 5 min). The timer is reset every time you switch to a tab or its data is accessed.
- **Check interval** — how often the eviction scheduler runs (minimum 10 s, default 60 s). A shorter interval means more responsive eviction at a negligible CPU cost.

#### Eviction scope

When a tab is evicted all three layers are cleaned:

- **Frontend** — the in-memory `TabSnapshot` (graph, branches, CI, MR, pipeline data), the commit-detail cache, and the fingerprint baseline are removed.
- **Backend** — the stats cache (`RepoStats` computation result) and the ticket-link cache for that tab are cleared.
- **git2 handle** — the `Repository` object is dropped (if "Free git handle on eviction" is enabled), freeing libgit2 internal memory.

#### Protected tabs

The *minimum tabs to keep* most-recently-used tabs are always excluded from eviction.
  Switching to a tab resets its idle timer and moves it to the top of the recency list immediately.

## Repository Browser

The Repository Browser ships with a separate, persistent cache layer because listing every
  repo for an account against the GitHub or GitLab API is slow on large accounts (200+ projects).
  Unlike the per-tab cache above, this cache lives in `localStorage` and survives
  app restarts.

- **Cache TTL** — how long a fetched repo list stays valid (seconds, default 600 = 10 min).
    Within the TTL, opening the modal returns the cached list without a network call. Past the TTL the
    cached list is still shown immediately and a fresh fetch runs in the background; the strip in the
    modal flips from *Cached* to *Updated* once it completes. Set to `0` to
    disable caching entirely.
- **Clear repo browser cache** — wipes the on-disk cache for every connected provider.
    The next open re-fetches from the API.

#### Backend pagination is now parallel

The repo-listing backend was rewritten to fetch pages 2..N concurrently (it used to walk them
  sequentially). For 200+ repos that alone collapses the cold-load time from ~30s into a handful
  of seconds. GitLab's `statistics=true` flag was also dropped — it forced the API to
  compute repo size for every project, and the list view doesn't display sizes anyway.

---

# Settings — Access

The **Git & Integrations** section consolidates Git host accounts, credentials, and issue tracker connections into a single place.
  All secrets are stored in the OS keychain (Windows Credential Manager, macOS Keychain, libsecret on Linux).

## Git Providers (GitHub / GitLab)

Each provider card has a split **Connect** button. Click the main button to connect via OAuth,
  or click the **▾** chevron to pick a different method:

- **GitHub OAuth — Device Authorization Grant (RFC 8628)**.
    Arbor calls GitHub to obtain a *user code*, opens `https://github.com/login/device` in your default browser, and shows the code in the panel.
    Copy or click the open-page button, paste the code on github.com, and approve.
    Arbor polls the token endpoint in the background and stores the access token in the OS keychain.
    No callback server, no client secret — the flow uses only the public `client_id`.
- **GitLab OAuth — Authorization Code + PKCE**.
    Arbor starts a one-shot local callback server on `127.0.0.1:7731`, opens the GitLab
    authorization page, exchanges the returned code for a token, and stores it in the OS keychain.
- **Personal Access Token** — paste a PAT directly. Stored in the keychain and used for HTTPS operations.
- **Username + Password** — store a username and password/token pair. Used automatically for fetch, pull, and push.

For self-hosted GitLab, check **Self-hosted** and enter your instance hostname before saving — or use the **Advanced** panel below to point the OAuth flow at your private GitLab instance.

### Connected-user badge

Once a GitHub or GitLab connection settles into *connected*, the
  card replaces the raw `client_id` blob with a compact **user badge**: avatar, display name, and a secondary line
  (email or `@login`). Each line is click-to-copy — a tick
  flashes in place of the icon to confirm the copy. The badge is rendered
  by the shared `ProviderUserBadge` widget, so Linear / Jira /
  Atlassian connections (when the provider exposes `/me`) all
  get the same treatment with no per-card boilerplate.

Data comes from new `get_github_user` / `get_gitlab_user` IPCs that call the provider's `current_user()` on connect; if
  the call fails (revoked token, offline) the badge silently falls back to
  the connection summary — no error toast for what is purely cosmetic.

## Advanced — use your own OAuth application

Each OAuth provider card has an **Advanced — use my own OAuth app** toggle that expands an
  override panel.  Use it when:

- You forked Arbor and want OAuth tokens issued under your own GitHub / GitLab / Linear / Atlassian app.
- You're behind a corporate proxy that requires a captive client.
- You're connecting to a **self-hosted GitLab** instance that issues its own OAuth applications (set both `client_id` and `base_host`, e.g. `gitlab.company.com`).

Overrides are persisted in plain TOML at `~/.config/arbor/config.toml` under `[oauth.<provider>]`.
  The OAuth `client_id` is a public identifier (RFC 6749 §2.2) and is intentionally not stored in the keychain — only access and refresh tokens are.
  Leave a field empty to fall back to Arbor's bundled default.

Redirect / callback hints when registering your own app:

- **GitHub** — Device Flow only.  Enable *Device Flow* in your OAuth App settings.  No callback URL needed.
- **GitLab** — Redirect URI: `http://127.0.0.1:7731/callback`, scope `api`, *Confidential* off (PKCE replaces the secret).
- **Linear** — Redirect URI: `http://127.0.0.1:7729/callback`, public client.
- **Jira / Atlassian** — Redirect URI: `http://127.0.0.1:7730/callback`, scopes `read:jira-work write:jira-work offline_access read:me`.

Changing the `client_id` invalidates any refresh token obtained with the previous one — you'll be re-prompted to authorise the new app on the next refresh attempt.

## Additional Git Credentials

The **Additional Git Credentials** card lets you store credentials for other hosts
  (Bitbucket, Azure DevOps, custom Git servers). Select a provider preset or choose *Custom…* and enter the host manually.

## Issue Trackers (Linear / Jira)

Each tracker card uses the same split **Connect** button pattern — click the main button for the default method or **▾** for alternatives.

### Linear

- **OAuth (recommended)** — Authorization Code + PKCE with a localhost callback server on port 7729.
    Arbor ships with a bundled OAuth app — just click **Authorize** and Arbor opens the browser and completes the flow automatically.
    To use your own OAuth app instead, expand **Advanced — use my own OAuth app** and set the `client_id` (register a *Public* app at `linear.app → Settings → API → OAuth applications` with `http://127.0.0.1:7729/callback` as redirect URI).
- **Personal API Key** — generate at `linear.app → Settings → API → Personal API keys` and paste directly.

### Jira

- **API Token — Jira Cloud** — generate at `id.atlassian.com → Security → API tokens`.
    Enter your subdomain (the part before `.atlassian.net`), email, and the token.
- **Personal Access Token — Jira Data Center / Server** — generate at `Jira → Profile → Personal Access Tokens`.
    Enter the full hostname as subdomain (e.g. `jira.internal.example.com`), your email, and the PAT.
    Self-signed and internal-CA certificates are accepted automatically.
- **OAuth 2.0 (3LO) — Jira Cloud only** — click **Connect ▾ → OAuth 2.0** and follow the browser prompt.
    Arbor discovers your Cloud site automatically and stores access + refresh tokens in the OS keychain.
    To use your own Atlassian OAuth 2.0 (3LO) app, expand **Advanced — use my own Atlassian OAuth app** on the Jira card and set the `client_id` (register at `developer.atlassian.com → OAuth 2.0 (3LO)` with `http://127.0.0.1:7730/callback` as redirect URI).

See the **Issues (Linear / Jira)** section for the full compatibility table, sidebar filters, and plugin API.

---

# Settings — Project

## IDE Integration

Configure which IDE Arbor uses when opening a worktree or repository folder.
  All settings are stored in `~/.config/arbor/config.toml`.

### Default IDE

The IDE used when no language-specific default applies. Shown as *Default* badge
  in the worktree context menu.

### IDE by Language

Override the default IDE for a specific project type. For example, set **RustRover** for Rust and **WebStorm** for Node.js — the correct IDE will be highlighted and
  pre-selected automatically when right-clicking a worktree.

Supported project types: Rust, Node.js, Java (Maven), Java (Gradle), Go, Python, .NET, C++, Ruby, PHP.
  Leave a language row set to *— Use default —* to fall back to the global Default IDE.

### Executable Paths

Each built-in IDE entry shows a status dot (green = detected, grey = not found) and an optional
  path override field. Use the override if:

- The IDE executable is not in `PATH` (common on Windows for JetBrains IDEs).
- You have multiple versions installed and want to pin a specific one.
- The default command name doesn't match your installation (e.g. a custom build).

Click the folder icon to browse for the executable, or type the path directly. Changes apply after **Save**.

### Custom IDEs

Register any editor not in the built-in list. Each custom entry requires:

- **ID** — unique identifier (e.g. `emacs`). Used internally.
- **Name** — display name shown in menus.
- **Command** — executable to launch (absolute path or `PATH`-resolvable name).
- **Args** — optional extra arguments passed before the target path (space-separated).

Custom IDEs appear in the worktree context menu alongside built-in ones and can be set as the default.

### IDE Detection

At startup, Arbor probes each built-in IDE in the background via `which` / `where`.
  This runs as a non-cancellable background job (**System → IDE Detection**) so it never
  blocks the UI. Results populate the Executable Paths status dots and the worktree context menu.

- Detection runs **once per session**. Closing and reopening Settings does not re-trigger it.
- Click **Re-detect** (or press **Save**) to run a new detection pass — useful after installing an IDE mid-session or changing a path override.
- IDEs with an explicit path override are checked directly (file existence) — no `which` call needed.

## Terminals

Configure the integrated terminal panel: which shells appear in the **+** picker,
  which one opens by default, and where each executable lives. Settings are stored under `[terminals]` in `~/.config/arbor/config.toml`.

### Default Shell

The shell opened by the bare **+** button in the terminal panel. Leave on *— platform default —* to fall back to `cmd.exe` on Windows and `bash` on Linux/macOS. Any built-in or custom shell can be set as default by
  clicking the check icon on its row.

### Detected Shells

Each built-in shell shows a status dot:

- **Green** — found in `PATH` or at a well-known install location.
- **Grey ✕** — not detected. The shell is hidden from the terminal picker.
- **Grey dot** — detection has not finished running yet.

Use the **path override** field if a shell is installed in a non-standard location,
  or to pin a specific version when several are available. Click the folder icon to browse, or
  paste the absolute path. Saving the form re-runs detection automatically when paths change.

### Custom Terminals

Register any executable as a terminal entry. Each custom shell needs:

- **ID** — unique identifier (e.g. `dev-container`).
- **Display name** — label shown in the picker.
- **Command** — executable to launch (absolute path or `PATH`-resolvable name).
- **Args** — optional arguments passed on spawn (space-separated).

Custom terminals are *always* shown in the picker (they don't go through the detection
  probe — you defined them on purpose) and can be set as the default shell.

### Shell Detection

At startup Arbor probes each built-in shell in the background via `which` / `where`, with fallback paths for shells that don't add themselves to `PATH` (Git Bash, WSL, MSYS2, Cygwin, PowerShell 7). Detection runs as a non-cancellable background
  job — see **System → Shell Detection** in the Jobs overlay.

- Detection runs **once per session**. Use **Re-detect** after
      installing a new shell to refresh without restarting.
- Shells with an explicit path override are checked directly (file existence) rather than via `which`.
- Platform-irrelevant shells are filtered out: `cmd`/`powershell`/ `wsl` never appear on Linux, `zsh`/`tcsh`/`sh` never appear on Windows.

## Repository

Per-project overrides stored in `.arbor/config.toml` alongside the repository. Requires an open repository to edit.

- **Commit template** — pre-fills the commit message field when empty. Git's native `commit.template` takes priority if set; otherwise this template is used.
    Stored in `~/.config/arbor/config.toml` (applies to all repos).
- **Display name** — friendly name shown in the tab bar instead of the folder name.
- **Default remote** — remote used for fetch/pull/push when not specified. Defaults to `origin`.
- **Author identity override** — sets `user.name` / `user.email` for commits in this repo only. Leave blank to use the global Git identity.

## Issue Tracker

Per-project issue tracker and ticket link settings. All values are stored in `.arbor/config.toml`.

### Provider

Select which issue tracker (Linear or Jira) to use for this repository's Issues sidebar and Ticket Picker.
  Changing this also resets the default project filter. Connect credentials first in **Access → Issue Trackers**.

### Default Project Filter

When set, the Issues sidebar and Ticket Picker automatically pre-filter issues to the chosen project every time this
  repository is active. The user can still override the project from the filter bar at any time.

- Click the project combobox to see all projects available in the connected tracker.
- Select *— All projects —* to clear the default filter.
- Use the **↺** refresh button to reload the project list from the tracker.
- The selected project ID is stored as `issue_tracker_project_id` in `.arbor/config.toml`.

### Ticket Links — Custom Pattern

Override the default ticket ID regex for this repository. Leave blank to use the tracker default
  (`[A-Z][A-Z0-9]*-\d+` for Linear/Jira, `#\d+` for GitHub/GitLab).
  The pattern must contain exactly one capture group, e.g. `\b(MYCO-\d+)\b`.
  See the **Ticket Links** section for full documentation.

---

# Themes

Open the Theme Editor from **Settings → Appearance → Open Theme Editor**.
  Every colour, shadow and terminal palette in Arbor is driven by CSS custom properties
  exposed in the editor — change one, every panel updates live.

## Built-in themes

- **Dark** — JetBrains-inspired default.
- **Light** — high-contrast daytime variant.

Built-ins are read-only. Use the *clone* icon in the sidebar to fork one
  into your custom list, then edit freely.

## Importing themes

The *Import* button in the editor header opens the in-app file picker.
  Select one or many `.json` files (Ctrl/⌘+click to add, Shift+click for
  a range) and confirm — every file is parsed independently and each successful
  import becomes a new custom theme. Failures are surfaced via toast and the dev
  console without aborting the rest of the batch.

Imported themes always receive a freshly-generated `custom-*` id so they
  never clash with built-ins or other customs already on disk.

## Bundled presets

Arbor ships a small library of community-favourite palettes as plain JSON files
  in the `themes/` directory at the project root — exactly the same format
  the importer accepts. Browse there with the file picker and pick whichever you
  like; multi-select is supported, so installing the whole pack is one click.

### Dark

- `themes/tokyo-night.json`
- `themes/tokyo-night-storm.json`
- `themes/caffeine.json`
- `themes/dracula.json`
- `themes/monokai.json`
- `themes/gruvbox-dark.json`
- `themes/nord.json`
- `themes/solarized-dark.json`
- `themes/catppuccin-mocha.json`
- `themes/catppuccin-macchiato.json`
- `themes/catppuccin-frappe.json`
- `themes/one-dark.json`
- `themes/ayu-dark.json`
- `themes/rose-pine.json`
- `themes/rose-pine-moon.json`
- `themes/github-dark.json`
- `themes/kanagawa.json`

### Light

- `themes/tokyo-night-day.json`
- `themes/catppuccin-latte.json`
- `themes/one-light.json`
- `themes/solarized-light.json`
- `themes/gruvbox-light.json`
- `themes/ayu-light.json`
- `themes/rose-pine-dawn.json`
- `themes/github-light.json`

Once imported, presets become regular custom themes — edit, duplicate, export, or
  delete them like any other entry. Drop additional `.json` files into `themes/` to share your own palettes alongside the bundled ones.

## Exporting themes

Select any theme in the sidebar and hit *Export* in the header. The save
  dialog (the in-app picker, never the OS one) suggests a filename based on the
  theme id; pick a destination and you'll have a portable JSON file you can share,
  back up, or version-control.

## Theme JSON schema

A theme file is a small JSON document. Only `name` and `vars` are required; everything else is optional. The `vars` map keys are CSS
  custom properties that are written verbatim onto `:root`.

```
{
  "id":          "preset-tokyo-night",
  "name":        "Tokyo Night",
  "description": "A clean, dark theme inspired by Tokyo at night",
  "vars": {
    "--bg-base":      "#1a1b26",
    "--bg-elevated":  "#24283b",
    "--accent":       "#7aa2f7",
    "--text-primary": "#c0caf5",
    "--terminal-bg":  "#1a1b26",
    "...":            "..."
  }
}
```

Any non-string value or key not starting with `--` is dropped silently.
  Unknown variable names are accepted and written to the document — Arbor's own
  styles will simply ignore them, so it's safe to ship themes with extra tokens
  for plugins that consume their own variables.

## Beyond colours

A theme can also customise a few non-colour aspects of the UI. All of these
  are optional — themes that don't declare them inherit the global defaults.

### Geometry

- `--radius-sm / --radius-md / --radius-lg` — corner radius scale. Sharp Solarized uses `2/3/5px`; rounded Catppuccin uses `5/8/12px`; default is `4/6/10px`.
- `--scrollbar-width` — thumb width in pixels (defaults to `6px`; some themes prefer 7-8 px for a chunkier feel).
- `--scrollbar-radius` — thumb radius (default `999px` = pill; Solarized uses `2px` for a square scrollbar; Monokai uses `0` for utterly square).

### Selection feel

- `--selection-strength` — multiplier (0.5–1.5) applied to the alpha of text-selection backgrounds. `1` is neutral, light pastel themes use `0.7-0.8`, Dracula-style vivid themes go up to `1.3`.

### Typography (opt-in)

Themes can *suggest* a UI / code font, but the override is only applied
  when the user enables **Use theme fonts** in the editor header.
  Set `--theme-font-ui` and / or `--theme-font-code` to a
  CSS font-family stack:

```
"--theme-font-code": "'Hack', 'JetBrains Mono', ui-monospace, monospace"
```

The toggle is global and persisted; turning it off (default) restores the
  user's preferred font stack regardless of which theme is active. This means
  a theme can publish a canonical font without ever silently overriding what
  someone has installed.

## Storage

- Custom themes live as individual files at `~/.config/arbor/themes/<id>.json`.
- The active theme id is persisted in `~/.config/arbor/config.toml` under `[theme]`.
- Bundled presets live in the project repo at `themes/*.json` — they are imported on demand, not auto-loaded.

---

# Plugin Development â€” Basics

Arbor embeds **Lua 5.4** via the `mlua` crate. Plugins live in `plugins/<name>/` next to the executable and need only a `plugin.toml` manifest plus an entry-point Lua file.

| Field | Value |
| --- | --- |
| Runtime | Lua 5.4 (vendored) â€” no system Lua needed |
| Manifest | `plugin.toml` â€” required |
| Entry point | `main.lua` by default; override with `entry` |
| API version | Declare minimum required via `arbor_api` |
| Sandbox | `require()` scoped to the plugin dir; dangerous stdlib removed |

## Directory layout

```
plugins/
  my-plugin/
    plugin.toml       â† manifest (required)
    main.lua          â† entry point (default; override with entry = "â€¦")
    doc.html          â† optional: HTML docs shown in this panel under Plugins
    lib/utils.lua     â† require("lib.utils") works inside the plugin sandbox
    config/
      global.lua      â† optional sub-modules
```

## Installing & sharing plugins

Open the **Plugin Manager** (Activity Bar â†’ puzzle icon) â€” the top-right toolbar exposes two shortcuts that avoid hand-editing files on disk:

- **Import from .zip** — The Upload icon opens a file picker; pick a plugin archive and Arbor extracts it into plugins/<name>/, then reloads. The zip must contain a top-level plugin.toml (either at the archive root or inside a single wrapping folder). Existing folders with the same name are refused â€” delete the old copy first. Imported plugins land disabled by default â€” review the manifest's [permissions], then click the Power icon on the plugin card to enable.
- **Export Template wizard** — The Wand icon opens a 4-step wizard (Identity â†’ Permissions â†’ Hooks â†’ Recipes) that scaffolds a starter plugin and saves it as a zip. Each toggled recipe (command palette entry, keybinding, settings panel, modal form, toolbar action, sidebar, notification, background job, scheduler, HTTP) injects a canonical Lua snippet into main.lua. The bundle ships with sdk.d.lua + .luarc.json so lua-language-server provides arbor.* autocomplete in any editor.

> ℹ Once exported, unzip into `plugins/<name>/` and click **Reload** in the Plugin Manager â€” or hand the zip to another user and have them *Import* it.

## Managing installed plugins

The Plugin Manager lists every plugin discovered in `plugins/`. Each row exposes a fixed action column on the right (left â†’ right):

| Icon | Action | Behaviour |
| --- | --- | --- |
| âš™ï¸ Settings | Open the plugin's settings panel | Visible only when the plugin registered a settings container via arbor.ui.settings.panel(...). Disabled while the plugin is off. |
| â„¹ï¸ Info | Open the Plugin Info modal | Detailed read-out of identity, permissions, hooks, schedulers + maintenance actions (see below). |
| ðŸ—‘ Uninstall | Permanently remove the plugin | Deletes the plugins/<name>/ folder, the global plugin_data/<name>/ store, every per-repo .arbor/plugins/<name>/, and the persisted enable-state. Shows a cascade-warning modal if other enabled plugins still depend on it. |
| â» Power | Enable / Disable | Persisted across restarts. Disabling stops every scheduler, fires on_plugin_unload, and closes any sidebar / panel that the plugin owned. Re-enabling reloads the plugin and re-fires on_plugin_load. |

### Master kill-switch

The toggle at the top of the modal â€” *Abilita gestione plugin* â€” is the **master kill-switch**. While it is off, the runtime is empty: nothing is loaded at startup, no schedulers fire, the contribution registry is wiped, and the per-plugin list is hidden until the switch is flipped back on. Useful for diagnosing whether a misbehaving plugin is the cause of an issue.

### Plugin Info modal

Opened from the **Info** icon. Six grouped sections:

- **Identity** — Name, version, author, license, declared arbor_api, repository link (clickable, opens in the system browser) and keyword chips.
- **Schedulers** — One row per arbor.scheduler.register call with the action name, trigger summary (every 5m, cron(â€¦), â€¦) and a per-action toggle. The header row exposes Enable all / Disable all bulk buttons. Toggling a single schedule calls start_plugin_scheduler / stop_plugin_scheduler on the backend; the change is in-memory only â€” restarting Arbor re-applies the manifest defaults.
- **Permissions** — Coloured pills (safe / warn / danger) for filesystem scope, network allow-list, git capability tier and terminal access â€” same chips shown when reviewing imported plugins.
- **Hooks** — Lists every [hooks] entry the manifest opted into so reviewers can see at a glance which lifecycle events the plugin observes.
- **Maintenance Â· Open settings** — Shortcut to the plugin's settings container without first closing the modal. Disabled when the plugin is off or hasn't registered one.
- **Maintenance Â· Clear settings cache** — Two-step destructive button (first click arms it red, second click confirms). Wipes every persisted setting written by this plugin (global + per-repo) â€” the plugin's own folder and code stay untouched. Use after schema breaking changes or to reset a misbehaving config.

> ℹ The Info modal stays in sync with backend events â€” reloading plugins or toggling the master switch refreshes its content automatically.

## plugin.toml

```toml
[plugin]
name        = "my-plugin"
version     = "0.1.0"
description = "What it does"
author      = "You"
license     = "MIT"
repository  = "https://github.com/you/my-plugin"
keywords          = ["git", "tool"]
min_arbor_version = "0.1.0"  # optional; rejects plugin on older builds (semver)
arbor_api         = 1        # minimum Arbor plugin API version required
os                = []       # ["windows", "linux", "macos"] â€” empty = cross-platform
entry             = "main.lua" # default; can be changed
doc_file          = "doc.html" # optional: HTML file shown in the Docs panel

[permissions]
network              = []          # allowed hostnames for arbor.http.get
fs                   = "none"      # none | read | write
fs_scope             = []          # [] = sandboxed to the active repo; ["*"] = unrestricted; otherwise extra allowed paths
git                  = "none"      # none | read | write | history_rewrite
issues               = "none"      # none | read | write
toolchain            = "none"      # none | read | write
terminal             = "none"      # none | commands | any
terminal_scope       = []          # allowed command basenames when terminal = "commands"
# env_read accepts: true (all vars) | false (no os.getenv) | allowlist of names
env_read             = ["PATH", "JAVA_HOME"]
# service_call         = false    # arbor.service.call â€” invoke services from other plugins
# service_export       = false    # arbor.service.export â€” expose callable services
# settings_read_others = false    # arbor.settings.read other plugins' globals

[hooks]
on_plugin_load   = true   # fires once after main.lua executes (init/constructor)
on_plugin_unload = true   # fires when Arbor shuts down (cleanup)
on_repo_open     = true   # fires when a repo tab becomes active
on_repo_close  = true   # fires when a repo tab is closed
on_repo_init   = true   # fires when a new repo is initialized from a non-git folder
on_tab_switch  = true   # fires on every tab switch
on_commit        = true
on_push          = true
on_pull          = true
on_checkout      = true
on_fetch         = true
on_branch_create = true
on_branch_delete = true
on_branch_rename = true
on_tag_create    = true
on_tag_delete    = true
on_stash_push    = true
on_stash_pop     = true
on_rebase_start  = true
on_rebase_abort  = true

# Background scheduler â€” opt-in only. Schedule data (action, trigger,
# focus gating, â€¦) is declared from main.lua via arbor.scheduler.register.
[scheduler]
enabled = true

# Settings UI is no longer declared in plugin.toml. Plugins register a
# panel at runtime via `arbor.ui.settings.panel(...)` â€” see Plugin
# Development â†’ API: UI for the contribution-based settings model.
```

## Plugin documentation (doc.html)

Set `doc_file = "doc.html"` in your manifest to expose plugin-specific documentation under the **Plugins** group in the left nav. Plain HTML â€” styles from the host docs apply automatically.

### Supported elements

| Tag | Renders as |
| --- | --- |
| <h1> | Section title (large, bottom border) |
| <h2> | Sub-heading (small caps, accent) |
| <h3> / <h4> | Tertiary / quaternary heading |
| <p> Â· <ul> Â· <ol> | Body text and lists |
| <strong> | Bold, primary text colour |
| <code> Â· <pre><code> | Inline / block monospace |
| <kbd> | Keyboard key chip |
| <table> | Styled data table |

> ℹ CSS variables like `var(--accent)`, `var(--text-secondary)`, `var(--bg-overlay)` are available for custom inline styling.

```
<!-- doc.html example -->
<h1>my-plugin</h1>
<p>Short description of what the plugin does.</p>

<h2>Getting Started</h2>
<ol>
  <li>Open a repo â€” the plugin activates automatically.</li>
  <li>Click <strong>â–¶</strong> in the Activity Bar to run.</li>
</ol>

<h2>Permissions</h2>
<table class="shortcuts-table">
  <thead><tr><th>Permission</th><th>Why</th></tr></thead>
  <tbody>
    <tr><td><code>fs = "read"</code></td><td>Reads config files in repo.</td></tr>
  </tbody>
</table>
```

## main.lua skeleton

```lua
-- main.lua â€” thin wiring file
-- Register UI elements, subscribe to hooks. Keep logic in sub-modules.

local state = require("state")        -- sub-module inside this plugin dir

-- â”€â”€ Lifecycle â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
-- on_plugin_load fires once AFTER main.lua finishes executing.
-- Ideal for one-time initialisation (load settings, register combos, etc.)
arbor.events.on("on_plugin_load", function(ctx)
  arbor.log.info("loaded â€” api_version=" .. ctx.api_version)
  state.init()
end)

-- â”€â”€ Hooks â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
arbor.events.on("on_repo_open", function(ctx)
  state.set_repo(ctx.repo)
  arbor.log.debug("repo_open: " .. ctx.repo)
end)

arbor.events.on("on_commit", function(ctx)
  arbor.notify{ message = "Committed: " .. ctx.message, level = "success" }
end)

arbor.events.on("on_branch_rename", function(ctx)
  -- ctx.tab_id   : string  â€” the repository tab
  -- ctx.old_name : string  â€” previous branch name
  -- ctx.new_name : string  â€” new branch name
  arbor.log.info("Branch renamed: " .. ctx.old_name .. " -> " .. ctx.new_name)
end)

-- â”€â”€ UI registrations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
arbor.ui.add_context_menu_item({ target = "commit", label = "Inspect", action = "my_plugin:inspect", icon = "Search" })
```

## require() sandbox

`require()` inside a plugin is sandboxed to the plugin directory. Dots in the module name are converted to path separators (`require("lib.utils")` â†’ `plugins/my-plugin/lib/utils.lua`). Path traversal attempts (`../`) raise a Lua error. Standard Lua packages (`string`, `table`, `math`, `os`) are always available.

## Multi-file plugin layout (recommended)

```
plugins/compile-action/
  plugin.toml
  main.lua              â† thin wiring: require sub-modules, register hooks/UI
  state.lua             â† shared mutable state (current repo, running job IDs)
  detect.lua            â† project type auto-detection (Maven/Gradle/npm/â€¦)
  defaults.lua          â† default build configs per project type
  run_defaults.lua      â† default run configs per project type
  config/
    global.lua          â† global build settings CRUD + form
    project.lua         â† per-repo build settings CRUD + form
    run_global.lua      â† global run settings CRUD + form (+ auto_stop global default)
    run_project.lua     â† per-repo run settings CRUD + form (+ tomcat_home, auto_stop override)
    jdk.lua             â† JDK registry (shared by build + run)
  ui/
    combo.lua           â† build combo (Hammer icon)
    run_combo.lua       â† run combo (Play icon)
```

```lua
-- main.lua
local state     = require("state")
local combo     = require("ui.combo")
local run_combo = require("ui.run_combo")

arbor.events.on("on_plugin_load", function(ctx)
  combo.register()      -- ðŸ”¨ Build combo (right)
  run_combo.register()  -- â–¶  Run combo (left)

  arbor.keybinding.register({ key = "F9", action = "compile:run", description = "Build selected" })
  arbor.keybinding.register({ key = "F5", action = "run:run",     description = "Run selected"   })
end)

arbor.events.on("on_repo_open", function(ctx)
  state.set_repo(ctx.path)
  combo.refresh()
  run_combo.refresh()
end)
```

## Plugin dependencies

A plugin can declare that it requires another plugin (for example, because it publishes an event on the bus that the other plugin reads). Add one `[[dependencies]]` entry per required plugin in your `plugin.toml`:

```toml
[[dependencies]]
name     = "compile-action"
version  = ">=1.0.0"   # semver requirement; empty = any version
optional = false        # when true, a mismatch is a warning, not an error

[[dependencies]]
name     = "auto-fetch"
version  = "^0.2.0"
optional = true
```

Accepted semver operators: `=`, `>`, `>=`, `<`, `<=`, `~`, `^`, plus exact versions (`1.2.3`) and wildcards (`1.*`).

### Load ordering & errors

- **Topo-sort**At startup all manifests are topologically sorted so each plugin loads *after* its dependencies. Cycles are rejected with a descriptive error; involved plugins show greyed-out in the Plugin Manager.
- **Unmet dep**Missing or version-mismatched dependency â†’ plugin skipped, red banner on the card.
- **Optional**`optional = true` downgrades the error to a log warning. Your plugin still loads â€” guard calls that depend on the other plugin's presence.

### Dependency graph & cascade warnings

The Plugin Manager exposes a **Network** icon opening the *Plugin Dependency Graph* modal. Each plugin row reveals:

- **Depends on** — Plugins your plugin declares, with version requirements and optional/unmet tags.
- **Required by** — Plugins that currently depend on yours â€” follow arrows backward to see who'd break if you disabled it.

> ℹ Disabling a plugin that others require shows a **cascade-warning** modal listing affected dependents; explicit confirmation is required.

## Permissions reference

Declared once in `[permissions]` of `plugin.toml`. Capability is enforced at Lua call-time â€” trying to use a disabled API raises a runtime error.

| Key | Value | Enables |
| --- | --- | --- |
| network | string[] | Allowed hostnames for arbor.http.get. Exact match or registrable suffix ("maven.org" permits repo1.maven.org and itself). Use ["*"] for any host. Empty list = no network. |
| fs | "none" default | No arbor.fs.* access |
| "read" | Read-only filesystem ops (read / list / glob / exists / is_file / is_dir) |
| "write" | Read + write (write / append / touch / move / delete / copy / json_set / yaml_set / toml_set) |
| fs_scope | [] default | Sandboxed to the active repo's directory. Use ["*"] instead when the plugin writes to user-picked paths via arbor.ui.pick_file{ mode = "save" } â€” the sandbox would otherwise reject anything outside the repo (e.g. ~/Downloads/foo.md). |
| ["*"] | Unrestricted â€” any path on disk |
| ["/abs/path", â€¦] | Allow these absolute paths in addition to the active repo |
| git | "none" default | No arbor.repo.* / arbor.notes.* access |
| "read" | arbor.repo.current / branch / is_dirty / remote / branches / tags + arbor.notes.list / get |
| "write" | Read + non-destructive writes (fetch_active_tab, clone, notes.set / delete) |
| "history_rewrite" | Write + destructive history ops (rebase, reset --hard, force-push, amend, filter-branch). Granted separately because these can permanently destroy work. |
| issues | "none" default | No arbor.issues.* access |
| "read" | arbor.issues.search(), arbor.issues.get() |
| "write" | Read + arbor.issues.transition(), arbor.issues.comment() |
| provider | "none" default | No arbor.mr.* / arbor.ci.* access |
| "read" | arbor.mr.list, arbor.mr.current_user, arbor.ci.runs â€” credential-blind: tokens stay in the OS keyring |
| "write" | Reserved for future MR/CI mutations (create / comment / retrigger) |
| toolchain | "none" default | No arbor.toolchain.* access |
| "read" | list, active, env, detect |
| "write" | Read + add, remove, set_active |
| terminal | "none" default | No arbor.terminal.exec() |
| "any" | Any command allowed |
| "commands" | Only basenames listed in terminal_scope allowed |
| env_read | true default | os.getenv() reads any environment variable |
| false | os.getenv is removed from the sandbox |
| ["PATH", "JAVA_HOME"] | Allowlist â€” only listed names return a value, others return nil |
| service_export | bool | arbor.service.export / unexport / list_own â€” expose callable services |
| service_call | bool | arbor.service.call / list â€” invoke services from other plugins |
| settings_read_others | bool | arbor.settings.read(plugin, key) â€” read other plugins' globals (own settings always readable) |

> **Sandbox hardening** These Lua functions are removed from the sandbox: `os.execute`, `os.exit`, `os.remove`, `os.rename`, `io.*`, `load`, `loadfile`, `dofile`. The `terminal` permission is captured at plugin load time â€” it cannot be escalated by overwriting a Lua global.

---

# Plugin Development â€” Hooks & Events

Declare which hooks your plugin subscribes to via boolean flags in `[hooks]`.
  Register handlers in Lua with `arbor.events.on("hook_name", fn)`. The full
  hook catalog (with the ctx schema for each one) is also browseable at runtime via `arbor.hooks.list()` and `arbor.hooks.describe(name)`.

## String enums used by the API

```lua
-- arbor.notify level
"info" | "success" | "warning" | "error"     -- default "info"

-- arbor.log.LEVELS â€” autocomplete-friendly aliases for the bare strings
arbor.log.LEVELS.DEBUG  -- "debug"
arbor.log.LEVELS.INFO   -- "info"
arbor.log.LEVELS.WARN   -- "warn"
arbor.log.LEVELS.ERROR  -- "error"

-- Manifest enum strings (used only inside plugin.toml â€” not at runtime)
-- terminal: "none" | "commands" | "any"
-- fs:       "none" | "read" | "write"
-- git:      "none" | "read" | "write" | "history_rewrite"
-- form variants: "default" | "primary" | "danger" | "ghost"
```

## Hooks reference

| Hook (TOML key & event name) | Context fields |
| --- | --- |
| â”€â”€ Lifecycle â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_plugin_load | plugin_name, dir, api_version |
| on_repo_open | tab_id, path, name |
| on_repo_close | tab_id, path, name |
| on_repo_init | path, name, default_branch, provider, remote_url, has_readme, license, gitignore |
| on_repo_deregistered | repo_id, path, name, reason |
| on_project_missing | repo_id, path, name, reason ("missing" \| "unreachable" \| "not_a_repo") |
| on_project_relocated | repo_id, old_path, new_path, name, remote_url |
| on_tab_switch | tab_id |
| â”€â”€ Git operations â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_pre_commit | tab_id, message, amend â€” vetoable (return a string to block) |
| on_commit | tab_id, oid, message, amend |
| on_push | tab_id, remote, refspec, force |
| on_pull | tab_id, remote |
| on_fetch | tab_id, remote |
| on_checkout | tab_id, branch or oid (detached) |
| â”€â”€ Branch / tag â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_branch_create | tab_id, name, from_oid |
| on_branch_delete | tab_id, name or names[] (bulk delete) |
| on_branch_rename | tab_id, old_name, new_name |
| on_tag_create | tab_id, name, oid, annotated |
| on_tag_delete | tab_id, name |
| â”€â”€ Stash â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_stash_push | tab_id, index, message, include_untracked |
| on_stash_pop | tab_id, index, drop (true=pop, false=apply) |
| â”€â”€ Rebase â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_rebase_start | tab_id, base, action_count |
| on_rebase_abort | tab_id |
| â”€â”€ Git Flow â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_flow_init | tab_id |
| on_flow_feature_start | tab_id, name |
| on_flow_feature_finish | tab_id, name |
| on_flow_release_start | tab_id, version |
| on_flow_release_finish | tab_id, version |
| on_flow_hotfix_start | tab_id, name |
| on_flow_hotfix_finish | tab_id, name |
| â”€â”€ Pipelines â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_pipeline_run_request | pipeline_id, tab_id? â€” fired only when the user presses Play on a stub def (empty stages); defs with non-empty stages are replayed directly. Handler must compile stages and call arbor.pipeline.run |
| on_pipeline_started | run_id, pipeline_id, plugin |
| on_pipeline_step_done | run_id, stage, step, exit_code |
| on_pipeline_done | run_id, plugin, status |
| â”€â”€ Merge Requests / Pull Requests â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_mr_opened | number, title, source_branch, target_branch, provider |
| on_mr_merged | number, provider |
| on_mr_updated | number, provider |
| â”€â”€ Issues â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_issue_linked | issue_id, identifier, sha, branch |
| on_issue_transitioned | issue_id, identifier, from_status, to_status |
| â”€â”€ Git notes â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_note_saved | tab_id, commit_oid, namespace, plugin? (set when fired from Lua) |
| on_note_deleted | tab_id, commit_oid, namespace, plugin? (set when fired from Lua) |
| â”€â”€ Workspaces â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_workspace_created | id, name, color_idx, group_id, repo_ids, repo_count |
| on_workspace_updated | id, name, color_idx, group_id, repo_ids, repo_count |
| on_workspace_deleted | id, name, color_idx, group_id, repo_ids, repo_count |
| on_workspace_switched | id, name, color_idx, repo_ids, from_id? (previous workspace) |
| on_workspace_repo_added | workspace_id, repo_id |
| on_workspace_repo_removed | workspace_id, repo_id |
| â”€â”€ Security â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_security_summary_loaded | tab_id, provider, counts, total, risk_label?, web_url? (counts are active-only) |
| on_security_finding_state_changed | tab_id, finding_id, severity, from_state?, to_state, title?, web_url? (plugin-cooperation channel) |
| â”€â”€ Theme / branding â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| on_theme_changed | theme_id, theme_name, vars (merged effective stylesheet), source ("user"\|"plugin"\|"init") |
| â”€â”€ Schedulers â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€ |
| arbor.scheduler.register (action name) | Spring-style triggers: fixed_rate / fixed_delay / cron. Manifest opt-in: [scheduler] enabled = true |

## Vetoable hooks â€” on_pre_commit

A small set of hooks runs *before* the host operation and lets
  any handler abort it. Today only `on_pre_commit` uses this
  pattern; future additions (e.g. `on_pre_push`) will follow
  the same convention.

- Returning a non-empty **string** from the handler
      blocks the operation. The string is used as the abort reason
      and shown to the user.
- Returning `false` blocks without a stated reason.
- Returning `nil` (or no value) lets the operation
      proceed.
- Multiple plugins each see the same payload; **every** veto is concatenated into the final error message.

```lua
arbor.events.on("on_pre_commit", function(ctx)
  -- ctx = { tab_id, message, amend }
  if #ctx.message > 200 then
    return "Subject too long: " .. #ctx.message .. " chars (max 200)."
  end
  -- nothing returned â†’ commit proceeds
end)
```

## arbor.events â€” subscribe and emit

One namespace for both built-in lifecycle hooks (`on_repo_open`, `on_commit`, â€¦) and plugin-defined events. Subscribers don't have to distinguish the two: every event flows through the same `arbor.events.on(name, fn)`.

**Naming rule for plugin events:** events are always published under the *publisher's* plugin name. If you call `arbor.events.emit("build-done", ...)` from the plugin `compile-action`, Arbor dispatches `compile-action:build-done` to every subscriber. If you include a colon yourself, the prefix must match your own plugin name â€” otherwise a runtime error is raised (this prevents one plugin from spoofing another's events).

```lua
-- â”€â”€ Publisher: plugins/compile-action/main.lua â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
arbor.events.on("compile:run", function(_)
  local job, err = arbor.job.spawn({
    name    = "Build",
    command = "make",
    cwd     = arbor.repo.current(),
  })
  if not job then arbor.log.warn("spawn failed: " .. err); return end
  job:ok(function(r)  arbor.events.emit("build-done", { success = true,  exit_code = r.exit_code, repo = arbor.repo.current() }) end)
     :err(function(r) arbor.events.emit("build-done", { success = false, exit_code = (r and r.exit_code) or -1, repo = arbor.repo.current() }) end)
end)

-- â”€â”€ Subscriber: plugins/auto-notify/main.lua â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€â”€
arbor.events.on("compile-action:build-done", function(ctx)
  if ctx.success then
    arbor.notify{ title = "Build OK", message = "Finished cleanly", level = "success" }
  else
    arbor.notify{ title = "Build failed", message = "Exit " .. ctx.exit_code, level = "error" }
  end
end)
```

Payloads are serialised to JSON once on the emitting side and delivered as native Lua tables to every subscriber.

**Delivery is asynchronous.** `emit` dispatches on a background thread so it can safely be called from inside a hook handler (where the plugin host mutex is already held). Don't assume subscribers have run by the time `emit` returns â€” if you need to react to completion, have the subscriber emit its own follow-up event.

### arbor.service â€” cross-plugin RPC

Where `arbor.events.emit` is fire-and-forget, `arbor.service` is
  request / response. A plugin exports named functions; other plugins call them
  with arguments and get the return value as a Promise. Calls always run on a
  background thread and never block the caller, so they're safe to invoke from
  inside any hook handler.

```lua
-- Provider: plugins/greeter/main.lua ------------------------------------------
-- manifest.toml â†’ [permissions] service_export = true
arbor.service.export("greet", function(args)
  return "hello " .. (args.name or "world")
end)

-- Consumer: plugins/caller/main.lua --------------------------------------------
-- manifest.toml â†’ [permissions] service_call = true
arbor.service.call("greeter.greet", { name = "Arbor" })
  :ok(function(r) arbor.log.info(r) end)                  -- "hello Arbor"
  :err(function(e) arbor.log.warn(e.kind .. ": " .. e.message) end)

-- Inside an async.run coroutine you can await sequentially:
arbor.async.run(function()
  local r, err = arbor.async.await(arbor.service.call("greeter.greet", { name = "Arbor" }))
  if err then arbor.log.warn(err.message); return end
  arbor.log.info(r)
end)
```

#### Typed error kinds

The promise rejects with a table `{ kind, message }`; `kind` is one of:

- `not_found` â€” the target plugin isn't loaded, or the requested method isn't registered
- `plugin_disabled` â€” the target plugin is installed but disabled in the Plugin Manager
- `handler_error` â€” the provider's handler raised while executing (message carries the Lua error)

An optional third `cb` argument still works as zucchero: it fires alongside
  the promise with `(ok, value_or_err)`. Omit it (and the promise) entirely for
  "fire and forget" calls whose outcome you don't care about.

#### Debug helpers

```lua
arbor.service.list()        -- every "<plugin>.<method>" exported by any enabled plugin
arbor.service.list_own()    -- only the services this plugin has exported
```

> **Delivery semantics** Each call spawns a short-lived worker thread that acquires the plugin host mutex, runs the target handler, then invokes the caller's callback â€” in that order, under the same lock. The callback executes on the worker thread, so don't assume Svelte-side state is in any particular state; prefer to `arbor.events.emit` a follow-up event for UI reactions.

### Wildcard subscriptions

The event name passed to `arbor.events.on` may contain one or more `*` characters. Each `*` matches any sequence of characters â€” including empty strings and colon / dot separators â€” with no segment boundaries. Literal strings without `*` still require an exact match.

```lua
-- Debug: log every event fired anywhere
arbor.events.on("*", function(ctx)
  arbor.log.debug("bus event received: " .. arbor.json.encode(ctx))
end)

-- Listen to all events from one plugin
arbor.events.on("compile-action:*", function(ctx)
  -- matches "compile-action:build-done", "compile-action:started", â€¦
end)

-- Match a suffix
arbor.events.on("*:build-done", function(ctx) ... end)
```

> **Note** A plugin with at least one wildcard subscription bypasses the manifest hook filter â€” it will receive all built-in lifecycle hooks too (`on_commit`, `on_repo_open`, â€¦) even if they aren't declared under `[hooks]`. Handlers must tolerate varied payload shapes.

### Discovering hooks at runtime â€” arbor.hooks

Every built-in hook ships with a machine-readable schema describing the `ctx` table its handlers receive. Use it to generate docs, build
  validators, or pick the right hook to subscribe to without leaving your editor.

```lua
-- List every built-in hook
for _, def in ipairs(arbor.hooks.list()) do
  arbor.log.info(def.category .. " :: " .. def.name)
end

-- Inspect one hook
local d = arbor.hooks.describe("on_repo_open")
-- d = {
--   name        = "on_repo_open",
--   category    = "repo",
--   description = "Fired when the user opens a repo â€¦",
--   ctx = {
--     { name="tab_id", type="string", required=true, description="â€¦" },
--     { name="path",   type="string", required=true, description="â€¦" },
--     { name="name",   type="string", required=true, description="â€¦" },
--   },
-- }
```

Action hooks fired via `arbor.events.emit`, `arbor.command.register`,
  or `arbor.job.spawn{on_done=â€¦}` are *not* in the catalog â€” they're plugin-defined. `describe()` returns `nil` for those.

---

# Plugin Development â€” API: Core

Core Lua APIs available to all plugins. No special permissions required unless noted.

## Calling convention

The `arbor.*` API uses two consistent conventions throughout:

- **Errors as tuples**. Any function that can fail at runtime (I/O, parse, network, git, registry) returns `(value, nil)` on success and `(nil, err_string)` on failure. Callers that don't care about the error simply read the first return value; callers that do care can check the second. Programming errors (permission denied, missing required argument, wrong Lua type) still raise â€” those are bugs to fix in the plugin, not recoverable failure modes.
- **Table arguments for > 2 args or any optional arg**. Functions like `arbor.fs.move{ src, dest, overwrite? }`, `arbor.terminal.exec{ command, cwd? }`, `arbor.text.replace{ content, pattern, replacement, plain? }` take a single config table. This keeps call sites readable when fields are added later. Single-mandatory-arg functions (`arbor.fs.read(path)`, `arbor.repo.remote(name)`) stay positional; `arbor.events.emit(name, payload)` is also positional as a hot-path exception.

```lua
-- Tuple return: ignore the error or branch on it
local content    = arbor.fs.read(path)                  -- nil on failure, fine if you don't care
local content, err = arbor.fs.read(path)                 -- handle the failure explicitly
if not content then arbor.log.warn("read: " .. err); return end

-- Table-config: missing required field RAISES (programming error)
local ok, err = arbor.fs.move{ src = a, dest = b, overwrite = true }
if not ok then arbor.log.warn("move failed: " .. err) end
```

## arbor.log â€” logging

```lua
arbor.log.debug("detailed trace")
arbor.log.info("something happened")
arbor.log.warn("unexpected state: " .. tostring(val))
arbor.log.error("fatal: " .. err)
-- All messages are prefixed [plugin-name] in the Arbor log
```

Every call is also pushed to an in-memory ring buffer (last 5 000 entries)
  and surfaced in the **Plugin Logs** bottom panel â€” *Tools â†’ Plugin Logs* in the main menu, or `Alt+Shift+L`.
  Disabled plugins do not log: their entries are dropped at the API boundary,
  and plugins disabled at startup never get a Lua VM in the first place.

### Plugin Logs panel

The panel streams new lines in real time and is the canonical place to
  triage plugin behaviour without leaving Arbor.

- **Multi-select plugin filter** â€” a Filter dropdown with one
    checkbox per plugin that has logged anything this session. Includes *All plugins* / *None* shortcuts and a header summary
    (*"compile-action +2"*) when more than one is active.
- **Per-level toggles** â€” independent buttons for `debug` / `info` / `warn` / `error`. Off levels are excluded from the visible list and
    the line counter.
- **Free-text search** â€” case-insensitive substring match
    across the whole formatted line (timestamp, level, plugin, message).
    The search field highlights matches inline.
- **Pipeline tagging** â€” log lines mirrored from a pipeline
    step's captured stdout/stderr carry the pipeline name and run id. A
    dedicated *Pipeline* selector in the filter dropdown lets you
    isolate one run; *Clear pipeline logs* wipes only those entries
    and leaves your direct `arbor.log.*` output intact.
- **Structured highlighting** â€” recognised tokens
    (timestamps, run ids, exit codes, paths) get their own colour so a
    scrolling stream stays scannable. Severity tints follow the global
    palette (info / warn / error).
- **Auto-scroll & jump-to-latest** â€” the panel pins the
    view to the newest line; scrolling up pauses auto-follow and reveals a
    pill that snaps back to the bottom on click.
- **Copy & Clear** â€” Copy serialises the currently
    visible (i.e. filtered) lines to the clipboard as plain text. Clear
    drops every entry from the buffer.

The 5 000-entry cap evicts oldest-first. If you need durable
  per-plugin retention, write to your own log file via `arbor.fs.append`.

## arbor.settings â€” persistence

Settings are split into two scopes:

- **global** â€” stored in `~/.config/arbor/plugin_data/<name>/global.json` â€” independent of the active repo
- **project** â€” stored in `<repo>/.arbor/plugins/<name>/project.json` â€” per-repository; raises a Lua error if no repo is open

```lua
-- Global settings
arbor.settings.global.set("api_key", "secret")
local key  = arbor.settings.global.get("api_key")     -- nil if absent
local all  = arbor.settings.global.get_all()            -- table of all keys
arbor.settings.global.clear("api_key")                 -- delete a single key (set to nil)

-- Project settings (requires an active repo)
arbor.settings.project.set("profile", "prod")
local p = arbor.settings.project.get("profile")
local all_proj = arbor.settings.project.get_all()
```

## arbor.json â€” encode / decode

```lua
local s, err = arbor.json.encode({ key = "val", n = 42 })
-- s = '{"key":"val","n":42}'   err = nil on success

local t, err = arbor.json.decode('{"a":1}')
-- t.a == 1   err = nil on success
```

## arbor.json_studio â€” open the JSON inspector

One-call API that opens a host-rendered modal: lazy virtualised tree, JSONPath query, syntax-highlighted text view. Pass `text` or `path`. Backed by simd-json on the host so multi-megabyte payloads stay responsive. Earmarked to migrate to a self-contained WASM plugin once that runtime lands â€” the API will not change.

```lua
-- Open from disk (host reads the file)
arbor.json_studio.open({ path = "/abs/data.json" })

-- Open inline text
arbor.json_studio.open({
  text  = response_body,
  title = "API response",   -- optional; defaults to filename or "JSON Studio"
})

-- The query bar in the modal accepts full RFC 9535 JSONPath:
--   $.foo.bar                       -- property chain
--   $.arr[0]   $.arr[1:5]            -- index / slice
--   $..key                           -- recursive descent
--   $.users[?@.age > 30]             -- filter
--   $.books[?@.price < 10 && @.in_stock]
--   $..*[?match(@.email, ".*@.*")]   -- regex (RFC function)
--   $[?length(@.tags) > 2]           -- length() / count()
-- Plus shorthands: bare "foo" â†’ $..foo, ".foo" â†’ $.foo, etc.
```

## arbor.fs â€” filesystem

Requires the `fs` permission: `"read"` for read-only ops, `"write"` for read+write. The `fs_scope` field controls path bounds â€” empty (default) sandboxes to the active repo; `["*"]` grants unrestricted access; any other list extends the active-repo sandbox with those absolute paths. All read/write functions return `result, nil` on success or `nil, err` on failure.

```lua
local content, err = arbor.fs.read("/path/to/file.txt")
local ok,      err = arbor.fs.write("/path/to/out.txt", content)
local entries      = arbor.fs.list("/path/to/dir")  -- array of {name, is_file, is_dir}
local joined       = arbor.fs.join("/base", "sub", "file.txt")
local exists       = arbor.fs.exists("/path")
local is_file      = arbor.fs.is_file("/path")
local is_dir       = arbor.fs.is_dir("/path")
-- copy(src, dst): if dst is an existing dir, file is placed inside it
arbor.fs.copy("/path/to/app.war", "/opt/tomcat/webapps/")
-- delete(path): removes a file or a directory tree
arbor.fs.delete("/path/to/old.war")
```

## arbor.repo â€” repository info

Read functions require `git = "read"` (or higher). `fetch_active_tab` and `clone` require `git = "write"` (or higher).

```lua
local path     = arbor.repo.current()           -- active repo path, or nil
local branch   = arbor.repo.branch()            -- current branch name
local dirty    = arbor.repo.is_dirty()          -- bool: uncommitted changes?
local remote   = arbor.repo.remote("origin")    -- URL of the named remote, or nil

-- Fetch origin for the currently active tab (the tab the user is looking at).
-- Returns true on success, false when silently skipped (no active tab, no
-- origin remote, or network failure â€” no error is raised either way).
-- After a successful fetch, emits "arbor://graph-refresh" so the frontend
-- reloads the commit graph and remote branch list automatically.
-- Ideal for use inside a focus-gated scheduler (only_when_focused = true).
local ok = arbor.repo.fetch_active_tab()   -- requires git = "write" (or higher)

-- List branches and tags of the active repo (sorted, with is_head flag).
local branches = arbor.repo.branches()         -- [{name, is_remote, is_head}]
local tags     = arbor.repo.tags()             -- [{name, target}]

-- List commits in a range, newest-first by author time. Returns (commits, err).
-- Defaults: from=nil (walk to root), to="HEAD", limit=1000, include_merges=true.
local commits, err = arbor.repo.commits({
  from           = "v1.0.0",   -- exclusive lower bound (commit/tag/branch)
  to             = "HEAD",     -- inclusive upper bound
  limit          = 500,
  include_merges = false,
})
-- Each commit: { oid, short_oid, summary, message, author_name,
--                author_email, author_time, parents }

-- List untracked-and-not-ignored paths in the working tree.
-- Useful for housekeeping plugins (e.g. proposing .gitignore entries).
local paths = arbor.repo.untracked()           -- ["target/foo.bin", ".env", ...]
```

### arbor.repo.clone â€” background clone

Clone a remote repository into a local directory. The clone runs in a
  background **Job** â€” progress streams into the Jobs overlay and
  Job Output panel exactly like `arbor.job.spawn` results, with
  live cancel support. Uses the system `git` binary so SSH keys and
  credential helpers (including the Arbor keyring) work transparently.

Returns the `job_id` string, so you can pair it with `arbor.job.list()` / `arbor.job.cancel(id)`.

```lua
local job_id = arbor.repo.clone({
  url                = "https://github.com/user/repo.git",  -- required
  dest               = "/abs/path/to/target",               -- required, parent dir must exist
  branch             = "main",                              -- optional (--branch)
  shallow            = false,                               -- optional (--depth 1)
  recurse_submodules = false,                               -- optional (--recurse-submodules)
  name               = "Clone myrepo",                      -- optional display name in Jobs overlay
  category           = "Clone",                             -- optional grouping label
  on_done            = function(ctx)
    -- ctx = { job_id, success, exit_code, cancelled, dest, url }
    if ctx.success then
      arbor.notify{ title = "Clone done", message = ctx.dest, level = "success" }
    else
      arbor.notify{ title = "Clone failed", message = "exit " .. tostring(ctx.exit_code), level = "error" }
    end
  end,
})   -- requires git = "write" (or higher)
```

## arbor.workspace â€” workspace and repo-registry queries

Read-only APIs for inspecting the user's workspaces and the central repo
  registry. No special permissions required. The mutating `switch()` call emits `arbor://workspace-switched` and fires the `on_workspace_switched` hook so other plugins can react.

```lua
local list   = arbor.workspace.list()          -- [{id, name, color_idx, group_id, repo_ids, repo_count}]
local active = arbor.workspace.active()         -- active workspace or nil
local ws     = arbor.workspace.get(ws_id)       -- single workspace or nil

-- Every repo Arbor has ever registered (not just the active workspace's members).
local all_repos = arbor.workspace.list_repos()   -- [{id, path, display_name, remote_url}]
-- Just the repos in a specific workspace:
local ws_repos  = arbor.workspace.list_repos(ws_id)

local repo = arbor.workspace.repo(repo_id)       -- {id, path, display_name, remote_url} or nil

-- Activate a different workspace (swaps the tab set on the UI side).
local ok = arbor.workspace.switch(ws_id)         -- returns bool
```

## arbor.tabs â€” programmatic tab control

Open a registered repository as an Arbor tab. The repo must already be
  in the registry (added via the workspace UI or auto-registered via `arbor.workspace.list_repos`). If a tab for that repo is
  already open, it is brought to the front instead of duplicated.

```lua
local ok, err = arbor.tabs.open_repo(repo_id)   -- (true, nil) | (false, err)
```

## arbor.mr / arbor.ci â€” git provider MRs & CI (credential-blind)

Read-only access to merge requests and CI runs hosted on the git
  provider behind a registered repository. Permission gate: `provider = "read"`. The OAuth token never leaves the OS
  keyring; the host resolves it internally and hands the plugin only
  the decoded payloads. Pass `repo_id` from `arbor.workspace.list_repos()` to scope the call to a
  specific registered repo, or omit it to use the active tab.

```lua
-- Who am I on this provider?
local me = arbor.mr.current_user({ repo_id = entry.id })   -- { id, login, name, ... }

-- List my open MRs across one repo. Use the literal "current_user"
-- sentinel to mean "the authenticated user on THIS provider" â€” the host
-- resolves it for you, the plugin never has to know the actual login.
local mrs, err = arbor.mr.list({
  repo_id = entry.id,        -- workspace registry id; default: active repo
  state   = "open",          -- "open" | "closed" | "merged" | "all"
  author  = "current_user",  -- or any explicit login string
})
-- Each MR (camelCase): { number, title, state, isDraft, author, sourceBranch,
--                        targetBranch, webUrl, checksStatus, ... }

-- Most recent CI run on a branch
local runs, err = arbor.ci.runs({
  repo_id  = entry.id,
  branch   = mr.sourceBranch,
  per_page = 1,
})
-- Each run: { id, name, status, branch, commit_sha, web_url, created_at,
--             provider, duration_secs }
```

## arbor.security â€” vulnerability dashboard (credential-blind)

Read-only access to GitLab Vulnerability Reports and GitHub
  GHAS / Dependabot / Secret-Scanning posture data. Same permission
  gate (`provider = "read"`) and same `repo_id` resolution as `arbor.mr` / `arbor.ci`. Default
  state filter is active-only (Detected + Confirmed) â€” pass `states` explicitly for closed findings or both.

```lua
-- Cheap probe (does the provider expose a dashboard for this repo?)
local ok = arbor.security.supports({ repo_id = entry.id })

-- Headline summary used by the dashboard panel.
local sum = arbor.security.summary({
  repo_id    = entry.id,
  range_days = 90,    -- optional, clamped to [7, 90], default 30
})
-- sum.counts          : { critical, high, medium, low, info, unknown }   (active-only)
-- sum.median_age_days : same shape, days as integers (or nil per severity)
-- sum.risk_score      : { value: number, label: "Low|Medium|High|Critical" } | nil
-- sum.time_series     : { points = [...], range_days } | nil
-- sum.web_url         : provider-native dashboard URL

-- Findings list â€” defaults to active scope.
local list = arbor.security.findings({
  repo_id    = entry.id,
  severities = {"critical", "high"},      -- optional
  states     = {"resolved", "dismissed"}, -- optional, default {detected, confirmed}
  search     = "deserialization",
  limit      = 200,
})
-- Each: { id, severity, state, title, description?, scanner?, report_type?,
--         file_path?, start_line?, web_url?, created_at, age_days, identifiers, provider }
```

## arbor.meta â€” plugin identity & environment

```lua
arbor.meta.plugin_name()              -- "my-plugin"
arbor.meta.api_version()              -- 1  (Arbor plugin API integer)
arbor.meta.app_version()              -- "0.9.0"  (Arbor app semver string)
arbor.meta.plugin_dir()               -- "/path/to/plugins/my-plugin"
arbor.meta.os()                       -- "windows" | "macos" | "linux"
arbor.meta.plugin_loaded("other")     -- true / false (live + enabled check)
```

`plugin_loaded(name)` is a synchronous check against the host's
  plugin registry â€” use it to branch on whether a sibling plugin is active
  right now without going through the async, fire-and-forget `arbor.service.call` path (which races against startup and can
  silently no-op on host mutex contention).

Use `arbor.meta.os()` to build platform-correct commands and paths:

```lua
local is_win = arbor.meta.os() == "windows"
local sep    = is_win and "\\" or "/"
local ext    = is_win and ".bat" or ".sh"
-- e.g. build the Tomcat catalina script path:
local bin = tomcat_home .. sep .. "bin" .. sep .. "catalina" .. ext
```

## arbor.timer â€” deferred / recurring execution

```lua
-- Fire once after delay_ms milliseconds
local id = arbor.timer.after(500, function()
  arbor.log.info("fired after 500ms")
end)

-- Fire every interval_ms milliseconds until cancelled
local id2 = arbor.timer.every(5000, function()
  arbor.log.info("tick")
end)

arbor.timer.cancel(id)   -- cancel a timer by its id
```

**Tip:** prefer `arbor.scheduler.register` (below) for recurring tasks â€” its triggers are richer (cron, fixed_delay, focus gate) and the registrations are shown in the Plugin Manager so users can stop/start each one individually.

## arbor.scheduler â€” Spring-style background schedules

Opt the plugin into the scheduler with `[scheduler] enabled = true` in `plugin.toml`, then declare every concrete schedule from `main.lua`. Triggers are modelled on Spring's `@Scheduled` annotation: pick exactly one of `fixed_rate`, `fixed_delay`, or `cron`.

| Field | Meaning |
| --- | --- |
| action | String â€” required. Plugin action name fired each tick (subscribe with arbor.events.on(action, fn)). |
| fixed_rate | Duration. Fire every N regardless of how long the previous handler took. Next fire = previous start + N. |
| fixed_delay | Duration. Wait N after the previous handler returned. Next fire = previous end + N. Use this when overlap would be harmful. |
| cron | 6-field Spring cron â€” second minute hour day-of-month month day-of-week. Anchored to the wall clock, not to "now + N". |
| initial_delay | Optional duration. Wait this long before the first fire (fixed_rate / fixed_delay only â€” cron always uses the next matching instant). |
| on_load | Optional bool. Also fire once immediately at plugin load, in addition to the normal cadence. Default false. |
| only_when_focused | Optional bool. Skip firing while the app window is unfocused or minimised. The clock keeps ticking; a missed tick is simply dropped. Default false. |

Durations accept bare numbers (seconds), suffix form (`"30s"`, `"5m"`, `"2h"`, `"1d"`), or ISO-8601
  (`"PT30S"`, `"PT1H30M"`).

```lua
-- plugin.toml:
--   [scheduler]
--   enabled = true

-- Fixed-rate: every 5 minutes, regardless of handler duration.
arbor.scheduler.register({
  action     = "my_plugin:refresh",
  fixed_rate = "5m",
  on_load    = true,                -- also fire once at plugin load
})

-- Fixed-delay: 30 s AFTER the previous fetch finishes â€” prevents overlap
-- when the network is slow.
arbor.scheduler.register({
  action            = "my_plugin:slow_poll",
  fixed_delay       = "30s",
  initial_delay     = "10s",
  only_when_focused = true,
})

-- Cron: every weekday at 09:30 (sec min hr dom mon dow). Anchored to wall clock.
arbor.scheduler.register({
  action = "my_plugin:morning_brief",
  cron   = "0 30 9 * * MON-FRI",
})

arbor.events.on("my_plugin:refresh", function(_ctx)
  arbor.log.info("tick")
end)
```

Re-calling `register` with the same `action` replaces
  the previous entry â€” handy for plugins that recompute cadence from settings.
  Inspect the current set with `arbor.scheduler.list()`; users can
  also stop/start individual entries from the Plugin Manager.

## Built-in utility modules

These are available via `require()` inside any plugin without adding files â€” they are pre-loaded by the sandbox.

| Module | Key exports |
| --- | --- |
| arbor.schema | validate(data, rules) â†’ ok, errors Â· check(data, rules) â†’ bool (shows toast on first error) |
| arbor.async | Promise Â· run(fn) Â· await(p) Â· debounce(fn, delay_ms) Â· throttle(fn, interval_ms) |
| arbor.event | on(event, fn) Â· off(event, fn?) Â· emit(event, payload) â€” in-process pub/sub between plugin modules |

```lua
-- arbor.schema â€” validate form submissions
local schema = require("arbor.schema")
arbor.events.on("my_plugin:save", function(ctx)
  if not schema.check(ctx, {
    name    = { required = true, max_len = 64 },
    url     = { required = true, pattern = "^https?://" },
    timeout = { min = 1, max = 300 },
  }) then return end   -- check() shows toast on first error
  -- ... proceed with save ...
end)

-- arbor.async â€” promises + debounce
local async   = require("arbor.async")
local refresh = async.debounce(function()
  -- called at most once per 200ms after the last trigger
end, 200)

-- Promise: producers (service.call, job.spawn, ui.confirm) return one.
arbor.service.call("compile-action.resolve_java_home", {})
  :ok(function(r)  arbor.log.info("JAVA_HOME = " .. (r.java_home or "")) end)
  :err(function(e) arbor.log.warn("svc " .. e.kind .. ": " .. e.message) end)

-- Sequential await inside async.run â€” yields the coroutine until each promise settles.
async.run(function()
  local ok, err = arbor.async.await(arbor.ui.confirm{ message = "Proceed?" })
  if err or not ok then return end
  local r, sErr = arbor.async.await(arbor.service.call("greeter.greet", { name = "you" }))
  if sErr then arbor.log.warn(sErr.message); return end
  arbor.log.info(r)
end)

-- arbor.event â€” decouple modules
local ev = require("arbor.event")
ev.on("config_changed", function(payload)
  -- payload.repo, etc.
end)
ev.emit("config_changed", { repo = arbor.repo.current() })
```

---

# Plugin Development â€” API: UI

APIs for interacting with the Arbor user interface: notifications, forms, activity bar entries, keyboard shortcuts, and the command palette.

## arbor.ui â€” user interface

| Function | Description |
| --- | --- |
| arbor.notify{ message, title?, level?, action? } | Add a persistent notification to the in-app notification center. level: "info" \| "success" \| "warning" \| "error" (default "info"). See the arbor.notify section below. |
| arbor.ui.form(config) | Display an input form modal; submitting fires submit_action |
| arbor.ui.confirm{ message, confirm_label?, confirm_variant?, state? } | Confirmation dialog. Returns a Promise that resolves with true on confirm and false on cancel. confirm_variant: "primary" \| "danger" \| "ghost". |
| arbor.ui.pick_file(opts) | Native file/folder picker. Fires opts.action with { path, ...opts.extra } on confirm; empty path on cancel. opts.mode: "file" (default), "folder", "save". Optional: title, extensions, initial_path. |
| arbor.ui.add_sidebar(opts) | Register a plugin panel attached to an ActivityBar icon. Accepts side: "left"\|"right" (default "right"), position: "top"\|"bottom" (default "top"), and kind: "form"\|"tree" (default "form"). Form panels respond to panel:open:<id> with set_panel_content; tree panels push nodes via tree.set and accept cross-plugin contributions â€” see the Tree sidebars section below. |
| arbor.ui.set_panel_content(id, body) | Push form-DSL content ({title, nodes, actions?}) into a registered panel. Call from the panel:open:<id> handler, or any time underlying state changes. |
| arbor.ui.tree.set(sidebar_id, body) | Push a tree snapshot into a kind="tree" sidebar. body is {title?, breadcrumb?, nodes} or a bare nodes array. breadcrumb is an optional list of segments {label, icon?, action?, data?, badge?, tooltip?} rendered as a clickable trail above the tree â€” segments with empty action are non-interactive (the current location). Triggers a re-render on the frontend. Multi-selection: tree sidebars now support Ctrl/Cmd+click toggle and Shift+click range. Context-menu items can scope themselves via when.multi: true = only in multi-select, false = single-row only, omitted = both. Action handlers receive ctx.node_ids[] and ctx.nodes[] (single-row contexts get a 1-element array; ctx.node_id and ctx.data stay populated for backward compat). |
| arbor.ui.tree.get(sidebar_id) | Read the snapshot you most recently set, or nil. Useful when merging incremental updates without keeping a parallel cache. |
| arbor.ui.contribute(point, item) | Push an item into a contribution point owned by another plugin. item = {id, payload, priority?, when?, disabled?, group?}. Re-contributing with the same id replaces the previous payload (idempotent). when / disabled / group live at the top level â€” placing them inside payload still works but logs a deprecation warn. |
| arbor.ui.unregister_contribution(point, item_id) | Remove a contribution your plugin previously pushed. |
| arbor.ui.contribution_point(config) | Declare a contribution point owned by your plugin. config = {name, description?, schema?}. Informational â€” listed in list_contribution_points; payloads are NOT validated at runtime. |
| arbor.ui.list_contributions(point) | Read the merged list of contributions for a point (sorted by priority). Lets a host plugin fold contributions into its own snapshot. |
| arbor.ui.container.register(opts) | Declare an aggregated modal. opts = {id, title, kind?, layout?, width?, height?, submit_label?, cancel_label?, on_load?, on_save?}. width / height in px reference a 1920Ã—1080 window and scale linearly with the actual viewport (so "960px" means "half the viewport"). Body is built from <plugin>::<id>:category + <plugin>::<id>:section contributions. |
| arbor.ui.container.open(key) Â· close(key) | Show / hide a registered container by its "<plugin>::<id>" key. |
| arbor.ui.settings.panel(config) | Sugar over container.register: same shape, but forces kind = "modal" + layout = "tree_nav" and binds the sub-points to the conventional <plugin>:settings:{category,section} naming. The gear in Plugin Manager appears whenever a plugin owns at least one container. |
| arbor.ui.settings.open(plugin_name, panel_id) | Open a registered settings panel programmatically. Same effect as the user clicking the gear icon. |
| arbor.ui.settings.close() | Close the currently open settings panel. |
| arbor.ui.icon.register(config) | Register a custom SVG icon, namespaced as plugin:<your_plugin>:<id>. Reference it from any icon field. Wiped on plugin reload / disable. |
| arbor.ui.add_graph_combo(opts) | Register a split button (run + dropdown). target: "activity_bar" (default) or "repo_actions" |
| arbor.ui.set_combo_options{ id, options, selected? } | Dynamically update a combo's option list (call from on_repo_open to refresh per-repo). Optional selected adopts a pick if it appears in options. Thin sugar over contribute_patch("arbor:activitybar", id, {options=â€¦}). |
| arbor.ui.set_autocomplete_options(id, opts) | Reply with fresh suggestions for an autocomplete field using source_action. Call inside the handler registered for that action. |
| arbor.ui.form.set_options(name, opts) | Swap the option list of a select / radio / autocomplete field in the currently-open form |
| arbor.ui.form.set_disabled(name, bool) | Disable or re-enable a field in the currently-open form |
| arbor.ui.form.set_value(name, v) | Programmatically set a field's value in the currently-open form |
| arbor.ui.form.replace(cfg) | Swap the whole node tree of the open form in-place, preserving field values by name. See Dynamic form updates. |
| arbor.ui.form.set_loading(arg) | Toggle the loading overlay without re-rendering the form. arg can be true / false, a label string (implies true), or { loading, label }. Cheap â€” use it for per-step progress ticks during a fan-out loop. |
| arbor.ui.form.close() | Programmatically dismiss the currently-open form. Pair with keep_open = true on the form config when submit launches a follow-up flow (file picker, second form): the modal stays mounted while the secondary flow is up, and you call close() once it completes. |
| arbor.ui.operation.start{â€¦} | Push a progress card into the operations overlay (same widget used by Pull / Fetch-all / Pull-all). Config: {id, title, subtitle?, steps[{key,label}], current?}. The id is plugin-scoped server-side â€” collisions across plugins are impossible. |
| arbor.ui.operation.set_current(id, step_key, detail?) | Move the active-step pointer; auto-completes earlier rows and leaves later ones pending. |
| arbor.ui.operation.update_step(id, step_key, {status?, detail?}) | Patch a single row. status: "pending"\|"completed"\|"skipped"\|"error". Avoid setting "active" here â€” use set_current instead (sticky active = forever spinner). |
| arbor.ui.operation.finish(id, {summary?, error?}) | Close the card. It lingers a few seconds with the summary or error, then auto-dismisses. |
| arbor.ui.add_separator() | Insert a horizontal separator in the activity bar after the last registered item |
| arbor.ui.add_context_menu_item(opts) | Add item to the commit/branch/file context menu |
| arbor.ui.add_menu_item(opts) | Add item to the hamburger menu |
| arbor.ui.add_toolbar_action(opts) | Add an inline action button to one of Arbor's toolbars. target: "diff", "status-bar:left", "status-bar:right", "title-bar:left", "title-bar:right", "commit-detail", "commit-form". Unknown targets pass through verbatim â€” usable for plugin-owned custom toolbars. |
| arbor.ui.open_path(path) | Hand a file/folder to the OS default handler (Explorer on Windows, Finder on macOS, xdg-open on Linux). Used to expose "Open in file manager" affordances on artefact folders. |
| arbor.ui.copy_to_clipboard{ text, toast? } | Copy text to the system clipboard via the webview; optional toast overrides the success message ("Copied to clipboard" by default). For one-shot copies driven by the user clicking a value, prefer the copy_link DSL node â€” it runs entirely client-side with no plugin hop. |
| arbor.ui.show_pipeline_run(run_id) | Deep-link to a pipeline run: opens a standalone detail modal (graph + output log) on top of whatever is currently visible. No-op on empty run_id. Use it to jump from a plugin's own UI (sidebar, history modal, â€¦) to the canonical run view without opening the bottom Pipelines panel. |
| arbor.ui.set_branding{ svg? \| svg_path?, window_icon_path? } | Replace the default Arbor app mark. Pass either svg (inline markup) or svg_path (absolute path read off disk by the host â€” no fs.read perm needed) to paint the in-app surfaces (title-bar slot, welcome screen, About modal, HTML stats export). window_icon_path is an absolute path to a raster image (PNG / ICO) handed to the OS window-icon API â€” taskbar / Alt-Tab / window chrome on Windows & Linux. At least one field is required; missing fields don't reset their counterpart. RAM-only â€” see the Branding & theme section below. |
| arbor.ui.clear_branding() | Restore both the bundled SVG mark and the bundled window icon. No-op when the current override belongs to another plugin. |
| arbor.ui.set_theme_tokens{ vars } | Layer a CSS-variable overlay on top of the active theme. vars is a "--name" = "value" table (every key must start with --). Overlays survive theme switches; they vanish on plugin reload or clear_theme_tokens. |
| arbor.ui.clear_theme_tokens() | Drop this plugin's overlay; other plugins' overlays remain. |

## The unified contribution model

Every `add*` / `set*` / `register` call above is sugar
  on top of `arbor.ui.contribute(point, item)`. Each surface â€” context menu,
  command palette, keybindings, sidebars, activity bar, icons, tree state, panel content â€”
  is exposed as a **well-known contribution point**. Plugins may use the sugar
  API or call `contribute` directly; the result is the same.

The frontend reads a single canonical store (`list_plugin_contributions(point)`) and listens to `arbor://contributions-changed` to refresh. Render-time iteration goes through one host-side primitive (`<Contribution point=â€¦>`) that filters out items from disabled plugins, applies `when` / `disabled` automatically, wraps each snippet in an error boundary, and exposes a `fire(extra?)` helper.

**Top-level fields.** `when`, `disabled`, `group` are typed top-level fields on the
  contribution item â€” not magic keys inside `payload`. `when` takes `{kind?: string|string[], data_field?: {key, value}}` and is
  matched against the renderer's context. `disabled = true` hides the item
  without unregistering it. `group` is a free-form bucket label consumers can
  use to render section headers.

**Built-in point validation.** Payloads contributed to built-in points (`arbor:status-bar:*`, `arbor:menu`, `arbor:keybinding`, etc.) are checked against a
  shape at `contribute` time. A malformed payload is logged
  (`tracing::error`) with the offending plugin / point / item id and dropped
  before it reaches the registry. Plugin-defined points (anything that doesn't
  start with `arbor:`) are not validated.

### Sugar APIs â†” contribution points

| Built-in point | Sugar API | Payload |
| --- | --- | --- |
| arbor:context-menu:<target> | add_context_menu_item | {target, label, action, icon?} |
| arbor:menu | add_menu_item | {label, action, icon?} |
| arbor:sidebar | add_sidebar | {action, label, icon?, side, position, kind, â€¦} |
| arbor:activitybar | add_graph_combo Â· add_separator | {kind: "combo"\|"separator", target, â€¦} |
| arbor:diff-toolbararbor:status-bar:<side>arbor:title-bar:<side>arbor:commit-detail:actionarbor:commit-form:action | add_toolbar_action (single sugar, target selects) | {label?, icon?, action, tooltip?, color?} |
| arbor:command-palette | arbor.command.register | {title, description?, icon?, group?} |
| arbor:keybinding | arbor.keybinding.register | {key, ctrl?, shift?, alt?, action, description?} |
| arbor:icon | arbor.ui.icon.register | {svg} |
| arbor:tree-state | arbor.ui.tree.set | {title?, nodes[], version} â€” replace-by-id |
| arbor:panel-content | arbor.ui.set_panel_content | {title?, nodes, actions?} â€” replace-by-id |
| <plugin>::<id>:category<plugin>::<id>:section | arbor.ui.container.register + arbor.ui.contribute | Aggregated modal (containers). See Containers below. |

Context menus are split **per target** so consumers subscribe only to
  the slot they care about. Use `add_context_menu_item({target = "commit", â€¦})` â€” the dual-write derives the point name as `arbor:context-menu:commit`.
  Known targets: `commit`, `branch`, `tag`, `stash`, `file`, `remote`, `submodule`, `worktree`, `line`, `hunk`, `tab`, plus any plugin-defined string.

Re-contributing with the same `(plugin, point, id)` replaces the previous payload,
  so the sugar APIs that update state at runtime (`set_combo_options`, `tree.set`, `set_panel_content`) work naturally. Use a stable `id` to keep updates idempotent.

When you only want to update *some* fields of an item without re-specifying
  the whole payload, use `arbor.ui.contribute_patch(point, id, partial)` â€”
  it shallow-merges `partial` into the existing payload and writes back. `set_combo_options` is a thin sugar over this primitive.

### Toolbar action points (covered by add_toolbar_action)

Inline action buttons on Arbor's toolbars all share one sugar: `arbor.ui.add_toolbar_action({id, target, action, label?, icon?, tooltip?, color?})`.
  The `target` short-name selects which toolbar; the renderer ignores
  fields it doesn't care about.

| target | Point | Where it renders |
| --- | --- | --- |
| "status-bar:left" | arbor:status-bar:left | StatusBar, after the built-in indicators (branch / change pills). |
| "status-bar:right" | arbor:status-bar:right | StatusBar, before jobs / notifications / version (always visible). |
| "title-bar:left" | arbor:title-bar:left | TitleBar, after the workspace dropdown. |
| "title-bar:right" | arbor:title-bar:right | TitleBar, before docs / theme / settings. |
| "diff" | arbor:diff-toolbar | DiffViewer header â€” next to Copy / Maximize. |
| "commit-detail" | arbor:commit-detail:action | CommitDetailPanel â€” action row below the body. Fired with the commit oid. |
| "commit-form" | arbor:commit-form:action | CommitForm â€” between the Amend toggle and the Commit split button. |
| "workspace-row" | arbor:workspace-row | WorkspaceManagementModal â€” per-workspace action toolbar (after Edit / Export / Delete). Fired with {workspace_id, workspace_name, repo_count}. |
| <custom> | verbatim | Any other string passes through unchanged â€” use this to target your own plugin's toolbars without a separate sugar. |

### Decorator points (no sugar yet â€” use arbor.ui.contribute)

| Point | Where it renders | Payload |
| --- | --- | --- |
| arbor:branch-decorator | BranchTree â€” badge next to a branch row. branch_pattern filters which branches. | {branch_pattern?, label?, icon?, color?, tooltip?} |
| arbor:file-decorator | FileDiffList / FileTree â€” badge next to a file path. | {path_pattern?, label?, icon?, color?, tooltip?} |
| arbor:welcome-action | WelcomeScreen â€” quick-action card. | {title, description?, icon?, action} |
| arbor:pipelines:toolbar | PipelinesPanel â€” extra icon-only buttons in the left vertical toolbar (Local Pipelines tab), after the built-in Run / Stop / Resume / Clear cluster. | {icon, tooltip?, label?, accent?, success?, danger?, divider_before?, disabled?} |

Some decorator points may not yet have a built-in consumer in your version
  of Arbor â€” they are declared up-front so plugins can start contributing
  without API churn.

### Toolbar action â€” example

```lua
-- Status bar pill that opens the build settings on click.
arbor.ui.add_toolbar_action({
  id      = "active-jdk",
  target  = "status-bar:left",
  label   = "JDK 21",
  icon    = "Coffee",
  action  = "compile:open_settings",
  tooltip = "Active JDK toolchain",
  color   = "accent",
})

-- Diff toolbar: format the file with prettier on click.
arbor.ui.add_toolbar_action({
  id     = "format",
  target = "diff",
  icon   = "Sparkles",
  action = "my_plugin:format_file",
})

-- Commit form: run a pre-commit lint check before allowing the commit.
arbor.ui.add_toolbar_action({
  id     = "lint",
  target = "commit-form",
  label  = "Lint",
  icon   = "CheckCircle2",
  action = "my_plugin:run_lint",
})
```

## Branding & theme

Plugins can swap the app mark and overlay extra CSS variables on top of the
  active theme to deliver an enterprise-branded experience. Both surfaces are **RAM-only**: nothing is persisted, so reloading Arbor restores
  the bundled identity unless the same plugin re-applies the overrides during
  its `on_plugin_load` handler.

### Replace the logo

`arbor.ui.set_branding` covers two surfaces and the plugin
  picks which to override per call:

- `svg` â€” inline SVG markup (the string must start with `<svg`). Paints every in-app surface that shows the
    Arbor identity (title bar, welcome screen, About modal) *and* is embedded by the HTML stats exporter so co-branded reports stay
    consistent without a second round-trip through the plugin.
- `svg_path` â€” alternative to `svg`: absolute
    path to an `.svg` file the host reads off disk. Use this
    when you'd rather ship the artwork as a sibling asset
    (`assets/logo.svg`) than embed it as a long string in `main.lua`. Same trust model as `window_icon_path` â€”
    no `fs.read` permission is required since the read happens
    server-side. Mutually exclusive with `svg`.
- `window_icon_path` â€” absolute path to a **raster** image (PNG or ICO; SVG is rejected because the OS window-icon API
    needs a rasterised buffer and Arbor doesn't bundle a renderer). Used
    for the OS-level icon: taskbar, Alt-Tab list and window chrome on
    Windows / Linux. macOS dock icons come from `Info.plist` and require a build-time swap, so this field is a no-op there.

Either field can be supplied alone â€” a follow-up call that only sets `window_icon_path` swaps the icon without touching the SVG,
  and vice-versa. `arbor.ui.clear_branding()` drops both at
  once and restores the bundled assets.

```lua
-- Replace the Arbor mark + the OS window icon for this session.
-- Hand the host an absolute path â€” no fs.read permission needed.
local dir = arbor.meta.plugin_dir()
arbor.ui.set_branding{
  svg_path         = dir .. "/assets/acme.svg",
  window_icon_path = dir .. "/assets/acme.ico",
}

-- Or embed the markup inline (handy for tiny marks):
-- arbor.ui.set_branding{ svg = "<svg â€¦>â€¦</svg>" }

-- Later: swap only the OS icon (e.g. tint based on environment).
arbor.ui.set_branding{ window_icon_path = dir .. "/assets/acme-prod.ico" }

-- Restore the bundled assets (no-op when another plugin owns the override).
arbor.ui.clear_branding()
```

### Overlay extra theme tokens

`arbor.ui.set_theme_tokens{vars}` writes a map of CSS
  custom properties (each key must start with `--`) onto the
  document root, layered *on top of* the active theme. Overlays
  survive theme switches: when the user picks a new theme, Arbor reapplies
  the active theme first and then re-merges every plugin overlay. Each
  plugin owns one overlay slot; calling `set_theme_tokens` twice
  replaces the previous payload, and `clear_theme_tokens` releases just this plugin's slot.

```lua
-- Tint the accent + diff colors with the corporate palette.
arbor.ui.set_theme_tokens{
  vars = {
    ["--accent"]              = "#e94e1b",
    ["--accent-hover"]        = "#ff6233",
    ["--accent-subtle"]       = "rgba(233, 78, 27, 0.16)",
    ["--diff-add-bg-strong"]  = "rgba(46, 160, 67, 0.42)",
  },
}

-- Listen to theme changes so we can re-tint custom widgets that don't
-- read CSS vars (e.g. a canvas-rendered chart). Declare the subscription
-- in plugin.toml: [hooks] on_theme_changed = true
arbor.events.on("on_theme_changed", function(ctx)
  -- ctx.source: "user" | "plugin" | "init"
  -- ctx.vars:   merged effective stylesheet (active theme + every overlay)
  arbor.log.info("theme is now " .. ctx.theme_name)
end)

-- Drop our overlay when the plugin's branding mode is turned off.
arbor.ui.clear_theme_tokens()
```

## arbor.notify â€” persistent notifications

Adds a notification to the in-app notification center (bell icon in the status bar). Notifications persist until the user explicitly dismisses them. An optional `action` table renders a click button on the notification that triggers a built-in side-effect. Boundary validation: `message` must be a non-empty string and `level` (when supplied) must be one of `"info"|"success"|"warning"|"error"` â€” invalid input raises a Lua error.

```lua
-- arbor.notify{ message, title?, level?, action? }
-- level: "info" | "success" | "warning" | "error"  (default "info")

arbor.notify{ title = "Build succeeded", message = "Release build completed", level = "success" }
arbor.notify{ title = "Build failed",    message = "Exited with code 2 â€” see Jobs panel", level = "error" }
arbor.notify{ message = "Config reloaded" }    -- title-less, defaults to "info"

-- With a click action: button shown in the overlay; clicking runs the
-- associated side-effect and dismisses the notification.
arbor.notify{ title = "Sync Â· MyLink", message = "Checked out develop on 2 worktrees",
              level = "success",
              action = { kind = "open-link-manager", label = "View link", link_id = "..." } }

arbor.notify{ title = "Repo updated", message = "main pulled 3 commits", level = "info",
              action = { kind = "open-tab-by-repo-id", label = "Focus tab", repo_id = "..." } }
```

**Action kinds**:

- `open-link-manager` â€” needs `label`, `link_id`; opens the Linked Worktrees manager pre-selected on that link.
- `open-tab-by-repo-id` â€” needs `label`, `repo_id`; activates the matching open tab (no-op if not currently open).

## arbor.command â€” command palette entries

Register items that appear in the Command Palette (`Ctrl+K`). Each entry fires the action `command:<id>` on the plugin when selected.

```lua
arbor.command.register({
  id          = "my-action",    -- unique within this plugin
  title       = "My Action",    -- shown in the palette
  description = "Does something useful",  -- subtitle (optional)
  icon        = "Play",         -- Lucide icon name (optional)
  group       = "My Plugin",    -- section label (optional)
})

-- Handle the action:
arbor.events.on("command:my-action", function(_ctx)
  arbor.notify{ message = "Hello from the palette!", level = "success" }
end)

-- Remove the entry at runtime:
arbor.command.unregister("my-action")
```

## arbor.contribution â€” registry introspection

Read-only access to the unified contribution registry. A plugin can list every
  contribution registered against a point and every point that's been declared.
  Useful when a host plugin wants to know whether someone has overridden one of
  its sections, when defaulting depends on what's currently contributed, or when
  one plugin orchestrates several others.

| API | Description |
| --- | --- |
| arbor.contribution.list(point) | Items contributed to point, sorted by priority. Each item: {plugin_name, item_id, payload, priority, when?, disabled?, group?}. payload is a Lua table. |
| arbor.contribution.list_points() | Every declared contribution point: {plugin_name, name, description?, schema?}. |

```lua
-- Skip the manual entry if another plugin already
-- contributed a "manual-remove" item under our sidebar.
local existing = arbor.contribution.list("compile-action:builds:context_menu")
local taken = false
for _, c in ipairs(existing or {}) do
  if c.item_id == "manual-remove" then taken = true; break end
end
if not taken then
  arbor.ui.contribute("compile-action:builds:context_menu", {
    id = "manual-remove", payload = { label = "Removeâ€¦", action = "remove" },
  })
end
```

Reads only: there is no `subscribe`. Plugins that need to react to
  contribution changes can listen to the `arbor://contributions-changed` Tauri event via the standard hook mechanism.

## arbor.keybinding â€” plugin keyboard shortcuts

Register keyboard shortcuts that fire a Lua action when triggered anywhere in the app. Plugin shortcuts are visible under the **Plugins** group in **Settings â†’ Keybindings** (read-only).

```lua
-- Call once during on_plugin_load.
arbor.events.on("on_plugin_load", function(_ctx)
  arbor.keybinding.register({
    key         = "F5",
    action      = "compile:run",   -- fired as a plugin hook
    description = "Run build",
  })

  arbor.keybinding.register({
    key         = "b",
    ctrl        = true,
    shift       = true,
    action      = "my_plugin:open_dashboard",
    description = "Open plugin dashboard",
  })
end)

arbor.events.on("compile:run", function(ctx)
  arbor.job.spawn({ name = "Build", command = "make", cwd = arbor.repo.current() })
end)
```

**Note:** plugin keybindings take priority over unbound app keys when the shortcut matches. They do *not* override user-customised app keybindings.

Registered shortcuts surface automatically in **Settings â†’ Keybindings** (read-only "Plugins" section) and the **Shortcuts** documentation page.
  No extra UI wiring is required from the plugin side.

## Combo Button

A split widget: a primary action button (icon only) on the left and a dropdown arrow on the right. `run_icon` accepts any Lucide icon name â€” common choices: `"Play"` (â–¶), `"Hammer"` (ðŸ”¨), `"Wrench"`, `"Zap"`.
  You can register **multiple combos** from the same plugin; they appear in
  registration order within the target area.

```lua
-- Register once (e.g. in on_plugin_load).
arbor.ui.add_graph_combo({
  id         = "my_plugin:run",
  run_icon   = "Play",           -- Lucide icon name
  run_action = "my_plugin:do_run",
  tooltip    = "Run application",
  target     = "repo_actions",   -- or "activity_bar"
  options    = {},
})

-- Refresh options when repo changes
arbor.events.on("on_repo_open", function(ctx)
  arbor.ui.set_combo_options{
    id = "my_plugin:run",
    options = {
      { value = "dev",  label = "Run Â· dev",  group = "Project" },
      { value = "prod", label = "Run Â· prod", group = "Project" },
    },
  }
end)

arbor.events.on("my_plugin:do_run", function(ctx)
  -- ctx.value = selected option value
  arbor.job.spawn({ name = "Run " .. ctx.value, command = "make run_" .. ctx.value,
                    cwd = arbor.repo.current() })
end)
```

### Action Options

Mark an option with `action = true` to make it behave like *"New Workspace"* in the workspace dropdown: clicking it fires the
  combo's `run_action` directly (so the plugin can open a modal or
  settings form) and does **not** become the persisted selection â€”
  the previously selected config stays active in the run button. Action options
  render in a visually separated footer below a divider.

```lua
arbor.ui.set_combo_options{
  id = "my_plugin:run",
  options = {
    { value = "dev",               label = "Run Â· dev",          group = "Project" },
    { value = "prod",              label = "Run Â· prod",         group = "Project" },

    -- Footer: open modals without changing the selection
    { value = "__new_config__",    label = "âŠ• New configurationâ€¦", action = true },
    { value = "__settings__",      label = "âš™ Run settingsâ€¦",      action = true },
  },
}

arbor.events.on("my_plugin:do_run", function(ctx)
  if ctx.value == "__new_config__" then open_new_config_modal() ; return end
  if ctx.value == "__settings__"   then open_settings_modal()   ; return end
  -- ctx.value = real config id otherwise
end)
```

### Rich Combo Options

Each combo option supports the following extra fields (all optional, additive
  on top of `value` / `label`):

| Field | Type | Effect |
| --- | --- | --- |
| icon | string (Lucide name) | Small icon rendered before the label. |
| subtitle | string | Caption shown below the label in muted text. |
| meta | string | Right-aligned tabular text (counts, durations, â€¦). |
| disabled | boolean | Renders the option dimmed and prevents selection. |
| group | string | Group label â€” consecutive options sharing a group are bucketed under a header. |

```lua
arbor.ui.set_combo_options{
  id = "my_plugin:run",
  options = {
    { value = "dev",  label = "dev",  group = "Project",
      icon = "Play",  subtitle = "fast feedback",  meta = "~3s" },
    { value = "prod", label = "prod", group = "Project",
      icon = "Rocket", subtitle = "release build", meta = "~45s" },
    { value = "stale", label = "legacy", group = "Project", disabled = true },
  },
}
```

## Sidebar Panels (add_sidebar)

Register a plugin panel attached to an ActivityBar icon. By default the
  icon appears on the **right** ActivityBar â€” a dedicated
  plugin-expansion rail, visually identical to the left but dedicated to
  plugins. The left bar is reserved for built-in Arbor sections, though
  plugins may also target `side="left"` when it makes sense.

The right ActivityBar is **completely hidden** when no plugin
  has registered a right-side entry â€” the layout falls back to the classic
  single-bar style.

| Field | Values | Default |
| --- | --- | --- |
| id | string (unique per plugin) | â€” required â€” |
| side | "left" \| "right" | "right" |
| position | "top" (side panel) \| "bottom" (shared bottom slot) | "top" |
| icon | Lucide icon name or single-char emoji | â€” generic icon â€” |
| label / tooltip | string | falls back to id |

The **bottom slot is unique**: clicking a plugin-bottom icon
  overrides whichever panel was open (stage / detail / terminal / jobs /
  pipelines / another plugin) â€” only ONE bottom panel is visible at any
  time, regardless of which ActivityBar fired the click.

Every bottom panel â€” built-in or plugin-contributed â€” wears the same
  standardized header chrome: a 34-px bar on `--bg-base` with the
  panel title on the left, optional inline content, plugin/built-in
  toolbar actions on the right, and a red dot close button at the very end
  (the same widget used by modal headers). For plugin-bottom panels the
  title comes from `arbor.ui.set_panel_content(id, {title, â€¦})`;
  the close button is wired automatically and clears the active bottom
  section. You don't render this chrome yourself â€” only the body content.

```lua
-- Register the panels once at plugin load.
arbor.events.on("on_plugin_load", function()
  arbor.ui.add_sidebar({
    id       = "overview",
    icon     = "ðŸ§©",
    label    = "Panel Demo",
    tooltip  = "Right-side demo panel",
    side     = "right",
    position = "top",         -- right sidebar
  })

  arbor.ui.add_sidebar({
    id       = "runtime",
    icon     = "ðŸ“‹",
    label    = "Demo â€” bottom",
    side     = "right",
    position = "bottom",      -- unique bottom slot
  })
end)

-- Respond to panel:open by pushing form-DSL content.
arbor.events.on("panel:open:overview", function(_ctx)
  arbor.ui.set_panel_content("overview", {
    title = "Panel Demo",
    nodes = {
      { type = "heading", text = "Right-side panels" },
      { type = "label",   text = "Content pushed live by the plugin." },
      { type = "divider" },
      { type = "list", items = {
          { id = "a", icon = "âœ“", label = "Action A", action = "demo:act-a" },
          { id = "b", icon = "â†»", label = "Refresh",  action = "demo:refresh" },
      }},
    },
    actions = {
      { label = "Open bottom panel", action = "demo:open-bottom" },
    },
  })
end)
```

### Supported form-DSL nodes in sidebars

The sidebar renderer is intentionally lightweight â€” it handles the shapes
  common to dashboards and launchers. Rich editing (`tree_layout`, `pipeline_editor`, wizards) still belongs in modals opened via `arbor.ui.form`. Nodes are rendered **recursively** â€” a `section` can contain `list`, `row`,
  nested `section`, etc. at arbitrary depth.

- `heading` â€” `{ type="heading", text="â€¦" }`
- `label` / `paragraph` â€” plain text (sidebar uses the `text` field, not `content`)
- `divider` â€” horizontal rule
- `button` â€” `{ type="button", label?, icon?, icon_only?, variant?, disabled?, tooltip?, action, id }`. Variants: `default` / `ghost` / `primary` / `danger`. `icon_only = true` renders a square 24Ã—24 button.
- `row` â€” `{ type="row", gap?, children[] }`. Inline flex, wraps when narrow. Use to lay out inline icon-button toolbars.
- `list` â€” `{ type="list", items=[{id,label,icon?,detail?,action?}â€¦] }`. A per-item `action` fires when the row is clicked; the row receives `{id, value, label}` in the action context.
- `section` â€” grouped container with optional `title` and nested `nodes`. Children render through the full node renderer, so every node type above is available inside.
- `card_item` â€” MR/Reflog-style list row. Fields: `id`, `icon`, `icon_variant` (accent/success/warning/danger), `title`, `subtitle`, `badge` (small chip, top-right of title), `meta` (`[{text, variant}]` chips below), `action` (primary click on the whole row), `actions` (`[{icon, tooltip, variant, action, extra}]` hover-revealed icon buttons on the right), `tooltip`. Use for dense clickable lists that also need secondary per-row actions.

```lua
-- Example: a sequences-like list where primary click runs, secondary
-- actions fade in on hover.
arbor.ui.set_panel_content("my_panel", {
  title = "Sequences",
  nodes = {
    { type = "card_item",
      id       = seq.id,
      icon     = "Workflow",
      title    = seq.name,
      subtitle = seq.description,
      badge    = tostring(#seq.items),
      meta = {
        { text = "3 enabled",    variant = "muted"   },
        { text = "fail-fast",    variant = "warning" },
        { text = "last: success",variant = "success" },
      },
      action = "my_plugin:run",                    -- primary click
      actions = {                                   -- hover-revealed
        { icon = "Play",   tooltip = "Run",       variant = "accent", action = "my_plugin:run" },
        { icon = "Pencil", tooltip = "Edit",      action = "my_plugin:edit" },
        { icon = "Trash2", tooltip = "Delete",    variant = "danger", action = "my_plugin:delete" },
      },
    },
  },
})
```

`set_panel_content` also accepts a top-level `actions = [{label, action, icon?}â€¦]` array that renders as full-width footer buttons below the body.

## Tree-kind sidebars (contribution model)

A `kind = "tree"` sidebar exposes a tree-of-nodes UI (header
  toolbar, scrollable body, optional footer) and lets *other plugins* extend it through named contribution points. The host plugin owns the tree
  data and the extension contract; consumers push items into the points and
  the same component renders both. This is the pattern used by the built-in `compile-action` sidebar, where `run-action` contributes its "Run configurations" section and per-row Tomcat actions
  without `compile-action` knowing about run.

### 1. Register the sidebar

```lua
arbor.ui.add_sidebar({
  id          = "compile",        -- panel id (namespaced as <plugin>:<id>)
  label       = "Build & Run",
  icon        = "Hammer",
  side        = "right",
  position    = "top",
  kind        = "tree",            -- â† opt into the tree renderer
})
```

### 2. Push the tree

Call `arbor.ui.tree.set(sidebar_id, body)` on every state change
  (typically from `on_repo_open` / `on_tab_switch`).
  Each node is shaped like:

```lua
{
  id            = "phase:maven:compile",   -- unique within parent
  label         = "compile",
  icon          = "CircleDashed",          -- Lucide name, emoji, or
                                           -- "plugin:<plugin>:<icon_id>"
  badge         = "default",                -- optional small chip
  badge_kind    = "accent",                -- info|success|warning|error|muted|accent
  kind          = "lifecycle_phase",        -- free-form classification used
                                            -- by your contribution filters
  selectable    = true,                     -- emits select / context_menu
  expanded      = false,                    -- initial state
  default_action = "compile:run_phase",     -- fired on dbl-click / Enter
  data          = { template_id = "maven", phase = "compile" },
  children      = { ... },                  -- recursive
}
```

### 3. Declare contribution points

Convention: name points `<plugin>:<sidebar_id>:<slot>`.
  The frontend reads the following slots automatically â€” declare them so
  consumers (and the docs) know they exist:

| Slot | Renders | Payload shape |
| --- | --- | --- |
| toolbar | Buttons in the panel header | {icon, tooltip, action, accent?, success?, danger?, divider_before?, disabled?} |
| tree.section | Top-level section appended to the tree | {section = <TreeNode>} |
| node_action | Hover-revealed icon button on each row | {icon, tooltip, action, accent?\|success?\|danger?, when?} |
| node_decorator | Always-on badge / icon between label and actions | {icon?, badge?, badge_kind?, tooltip?, when?} |
| context_menu | Right-click menu items | {label, action, danger?, separator?, when?} |
| dependency_provider | Auto-injects "Show dependencies" in the right-click menu when the node matches | {label, action, when?} â€” handler writes results via tree.set(request_id, â€¦) |
| footer | Items in the panel footer | {kind="text"\|"button", icon?, label?, action?, badge?} |

The `when` filter narrows a contribution to specific nodes: `{kind = "module"}`, `{kind = ["module","runnable"]}`,
  or `{kind = "module", data_field = {key = "template_id", value = "maven"}}`.
  Omit `when` to apply to every node.

### 4. Contribute from another plugin

```lua
-- maven-update-deps / main.lua
local POINT = "compile-action:compile:context_menu"

arbor.ui.contribute(POINT, {
  id       = "update-deps",
  priority = 50,                              -- lower renders first
  payload  = {
    label  = "Update dependencies (latest releases)â€¦",
    action = "maven-update-deps:update",
    when   = { kind = "module",
               data_field = { key = "template_id", value = "maven" } },
  },
})

arbor.events.on("maven-update-deps:update", function(ctx)
  -- ctx = { node_id = "module:maven:foo",
  --         data    = { template_id, role, pom_path, repo_path } }
  -- spawn job, etc.
end)
```

Re-call `arbor.ui.contribute` with the same `id` to
  replace the previous payload â€” useful when your contribution depends on the
  active repo (e.g. a tree section whose contents change per tab). Use `arbor.ui.unregister_contribution(point, id)` to remove it.

### Custom icons

When a Lucide name doesn't fit, register a raw SVG and reference it from
  any `icon` field as `"plugin:<your_plugin>:<id>"`.
  The SVG should use `currentColor` for stroke / fill so it picks
  up the surrounding text color.

```lua
arbor.ui.icon.register({
  id  = "my-logo",
  svg = '<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">'
     .. '  <path stroke="currentColor" fill="none" stroke-width="1.5" '
     ..       'd="M2 8 L8 2 L14 8 L8 14 Z"/>'
     .. '</svg>',
})

-- then in any tree node / contribution:
icon = "plugin:my-plugin:my-logo"
```

### Dependency tree modal

Right-clicking a tree row auto-injects a *Show dependencies* entry
  whenever a `dependency_provider` contribution matches the node
  (via its `when` filter). Selecting it opens the `DependencyTreeModal` and fires the provider's `action` with `{request_id, node_id, data}`.
  The provider's job is to populate `arbor.ui.tree.set(request_id, {title, nodes})` â€” the modal subscribes to that snapshot id and renders the result.

### Dependency Explorer modal (deps-explorer plugin)

Same transport, richer UI: the `deps-explorer` plugin opens an
  IntelliJ-style two-pane modal (resolved deps on the left, usages of the
  selected artifact on the right, with scope / outdated / conflict filters)
  by pushing snapshots under the dedicated sidebar id prefix `deps:<request_id>`. The frontend store `depsExplorerStore` filters the unified `arbor://contributions-changed` event for `point="arbor:tree-state"`, recognises the prefix and pops the
  modal up; subsequent updates with the
  same id patch the open modal reactively (used to attach Maven Central
  latest-version data after the initial tree lands). The pattern is reusable
  for any plugin that wants a dedicated modal â€” pick a unique sidebar-id
  prefix for the plugin and add a small store + listener.

```lua
-- Open the modal immediately with a "loading" snapshot.
local sid = "deps:" .. request_id
arbor.ui.tree.set(sid, &#123;
  title = "Resolvingâ€¦",
  nodes = &#123;&#125;,
&#125;)

-- Heavy work in the background; on done, push the real tree.
arbor.job.spawn(&#123;
  command = "mvn -B dependency:tree -DoutputFile=â€¦",
  on_done = function(jc)
    local nodes = parse_tree(arbor.fs.read(out_file))
    arbor.ui.tree.set(sid, &#123; title = "Maven dependencies", nodes = nodes &#125;)
  end,
&#125;)
```

## Containers (aggregated modals)

A **container** is an aggregated UI surface â€” currently a modal â€”
  whose body is built from cross-plugin contributions. The host registers the
  container; anyone (the host or a third party) contributes *categories* (left sidebar entries) and *sections* (right pane cards). Each section
  is rendered as its own `FormNodeRenderer` and saves in parallel.

Two layers compose every container:

| API | Purpose |
| --- | --- |
| arbor.ui.container.register(opts) | Declare the container shell. Returns immediately; the modal opens lazily on open(). |
| arbor.ui.container.open(key) | Show the modal. key is the canonical "<plugin>::<id>" id. |
| arbor.ui.container.close(key) | Dismiss it. Mismatched keys are ignored so a plugin can't close another's modal. |
| arbor.ui.contribute("<plugin>::<id>:category", item) | Add a sidebar entry. Payload: {label, icon?, description?, priority?}. |
| arbor.ui.contribute("<plugin>::<id>:section", item) | Add a section card. Payload: {category, label?, icon?, nodes, on_save?, state?, priority?}. |

`register` accepts `{id, title, kind?, layout?, width?, submit_label?, cancel_label?, on_save?, on_load?}`. `on_load` fires **once when the modal opens**, before
  categories/sections are read â€” use it to re-contribute fresh state. The
  contribution registry is reactive, so contributions arriving from `on_load` appear without a second round-trip.

Save semantics are **parallel best-effort**: each section's `on_save` fires concurrently (Promise.allSettled), failures are
  aggregated into a single toast, and the host's `on_save` (if set)
  fires last with the full namespaced state `{sections = {[plugin] = {[field] = value}}}`.
  Field-name collisions across sections of *different* plugins are
  prevented by a backend rewrite: every form-DSL field name is silently
  prefixed with `<contributing-plugin>::` when the section
  is contributed, and the prefix is stripped from each plugin's slice on
  save. Plugin code never sees the namespaced names â€” the rewrite is
  transparent. Collisions across sections of the *same* plugin
  still overwrite by last-writer (use unique field names within your own
  sections).

### Host registers a container

```lua
arbor.ui.container.register({
  id            = "main",
  title         = "My Plugin â€” Settings",
  width         = "960px",  -- referenced to a 1920Ã—1080 viewport,
  height        = "680px",  -- scales linearly with the actual window
  submit_label  = "Save All",
  on_load       = "my_plugin:refresh",
  on_save       = "my_plugin:host_save",   -- optional aggregated handler
})

arbor.ui.contribute("my-plugin::main:category", {
  id = "general",
  payload = { label = "General", icon = "Settings", priority = 10 },
})

arbor.ui.contribute("my-plugin::main:section", {
  id = "general-core",
  payload = {
    category = "general",
    label    = "Core",
    nodes    = { { type = "text", name = "api_key" } },
    on_save  = "my_plugin:save_general",
  },
})

arbor.ui.container.open("my-plugin::main")
```

## Plugin settings â€” sugar over containers

`arbor.ui.settings.*` is sugar over the container API for the
  conventional "plugin settings" surface. The wrapper:

- Registers a container with `kind = "modal"`, `layout = "tree_nav"`.
- Forces the category / section sub-points to the historical naming `<plugin>:settings:category` and `<plugin>:settings:section` (single colon between `plugin` and `settings`) â€” so plugins extending a
      host's settings panel use the natural compact name.
- Discovers panels via the container registry â€” the gear icon in Plugin
      Manager appears whenever a plugin owns at least one container.

| Point | Payload shape |
| --- | --- |
| <host>:settings:category | {label, icon?, description?, priority?} â€” sidebar entry. |
| <host>:settings:section | {category, label?, icon?, nodes, on_save?, priority?} â€” content card. category selects which sidebar entry the card belongs to. |

Anyone can contribute to either point. External plugins can (a) add a new
  sidebar entry, (b) drop a card into an existing entry, or (c) replace an
  existing card by re-contributing with the same `id`.

### 1. Host: register the panel + categories

```lua
-- Once at PLUGIN_LOAD. All calls are idempotent.
arbor.ui.settings.panel({
  id           = "main",
  title        = "My Plugin â€” Settings",
  width        = "960px",
  on_load      = "my_plugin:settings_refresh",  -- host pre-open hook
  on_save      = nil,                            -- per-section saves are enough
})

arbor.ui.contribute("my-plugin:settings:category", {
  id = "general",
  payload = { label = "General", icon = "Settings", priority = 10,
              description = "Core knobs that apply to every project." },
})
arbor.ui.contribute("my-plugin:settings:category", {
  id = "advanced",
  payload = { label = "Advanced", icon = "Sliders", priority = 20 },
})

-- Document the contribution points so external plugins can find them.
for _, p in ipairs({
  { name = "my-plugin:settings:category", description = "Sidebar entries." },
  { name = "my-plugin:settings:section",  description = "Content cards (must reference a category id)." },
}) do
  arbor.ui.contribution_point(p)
end
```

### 2. Host: contribute sections (cards) into its categories

```lua
local function build_general_card()
  local key = arbor.settings.global.get("api_key") or ""
  return {
    { type = "card_row", label = "API Key", children = {
      { type = "text", name = "api_key", default = key },
    }},
    { type = "card_row", label = "Mode", children = {
      { type = "select", name = "mode",
        default = arbor.settings.global.get("mode") or "balanced",
        options = {
          { value = "fast", label = "Fast" }, { value = "balanced", label = "Balanced" },
        }},
    }},
  }
end

arbor.ui.contribute("my-plugin:settings:section", {
  id = "general-core",
  payload = {
    category = "general",
    label    = "Core",
    nodes    = build_general_card(),
    on_save  = "my_plugin:save_general",
  },
})

arbor.events.on("my_plugin:save_general", function(ctx)
  arbor.settings.global.set("api_key", ctx.api_key)
  arbor.settings.global.set("mode",    ctx.mode)
  arbor.notify{ message = "Settings saved", level = "success" }
end)
```

### 3. Host: refresh on open

```lua
-- on_load fires once when the modal opens. Re-contribute the cards so
-- toolchain lists, run configurations, etc. reflect what is on disk.
arbor.events.on("my_plugin:settings_refresh", function(_ctx)
  arbor.ui.contribute("my-plugin:settings:section", {
    id = "general-core",
    payload = { category = "general", label = "Core",
                nodes = build_general_card(), on_save = "my_plugin:save_general" },
  })
end)
```

### 4. External plugin: extend an existing panel

```lua
-- "extras-plugin" adds a brand-new sidebar entry to my-plugin's panel.
arbor.ui.contribute("my-plugin:settings:category", {
  id = "extras",
  payload = { label = "Extras", icon = "Plus", priority = 50 },
})

-- And a card under it. The card header shows "Extras Â· extras-plugin"
-- so the user can see who injected it.
arbor.ui.contribute("my-plugin:settings:section", {
  id = "extras-flags",
  payload = {
    category = "extras",
    label    = "Verbose logging",
    nodes    = { { type = "toggle", name = "verbose" } },
    on_save  = "extras-plugin:save_flags",
  },
})
```

### 5. Open the panel programmatically

```lua
arbor.ui.settings.open("my-plugin", "main")
arbor.ui.settings.close()  -- close whatever is open
```

### Cross-plugin reads

Any plugin can read its own settings via `arbor.settings.global.get`.
  To read *another* plugin's settings, set `settings_read_others = true` in `[permissions]` and
  call `arbor.settings.read("other-plugin", "key")` / `arbor.settings.read_project("other-plugin", "key")`. Cross-plugin **writes** stay restricted: the target plugin must opt in by
  exporting a service via `arbor.service.export`, which the caller
  then invokes through `arbor.service.call`.

```lua
-- in extras-plugin, after settings_read_others = true:
local mode = arbor.settings.read("my-plugin", "mode") or "balanced"
```

The Plugin Manager also exposes a **Clear cache** button
  (two-click confirmation) that wipes a plugin's `global.json`.

## Form node types

| type | Key fields | Notes |
| --- | --- | --- |
| text | name, label, placeholder, default, pattern, pattern_hint, readonly | Also: password, email, url |
| textarea | name, label, placeholder, default, rows |  |
| number | name, label, default, min, max, step |  |
| range | name, label, default, min, max, step, show_value, value_format | value_format: "{v}ms" |
| checkbox | name, label, default |  |
| toggle | name, label?, description?, default, size (sm/md/lg) | iOS-style switch. Use for "feature on/off"; use checkbox for "I agree" |
| select | name, label, default, options[] | options: value+label+disabled? |
| radio | name, label, default, options[], inline | options: value+label+description? |
| color | name, label, default (#rrggbb) |  |
| kv_list | name, label, key_placeholder, value_placeholder, default | Submitted as JSON object |
| section | title, description, children[], collapsible, collapsed | Layout only |
| container | children[], columns, gap | CSS grid |
| row | children[], gap, align, wrap | Flexbox row |
| separator | label? | Labelled divider line |
| divider | â€” | Plain <hr> |
| paragraph | content, variant (normal/muted/heading/caption) |  |
| label | text, variant | Static text alias |
| alert | text, variant (info/warning/error/success) |  |
| code | text, language?, copy?, toast? | Read-only monospace block. When language matches a Prism grammar ("json", "rust", "yaml", â€¦) the block is syntax-highlighted using the same Prism setup as the diff viewer. copy: true shows a floating Copy button; toast overrides the success toast. |
| icon | icon (Lucide name), variant (default/muted/info/success/warning/danger), size, tooltip, class, style | Inline Lucide glyph for status dots / badges. Loader2 auto-spins via CSS. |
| copy_link | text, toast?, tooltip?, font (normal/"mono"), class, style | Click-to-copy pseudo-link with a subtle Copy glyph on the right. Calls navigator.clipboard directly â€” no plugin action hop. Ideal for paths, IDs, URLs. |
| button | label?, action, variant (default/primary/danger/ghost), close_after, disabled, icon, icon_only, tooltip, extra, class | Inline action; icon is a Lucide name, icon_only renders without label, extra merges into the action payload. Pass class = "pal-row" for a tight flush-left catalog-row style. |
| menu_button | label?, icon, icon_only, tooltip, show_chevron, options[] | Opens a dropdown menu. Each option: { label?, icon?, action?, extra?, variant?, disabled?, heading?, separator? } |
| date | name, label, default, min, max, readonly, required | Submits ISO "YYYY-MM-DD" |
| datetime | name, label, default, min, max, readonly, required | Submits "YYYY-MM-DDTHH:MM" (local, no TZ) |
| time | name, label, default, min, max, readonly, required | Submits "HH:MM" |
| switch | field, cases, default | Renders one branch based on another field's value |
| tabs | tabs[], default_tab | Tab strip; all fields inside always collected for submit |
| wizard | steps[], start_step, next_label, back_label | Multi-step form with Back/Next footer |
| file | name, label, pick_mode, extensions, placeholder | Opens FilePickerModal â€” submits path string |
| autocomplete | name, id, options?, source_action?, debounce_ms, free_form | Static or dynamic suggestions |
| tags | name, default, suggestions, max | Submits string[] |
| tree | name, nodes[], multi, expanded, bordered, max_height | Hierarchical selector. Nodes: value, label, icon?, group?, tag?, tag_variant?, description?, children? |
| table | name, columns[], min_rows, max_rows, add_label | Submits Array<Record> |
| tree_layout | nav_children[], content_children[], nav_width | 2-col split (nav + content). Typical use: tree on the left, form cards on the right gated with show_if |
| section | title, description, children[], collapsible, collapsed, card, count, add_action, header_actions[], class | card = true renders with dark title bar + counter pill + optional + button. collapsible = true toggles the body. header_actions: { icon, tooltip, action, extra, disabled, variant }[] â€” icon buttons in the header; variant = "danger" applies the red hover. class = "pf-card-compact" tightens body padding for dense list-mode cards. |
| card_row | label, description, children[] | Two-column label + controls row inside a section card |
| form_field | label?, optional_text?, required?, description?, hint?, error?, icon?, actions[]?, children[], for? | Vertical labeled wrapper â€” same look as the host's <FormField> widget. Wrap any nodes with the standard arbor field chrome (label on top, content below, optional hint/error/right-aligned actions). icon is a Lucide name; actions render right-aligned on the label row (typically button nodes). |
| cfg_list | items[] | Item rows with active dot + tags + hover edit/delete. Item: { id, label, active?, tags?, edit_action?, delete_action? } |
| suggest_grid | items[] | 2-col grid of suggestion cards. Item: { name, cmd?, tag?, action? } |
| counter_grid | items[], min_width?, gap?, padding?, actions.select? | Responsive KPI tile grid. Item: { key, label, value, hint?, color?, icon?, empty? }. actions.select fires { key } when a non-empty tile is clicked. color accepts any CSS expression â€” "var(--severity-high)", "#f97316". |
| score_gauge | value, min, max, segments[], label, size, value_color | Semi-circle gauge for a bounded value. Segment: { from, to, color }. size: "sm" \| "md" \| "lg" (default "md"). Display only. |
| time_series_chart | series[], x_kind, height, show_legend, y_include_zero | Multi-series line chart with hover tooltip + legend. Series: { id, label, color, points: [{ x, y }] }. With x_kind = "time" (default), x is an ISO-8601 string; with "linear" it's a number. |
| data_table | columns[], rows[], row_key?, height?, initial_sort?, empty?, actions.row_click? | Sortable / clickable table. Column: { key, label, width?, align?, kind?, color?, sortable? } with kind âˆˆ { "text", "code", "pill", "datetime", "age" }. Row colour override: _<key>_color. actions.row_click fires { row_id, row }. |
| filter_bar | name?, default?, search?, filters[], padding?, actions.change? | Search input + N chip dropdowns. Filter: { id, label, icon?, options[{ value, label, color? }], mode?, searchable?, wide? } with mode âˆˆ { "single", "multi" } (default "multi"). When name is set the value { search, filters: { [id]: string[] } } is collected into form values; actions.change fires { value } on every keystroke / chip toggle. Set search = nil to omit the search input. |

Top-level `arbor.ui.form(config)` options: `title`, `description`, `submit_label`, `submit_action`, `cancel_label`, `cancel_action`, `hide_submit`, `hide_cancel`, `width`, `height`, `sidebar` (two-column nav layout when the root is a `tabs` node), `state`, `css`, `loading`.

`loading = true` renders a translucent overlay with a centered
  spinner above the form body â€” use it while the plugin fans out to the
  network after opening the modal (e.g. fetching per-repo data before the
  dashboard has anything to draw). Toggle it live by passing `loading` alongside `nodes` to `arbor.ui.form.replace`: `arbor.ui.form.replace({ loading = false, nodes = ... })`.

`hide_submit` / `hide_cancel` drop the matching footer
  button entirely â€” useful for read-only modals (show one single *Close* button) or confirmation dialogs where only Submit makes
  sense. Keyboard Escape still closes the modal regardless of which buttons
  are visible.

## Builder DSL â€” chainable form construction

As an alternative to the table-config call, `arbor.ui.form()` (no
  argument) and `arbor.ui.form("id")` return a chainable `FormBuilder`. Every method returns the builder itself, so you can
  pipe a form together one node at a time and finalise with `:open()`.
  Calling `arbor.ui.form(table)` with a config table still works
  exactly as before â€” the builder is purely sugar.

```lua
arbor.ui.form()
  :title("Inspect Commit")
  :description("Add a personal note for this commit.")
  :state({ oid = ctx.oid })
  :textarea("note", { label = "Note", placeholder = "What's interesting?", rows = 3 })
  :text("tag",      { label = "Tag",  placeholder = "e.g. fix, refactor" })
  :checkbox("bookmark", { label = "Bookmark this commit" })
  :submit("Save Note", "inspect:save_note")
  :on_cancel("inspect:cancel_note")
  :open()
```

Each field method takes `(name, opts?)` or a single `{name = ..., ...}` table. Sections auto-close on the next `:section()` call, so flat layouts read naturally; use `:end_section()` to drop back to the top level explicitly. `:field(node)` is the escape hatch â€” push any node table that
  the field helpers don't cover (`tabs`, `tree_layout`, `cfg_list`, etc.).

| Method | Effect |
| --- | --- |
| :title(s) Â· :description(s) | Modal header |
| :submit(action) Â· :submit(label, action) | Sets submit_action (and submit_label when both args supplied) |
| :on_submit(action) | Sets submit_action only |
| :cancel(action) Â· :cancel({label, action}) | Cancel action / label |
| :on_cancel(action) | Sets cancel_action only |
| :state(t) | Echo state forwarded back in the submit ctx |
| :section(title\|cfg) Â· :end_section() | Open / close a flat section. Re-calling :section() auto-closes the previous one. |
| :text Â· :textarea Â· :password Â· :number | Input fields. Args: (name, opts?) or {name=..., ...} |
| :select Â· :radio Â· :checkbox Â· :toggle Â· :kv_list | Choice / boolean / kv inputs |
| :divider() Â· :label(text\|cfg) Â· :paragraph(s) Â· :heading(s) | Static layout nodes |
| :button(cfg) | Push a button node ({label, icon, action, variant}) |
| :form_field(label\|cfg, cfg?) | Push a form_field wrapper. Two call shapes: :form_field({label="â€¦", required=true, children={â€¦}}) or :form_field("Label", {children={â€¦}, hint="â€¦"}). |
| :field(node) | Escape hatch â€” push any node table verbatim |
| :open() | Compile to a config and emit the form modal |

## File / folder picker field

Opens the standard Arbor file picker as a modal on top of the plugin form. `pick_mode` controls behaviour:

- `"file"` â€” select an existing file (default)
- `"folder"` â€” select an existing directory
- `"save"` â€” pick a destination path (typing a new filename is allowed)

```lua
{ type = "file", name = "output",  label = "Output path",
  pick_mode = "save", extensions = { "pdf" },
  placeholder = "Choose a fileâ€¦" }

{ type = "file", name = "repo_dir", label = "Repository root",
  pick_mode = "folder" }
```

## Autocomplete field

Two modes. **Static:** plugin provides an `options` list and the form filters locally with fuzzy scoring. **Dynamic:** plugin sets `source_action` and replies to each keystroke with a fresh suggestion list via `arbor.ui.set_autocomplete_options(id, options)`.

```lua
-- Static list
{ type = "autocomplete", name = "framework", id = "fwk", label = "Framework",
  options = { "react", "svelte", "vue", "solid" } }

-- Dynamic source (debounced 200ms)
{ type = "autocomplete", name = "issue", id = "issues", label = "Issue",
  source_action = "my_plugin:search_issues", debounce_ms = 200 }

arbor.events.on("my_plugin:search_issues", function(ctx)
  -- ctx.id, ctx.query, ctx.state
  local hits = search_my_tracker(ctx.query)   -- plugin-specific
  local opts = {}
  for _, h in ipairs(hits) do
    table.insert(opts, { value = h.id, label = h.title, group = h.project })
  end
  arbor.ui.set_autocomplete_options(ctx.id, opts)
end)
```

## Tags / chips field

Multi-value free-form input. Press `Enter` or `,` to commit a tag; `Backspace` with an empty input removes the last chip. Set `suggestions` to restrict input to an allowlist (acts like a multi-select).

```lua
{ type = "tags", name = "labels", label = "Labels",
  default = { "bug", "priority-1" },
  suggestions = { "bug", "enhancement", "question", "priority-1", "priority-2" },
  max = 5 }
```

## Tree selector field

Hierarchical picker for one value (`multi = false`, default) or many (`multi = true` â€” submitted as `string[]`). Set `group = true` on a node to make it a non-selectable header (still expandable and clickable-to-toggle). Each node supports:

- `value`, `label` â€” required
- `icon` â€” Lucide name shown before the label
- `tag` â€” small colored pill after the label (e.g. `"Tomcat"`)
- `tag_variant` â€” `neutral | ok | warn | error | accent | dev | prod | test`
- `description` â€” dim subtitle under the label
- `children` â€” nested array of same shape

The tree itself is **flush by default** (no border, no background, no max-height) so it blends into its container â€” ideal inside a `tree_layout` nav. Opt in to the legacy bordered look via `bordered = true` and optionally cap scroll with `max_height`.

```lua
{ type = "tree", name = "sel_cfg", expanded = true, default = "cfg-1",
  nodes = {
    { value = "grp-java", label = "Java", icon = "Coffee", group = true, children = {
        { value = "cfg-1", label = "backend",  icon = "Server",  tag = "Tomcat", tag_variant = "test" },
        { value = "cfg-2", label = "api-main", icon = "Leaf",    tag = "Spring", tag_variant = "ok"   },
        { value = "cfg-3", label = "cli-tool", icon = "Package", tag = "JAR",    tag_variant = "accent" },
      }},
    { value = "cfg-4", label = "run-app", icon = "Package" },  -- leaf top-level
  },
}

-- Bordered + scroll cap
{ type = "tree", name = "scope", label = "Scope", bordered = true, max_height = "260px",
  nodes = { --[[ ... ]] } }

-- Multi-select variant
{ type = "tree", name = "tags_tree", multi = true, nodes = { --[[ ... ]] } }
```

## FormField wrapper

`form_field` wraps any nodes with the same chrome host modals use
  for native form fields: label on top, content below, optional description
  between, hint or error underneath, leading icon, and right-aligned actions on
  the label row. The built-in input types (`text`, `select`, â€¦)
  already render their own label â€” reach for `form_field` when you
  need to label non-field content (`button`, `copy_link`,
  a row of mixed controls), enrich a single field with affordances the type
  doesn't expose (icon, action button next to the label), or surface a
  computed error/hint that doesn't come from per-field validation.

```lua
-- Wrap a copy_link with a labeled chrome
{ type = "form_field", label = "Repository ID", icon = "Hash", children = {
    { type = "copy_link", text = ctx.repo_id, font = "mono" },
}}

-- Field with a leading icon + right-aligned action
{ type = "form_field",
  label   = "Branch", icon = "GitBranch",
  actions = {
    { type = "button", label = "Fetch", icon = "RefreshCw",
      variant = "ghost", action = "my_plugin:fetch_branches" },
  },
  children = {
    { type = "select", name = "branch", options = ctx.branches },
  }}

-- Validated wrapper with an explicit error
{ type = "form_field",
  label = "Tag name", required = true,
  error = (ctx.tag_clash and "A tag with this name already exists") or nil,
  children = {
    { type = "text", name = "tag" },
  }}

-- Builder DSL
arbor.ui.form()
  :form_field({
    label    = "API token",
    optional_text = "(stored in the OS keyring)",
    children = { { type = "password", name = "token" } },
    hint     = "Tokens are scoped to this repository only.",
  })
  :submit("Save", "my_plugin:save")
  :open()
```

## IntelliJ-style tree layout

`tree_layout` is a 2-column container: navigation (typically a toolbar + a tree) on the left, content on the right. Pair it with `show_if` on per-item sections and a tree selection stored in `values[name]` to get an IntelliJ-style run/debug configurations modal.

```lua
arbor.ui.form({
  title         = "Build Configurations",
  width         = "920px", height = "620px",
  submit_label  = "Save",
  submit_action = "my_plugin:save",
  cancel_action = "my_plugin:cancel",
  state         = { cfg_ids = { "cfg-1", "cfg-2" } },
  nodes = {
    { id = "root", type = "tree_layout", nav_width = "240px",
      nav_children = {
        { id = "toolbar", type = "row", gap = 4, align = "center", children = {
          { type = "menu_button", icon = "Plus", icon_only = true,
            tooltip = "New configuration", variant = "ghost",
            options = {
              { heading = true, label = "JVM" },
              { label = "Maven",  icon = "Package", action = "my_plugin:new", extra = { tpl = "maven"  }},
              { label = "Gradle", icon = "Package", action = "my_plugin:new", extra = { tpl = "gradle" }},
              { separator = true },
              { label = "Cargo",  icon = "Package", action = "my_plugin:new", extra = { tpl = "cargo" }},
            }},
          { type = "button", icon = "Minus", icon_only = true, variant = "ghost",
            tooltip = "Remove", action = "my_plugin:remove" },
          { type = "button", icon = "Copy", icon_only = true, variant = "ghost",
            tooltip = "Duplicate", action = "my_plugin:duplicate" },
        }},
        { id = "tree", type = "tree", name = "sel_cfg", expanded = true, default = "cfg-1",
          nodes = {
            { value = "cfg-1", label = "backend",  tag = "Tomcat", tag_variant = "test" },
            { value = "cfg-2", label = "api-main", tag = "Spring", tag_variant = "ok"   },
          }},
      },
      content_children = {
        { id = "sec-1", type = "section", card = true, title = "backend",
          show_if = { field = "sel_cfg", eq = "cfg-1" },
          children = {
            { type = "text", name = "cfg_1_name", label = "Name", default = "backend" },
            { type = "text", name = "cfg_1_port", label = "Port", default = "8080"    },
          }},
        { id = "sec-2", type = "section", card = true, title = "api-main",
          show_if = { field = "sel_cfg", eq = "cfg-2" },
          children = {
            { type = "text", name = "cfg_2_name", label = "Name", default = "api-main" },
          }},
      },
    },
  },
})
```

When a `tree_layout` is the sole root of a form, the body automatically strips its padding so the split reaches the modal edges (IntelliJ look). Combine with an always-unique `id` on each node to keep Svelte's diff efficient across `arbor.ui.form.replace(...)` calls.

## Dashboard widgets â€” generic, reusable

Four leaf nodes turn the host's dashboard primitives into form-renderable
  layout. They are **generic** â€” no domain coupling â€” so any plugin
  can compose its own dashboard by combining counter tiles, a gauge, a time-series
  chart, and a sortable table without writing custom Svelte.

### counter_grid

Responsive grid of KPI tiles. Each tile shows a label, a primary value,
  and an optional sub-line. Tiles with `empty = true` (or a numeric `value` of zero) render dimmed and ignore clicks. Click a non-empty
  tile to fire `actions.select` with `{ key }`.

```lua
{ type = "counter_grid",
  min_width = 140,
  actions   = { select = "dash:filter_by_kind" },
  items = {
    { key = "open",    label = "Open issues",    value = 42, hint = "+3 this week",
      color = "var(--severity-high)"   },
    { key = "blocked", label = "Blocked",        value = 7,  hint = "owner: build",
      color = "var(--severity-critical)" },
    { key = "wip",     label = "In progress",    value = 12, hint = "median 3.2d",
      color = "var(--accent)"          },
    { key = "done",    label = "Closed today",   value = 0,  hint = "â€”" }, -- empty
  },
}
```

### score_gauge

Semi-circle gauge for a single bounded value. Coloured `segments` define the band palette; the needle rotates to the interpolated value.
  Display only â€” no actions.

```lua
{ type = "score_gauge",
  value    = 73.5,
  min      = 0,
  max      = 100,
  label    = "High risk",
  size     = "md", -- "sm" | "md" | "lg"
  segments = {
    { from = 0,  to = 25,  color = "var(--severity-info)"     },
    { from = 25, to = 50,  color = "var(--severity-medium)"   },
    { from = 50, to = 75,  color = "var(--severity-high)"     },
    { from = 75, to = 100, color = "var(--severity-critical)" },
  },
}
```

### time_series_chart

Multi-series line chart with hover-tooltip and an interactive legend. Each
  series is rendered as a coloured polyline; the x-axis is time-aware by default
  (parses `x` as ISO-8601). Set `x_kind = "linear"` for
  numeric x.

```lua
{ type = "time_series_chart",
  height      = 220,
  show_legend = true,
  series = {
    { id = "critical", label = "Critical", color = "var(--severity-critical)",
      points = {
        { x = "2026-04-29", y = 5 },
        { x = "2026-04-30", y = 4 },
        { x = "2026-05-01", y = 6 },
      } },
    { id = "high", label = "High", color = "var(--severity-high)",
      points = { { x = "2026-04-29", y = 12 }, { x = "2026-04-30", y = 10 }, { x = "2026-05-01", y = 11 } } },
  },
}
```

### data_table

Sortable, optionally clickable table. Columns control rendering via `kind`: `text` (default), `code` (monospace), `pill` (coloured chip), `datetime` (locale string), `age` (compact d/mo/y). `color` on the column tints the
  cell â€” for `pill` kind it sets the chip background, for any other
  kind it tints the text (zeros and empty cells stay un-tinted, so a "0
  critical" reading doesn't shout in red). A per-row override `_<column.key>_color` takes precedence. Sorting is
  client-side on the column's `sortable` flag. Row click fires `actions.row_click` with `{ row_id, row }`.

```lua
{ type = "data_table",
  row_key      = "id",
  height       = 360,
  initial_sort = { key = "age", dir = "desc" },
  empty        = "No findings.",
  actions      = { row_click = "dash:open_finding" },
  columns = {
    { key = "severity", label = "Severity", width = "100px", kind = "pill", sortable = true },
    { key = "title",    label = "Title",    width = "1fr",   sortable = true },
    { key = "file",     label = "File",     width = "260px", kind = "code" },
    { key = "age",      label = "Age",      width = "70px",  kind = "age",  align = "right", sortable = true },
  },
  rows = {
    { id = "f-1", severity = "Critical", _severity_color = "var(--severity-critical)",
      title = "SQL injection in /api/users", file = "api/users.go:142", age = 8 },
    { id = "f-2", severity = "High",     _severity_color = "var(--severity-high)",
      title = "Outdated TLS version",     file = "infra/tls.tf:7",     age = 31 },
  },
}
```

### filter_bar

Pairs naturally with `data_table`: a search input plus N chip
  dropdowns whose state echoes back to the plugin on every change. When `name` is set, the value
  (`{ search, filters: { [id]: string[] } }`) is
  collected into the form's submit payload and survives `form.replace` rebuilds. `actions.change` fires in real time with `{ value }` so the plugin can re-filter rows and call `arbor.ui.form.replace` with the new `data_table` rows.

```lua
{ type    = "filter_bar",
  name    = "dash_filter",
  default = { search = "", filters = {} },
  search  = { placeholder = "Search title or fileâ€¦" },
  actions = { change = "dash:filter_changed" },
  filters = {
    { id = "severity", label = "Severity", icon = "ShieldAlert",
      options = {
        { value = "Critical", label = "Critical", color = "var(--severity-critical)" },
        { value = "High",     label = "High",     color = "var(--severity-high)"     },
        { value = "Medium",   label = "Medium",   color = "var(--severity-medium)"   },
      }},
    { id = "repo", label = "Repo", icon = "GitBranch", searchable = true,
      options = {
        { value = "api", label = "api" },
        { value = "web", label = "web" },
      }},
  },
}
```

Set `search = nil` to omit the search input and render a chip-only
  bar. Filters default to multi-select; pass `mode = "single"` on a
  filter to make it radio-like (selecting one option clears the others).

All five widgets are pure leaf nodes â€” they never collect form values
  beyond the optional `filter_bar.name`, so they can drop anywhere a
  layout node fits (inside `tabs`, gated by `show_if`,
  etc.). For interactive dashboards, pair them with `arbor.ui.form.replace` to push fresh data without unmounting the
  modal.

## Menu button (dropdown)

`menu_button` renders a button with a dropdown menu anchored below it. Each option fires its own `action`, optionally merging `extra` into the payload. Use `heading = true` for bold non-clickable section labels and `separator = true` for horizontal rules.

```lua
{ type = "menu_button",
  icon    = "Plus", icon_only = true, variant = "ghost",
  tooltip = "Add new configuration",
  options = {
    { heading = true, label = "Build tools" },
    { label = "Maven",  icon = "Package", action = "my_plugin:new", extra = { tpl = "maven"  }},
    { label = "Gradle", icon = "Package", action = "my_plugin:new", extra = { tpl = "gradle" }},
    { separator = true },
    { label = "Rust",   icon = "Package", action = "my_plugin:new", extra = { tpl = "cargo" }},
    { label = "Delete all", icon = "Trash2", action = "my_plugin:wipe", variant = "danger" },
  },
}
```

With `icon_only = true` the chevron is hidden by default (cleaner toolbar look). Set `show_chevron = true` explicitly if you want it back.

## Table (multi-column) field

Editable grid with one row per entry. Submitted as `Array<Record>` keyed by `column.key`. Columns support `text`, `number`, `checkbox` and `select` editors.

```lua
{ type = "table", name = "env_rules", label = "Environment rules",
  min_rows = 1, max_rows = 10,
  columns = {
    { key = "pattern", label = "Pattern", width = "2fr", placeholder = "GIT_*" },
    { key = "action",  label = "Action",  width = "140px", type = "select",
      options = { "allow", "deny" } },
    { key = "enabled", label = "On",      width = "60px",  type = "checkbox" },
  },
  default = {
    { pattern = "GIT_*", action = "allow", enabled = true },
  } }
```

## Wizard multi-step form

Split a long form into sequential steps. Arbor replaces the Submit button with `Back` / `Next` while stepping through, and re-enables Submit on the final step. All fields across every step are collected for the final payload â€” moving between steps never loses values.

```lua
arbor.ui.form({
  title         = "Create release",
  submit_action = "my_plugin:release",
  nodes = {
    { type = "wizard", id = "wiz", steps = {
        { id = "info", label = "Info", icon = "Info", children = {
            { type = "text",   name = "version", label = "Version", pattern = "^v?%d+%.%d+%.%d+$" },
            { type = "text",   name = "title",   label = "Title" },
          }},
        { id = "scope", label = "Scope", icon = "Layers", children = {
            { type = "tags", name = "modules", label = "Modules",
              suggestions = { "api", "web", "worker" } },
          }},
        { id = "review", label = "Review", icon = "Check", children = {
            { type = "paragraph", content = "Press Submit to create the release." },
          }},
      }},
  },
})
```

Any field node supports `show_if` for conditional visibility. `show_if` supports: `eq`, `neq`, `gt`, `lt`, `gte`, `lte`, `in`/`in_values`, `nin`, and logical `and`/`or`/`not`.

## Shortcut options syntax (select / radio)

For simple cases you can pass `options` as a plain array of strings. Arbor auto-expands each entry to `{ value = s, label = s:capitalised() }`. This keeps short enum lists readable:

```lua
-- Bare-string shortcut
{ type = "select", name = "mode", label = "Mode", options = { "dark", "light", "auto" } }

-- Equivalent full form
{ type = "select", name = "mode", label = "Mode", options = {
    { value = "dark",  label = "Dark"  },
    { value = "light", label = "Light" },
    { value = "auto",  label = "Auto"  },
  }}
```

Mix-and-match is allowed: a single `options` array can hold strings and full tables together, so you can upgrade individual entries (to add `description`, for instance) without rewriting the rest.

### Live change action

Set `actions = { change = "<action>" }` on a `select` to fire a plugin
  action on every selection. The handler receives `{ value }` (the chosen option's `value`)
  alongside the rest of the form's field map. Useful for "filter" or "window picker" controls
  that should re-fetch data immediately rather than waiting for Submit.

```lua
{ type = "select", name = "range_days",
  label   = "Trend window",
  default = "30",
  options = { "30 days", "60 days", "90 days" },
  actions = { change = "dashboard:range_changed" },
}

arbor.events.on("dashboard:range_changed", function(ctx)
    local n = tonumber(ctx.value:match("(%d+)"))
    -- re-fetch with the new range, then arbor.ui.form.replace(...)
end)
```

### Rich select / multiselect options

In addition to `value` / `label`, the `select` and `multiselect` field types accept **group headers**, **separators**, and per-item visual extras
  (`icon`, `description`, `meta`, `disabled`). Plain strings and
  the legacy `{ value, label }` shape continue to work â€” these entries are purely additive.

| Entry shape | Effect |
| --- | --- |
| "plain-string" | Auto-expanded to { value = s, label = capitalised(s) }. |
| { value, label, icon?, description?, meta?, disabled? } | Selectable item. icon is a Lucide name, description renders as a small caption under the label, meta as muted right-aligned text. |
| { group, items } | Group header â€” items is a nested option list. Optional collapsible = true, default_collapsed = true. |
| { separator = true, label? } | Decorative separator strip. With a label the strip becomes an uppercase section title. |

```lua
{ type = "select", name = "config", label = "Run config",
  searchable    = true,             -- auto-on if list > 12 items
  placeholder   = "Pick a config",
  empty_message = "No configs available",
  options = {
    { group = "Project", items = {
        { value = "dev",  label = "dev",  icon = "Play",  description = "fast feedback", meta = "~3s"  },
        { value = "prod", label = "prod", icon = "Rocket", description = "release build", meta = "~45s" },
      }},
    { separator = true, label = "Templates" },
    { value = "blank",  label = "Blank profile" },
    { value = "legacy", label = "legacy",  disabled = true },
  },
}
```

### multiselect (type = "multiselect")

Same option shape as `select`, stored as `string[]`.
  Renders with a checkbox per row; the panel stays open across selections so the
  user can pick several values without re-opening it. Optional `min` / `max` bounds enable count validation on submit.

```lua
{ type = "multiselect", name = "tags", label = "Tags",
  default = { "frontend", "rust" },
  min = 1, max = 4,
  options = {
    { value = "frontend", label = "Frontend", icon = "Code"  },
    { value = "backend",  label = "Backend",  icon = "Server" },
    { value = "rust",     label = "Rust",     icon = "Hammer" },
    { value = "ops",      label = "Ops",      icon = "Wrench" },
  },
}

arbor.events.on("my_plugin:save", function(ctx)
  -- ctx.tags == { "frontend", "rust" } (Lua table)
end)
```

Both `select` and `multiselect` support full keyboard
  navigation (`â†‘` `â†“` to move, `Enter` to pick, `Home`/`End`, `Esc` to close) and an optional
  search input that filters by label and description.

## Date / datetime / time fields

Native HTML5 pickers wired into the form. Values are submitted as plain strings â€” plugins parse them as needed:

| type | Submitted format | Example |
| --- | --- | --- |
| date | ISO 8601 date | "2026-04-20" |
| datetime | Local datetime, no timezone suffix | "2026-04-20T14:30" |
| time | 24-hour time | "14:30" |

```lua
arbor.ui.form({
  title         = "Schedule deploy",
  submit_action = "my_plugin:schedule",
  nodes = {
    { type = "date",     name = "on_date",  label = "Date",     default = "2026-04-20" },
    { type = "time",     name = "at_time",  label = "Start at", default = "09:00"      },
    { type = "datetime", name = "deadline", label = "Deadline",
      min = "2026-01-01T00:00", max = "2026-12-31T23:59" },
  },
})

arbor.events.on("my_plugin:schedule", function(ctx)
  arbor.log.info("deploy " .. ctx.on_date .. " at " .. ctx.at_time)
end)
```

`min` and `max` accept strings in the same format the field submits.

## Switch / case form nodes

`switch` branches the form on the current value of another field.
  Use it instead of repeating a `show_if` cascade when several mutually exclusive fields share a controlling value â€” easier to read and cheaper to maintain.

```lua
arbor.ui.form({
  title = "Build config",
  submit_action = "my_plugin:save",
  nodes = {
    { type = "select", name = "build_type", label = "Build type",
      options = { "maven", "gradle", "npm" } },

    { type = "switch", field = "build_type", cases = {
        maven  = {
          { type = "text",   name = "maven_goals", label = "Maven goals", default = "clean package" },
          { type = "number", name = "jdk_version", label = "JDK version",  default = 21 },
        },
        gradle = {
          { type = "text",   name = "gradle_tasks", label = "Gradle tasks", default = "build" },
        },
        npm = {
          { type = "text", name = "npm_script", label = "Script", default = "build" },
        },
      },
      default = { { type = "alert", text = "Unsupported build type", variant = "warning" } },
    },
  },
})
```

Fields inside every case are **initialised at form-open time**, so switching branches does not lose previously-entered values. Only the fields in the matching branch are rendered and validated.

**Equivalent using show_if (for comparison):**

```lua
-- Verbose alternative â€” one show_if per field per branch.
{ type = "text", name = "maven_goals", show_if = { field = "build_type", eq = "maven"  } },
{ type = "text", name = "gradle_tasks", show_if = { field = "build_type", eq = "gradle" } },
-- ... and so on for every field in every branch.
```

## Tabs form node

Group related fields into `Tab` panels. The strip appears at the top; clicking a tab swaps the visible content. *All* fields in every tab are always collected on submit â€” inactive tabs are hidden with CSS, not removed from the DOM â€” so you can freely split a large form without worrying about losing values.

```lua
arbor.ui.form({
  title = "Plugin settings",
  submit_action = "my_plugin:save",
  nodes = {
    { type = "tabs", id = "main", default_tab = "general", tabs = {
        { id = "general", label = "General", icon = "Settings", children = {
            { type = "text",     name = "api_url", label = "API URL" },
            { type = "checkbox", name = "verbose", label = "Verbose logging" },
          }},
        { id = "advanced", label = "Advanced", icon = "Wrench", children = {
            { type = "number", name = "timeout_ms", label = "Timeout (ms)", default = 5000 },
            { type = "kv_list", name = "headers", label = "Extra headers" },
          }},
      }},
  },
})
```

Supported `icon` names (Lucide): `Settings`, `Wrench`, `Cog`, `Bell`, `Folder`, `Package`, `GitBranch`, `Play`, `Code`, `FileText`, `Zap`, `Users`, `Key`, `List`, `AlertTriangle`, `Info`. Omit `icon` to show a text-only tab. Omit `default_tab` to open on the first tab.

## Dynamic form updates

While a form is open, the plugin can mutate individual fields from any handler (button action, bus event, timer, etc.). Calls route via the `plugin:form-update` Tauri event and are applied only if the currently-open form belongs to the caller plugin â€” cross-plugin updates are silently ignored.

```lua
arbor.ui.form({
  title         = "Deploy",
  submit_action = "deploy:run",
  nodes = {
    { type = "select", name = "env",    label = "Environment",
      options = { "dev", "staging", "prod" } },
    { type = "select", name = "region", label = "Region", options = { "loadingâ€¦" } },
    { type = "button", label = "Refresh regions", variant = "ghost",
      action = "deploy:refresh" },
  },
})

arbor.events.on("deploy:refresh", function(ctx)
  arbor.ui.form.set_disabled("region", true)
  local regions = fetch_regions(ctx.env)     -- your own logic
  arbor.ui.form.set_options("region", regions)
  arbor.ui.form.set_disabled("region", false)
  arbor.ui.form.set_value("region", regions[1].value)
end)
```

| Helper | Applies to | Notes |
| --- | --- | --- |
| setOptions(name, opts) | select, radio, autocomplete | Accepts the same options format as at open time (strings or full tables) |
| setDisabled(name, bool) | text, textarea, number, range, date/time, select, radio, checkbox | OR'd with the field's own readonly flag |
| setValue(name, v) | all value-bearing fields | Also clears the field's inline validation error |
| replace(cfg) | whole form | Swaps the root nodes tree in-place â€” no close+reopen flicker. See below. |

> **Note** `arbor.ui.form` is both a function (open a form) and a table of helpers. The `__call` metamethod preserves the original `arbor.ui.form(config)` syntax.

### arbor.ui.form.replace â€” in-place structural swap

Rebuilds the currently-open form from a new `nodes` tree without unmounting the modal. Field values whose `name` still exists are preserved; new fields get their declared defaults; gone fields are discarded. Ideal for IntelliJ-style tree modals where `+` / `âˆ’` / duplicate must update the nav & content without a flicker.

```lua
-- Payload shape:
--   nodes       = { ... new top-level nodes (same shape as arbor.ui.form.nodes) ... }
--   state       = { ... optional â€” replaces the echoed opaque state ... }
--   set_values  = { field_name = value, ... }  -- optional â€” applied AFTER rebuild

arbor.events.on("my_plugin:new", function(ctx)
  -- 1) persist pending edits (if any) from ctx.
  apply_pending_edits(ctx)

  -- 2) create the new item in storage.
  local new_id = create_from_template(ctx.tpl)

  -- 3) rebuild the form with updated tree + content, and force the tree
  --    selection onto the newly-created id.
  local body = build_form_body(load_all(), new_id)
  arbor.ui.form.replace({
    nodes      = body.nodes,
    state      = body.state,
    set_values = { sel_cfg = new_id },
  })
end)
```

State preservation rules during a replace:

- **Values**: by field `name` â€” present in both â†’ kept; new â†’ default; gone â†’ dropped
- **Collapse / tabs / wizard**: by node `id` â€” present â†’ kept; new â†’ declared collapsed/default
- **Tree expansion**: keyed by `field::value` â€” never cleared
- **Validation errors**: referencing a gone field are dropped

Assign **stable `id`** values to your root container (and to sections you'll add/remove) so Svelte's `{#each}` diff reuses the DOM across replaces instead of remounting the subtree.

## Form state â€” opaque context echo

Pass a `state` table to `form` to carry server-side context that isn't rendered in the UI but is echoed back unchanged in every `ctx` payload (submit, button actions, cancel).

```lua
arbor.ui.form({
  title         = "Edit Config",
  submit_action = "my_plugin:update",
  state         = { config_id = "cfg-42", revision = 3 },
  nodes = {
    { type = "text", name = "label", label = "Label" },
  },
})

arbor.events.on("my_plugin:update", function(ctx)
  -- ctx.label           = user input
  -- ctx.state.config_id = "cfg-42"   (echoed unchanged)
  -- ctx.state.revision  = 3
end)
```

---

# Plugin Development â€” API: Jobs & Integrations

APIs for running background processes, defining pipelines, executing blocking shell commands, and interacting with the issue tracker.

## arbor.job â€” background jobs

Use `arbor.job` for long-running or async work. The job runs in a separate OS thread; output is streamed line-by-line to the Jobs panel. Use `arbor.terminal.exec()` only for short blocking commands.

| Function | Description |
| --- | --- |
| arbor.job.spawn(config) | Launch a background job. Returns (JobHandle, nil) on success or (nil, err) on a spawn failure (lock / app-handle). The handle is a Promise with extra .id and :cancel() â€” it resolves with the on-done context on success and rejects with it on failure. Config: name, command, cwd?, env?, category? (groups jobs into collapsible sections in the overlay), hidden? (boolean â€” when true the job is excluded from the default Jobs panel listing and the status-bar running badge; revealed by the "Show hidden" toggle), on_done_action? (string â€” sugar), on_done? (function â€” sugar) |
| arbor.job.list() | Returns a Lua table of all job records |
| arbor.job.cancel(job_id) | Kill a running job (SIGTERM / taskkill /T). No-op if the job has already finished. |

```lua
-- Promise-style: chain :ok / :err on the returned handle.
local job, err = arbor.job.spawn({
  name    = "npm build",
  command = "npm run build",
  cwd     = arbor.repo.current(),
})
if err then
  arbor.notify{ message = "Spawn failed: " .. err, level = "error" }
  return
end
arbor.log.info("started job " .. job.id)
job:ok(function(ctx)  arbor.notify{ message = "Build succeeded âœ“", level = "success" } end)
   :err(function(ctx) arbor.notify{ message = "Build failed (exit " .. (ctx.exit_code or -1) .. ")", level = "error" } end)

-- on_done / on_done_action stay as zucchero â€” they fire alongside the promise.
arbor.job.spawn({
  name           = "Cargo build",
  command        = "cargo build --release",
  cwd            = arbor.repo.current(),
  on_done_action = "my_plugin:build_done",
})
arbor.events.on("my_plugin:build_done", function(ctx)
  arbor.log.info("exit_code=" .. ctx.exit_code)
end)

-- Job sequencing via :ok chain.
local function launch_service()
  local svc = arbor.job.spawn({ name = "Server", command = "./server", category = "Services" })
  if svc then svc:ok(function(_) arbor.notify{ title = "Server stopped", message = "", level = "info" } end) end
end

-- Hidden services owned by a domain-specific panel: the job runs but does
-- not appear in the generic Jobs overlay or the status-bar running badge
-- unless the user toggles "Show hidden". Cancellation still works.
arbor.job.spawn({
  name     = "Tomcat catalina",
  command  = "./catalina.sh run",
  cwd      = repo_dir,
  category = "Services",
  hidden   = true,
})

local build = arbor.job.spawn({ name = "Build", command = "make release", category = "Builds" })
if build then
  build:ok(function(_) launch_service() end)
       :err(function(ctx) arbor.notify{ title = "Build failed", message = "exit " .. (ctx.exit_code or -1), level = "error" } end)
end

-- Inside arbor.async.run you can await sequentially.
arbor.async.run(function()
  local b = arbor.job.spawn({ name = "Build", command = "make", category = "Builds" })
  if not b then return end
  local _, berr = arbor.async.await(b)
  if berr then arbor.log.warn("build failed"); return end
  arbor.job.spawn({ name = "Tests", command = "make test" })
end)
```

## arbor.pipeline â€” pipelines

Define and run multi-stage command pipelines. Results appear in the Pipelines panel (Workflow icon in the Activity Bar). No special permissions required.

| Function | Description |
| --- | --- |
| arbor.pipeline.define(config) | Register a pipeline. Config: id, name, description?, icon?, stages[] (each with id, name, steps[]) |
| arbor.pipeline.run{ pipeline_id, cwd? } | Start a pipeline run. Returns (run_id, nil) on success, (nil, err) on failure. Optional cwd overrides the default repo-root working directory |
| arbor.pipeline.cancel(run_id) | Cancel a running pipeline (stops after the current step) |
| arbor.pipeline.list() | Return all pipeline definitions registered by this plugin |

## arbor.http â€” native HTTP client

Asynchronous HTTP via the bundled `reqwest` client â€” no shell-out, no background job, no `curl` dependency. The callback fires when the response (or an error) arrives.

| Function | Description |
| --- | --- |
| arbor.http.get(url, callback) | GET url. callback(response) receives { ok, status, body, error? }. |
| arbor.http.get(url, opts, callback) | Same with options: { headers = {...}, timeout_ms = 10000 }. |

Requires the `network` permission. Set it to a list of allowed
  hostnames in `plugin.toml` â€” exact match or registrable suffix
  (`"maven.org"` permits `search.maven.org` and itself).
  Use `["*"]` to allow any host (avoid unless strictly necessary).

```toml
# plugin.toml
[permissions]
network = ["search.maven.org", "api.github.com"]
```

```lua
arbor.http.get(
  "https://search.maven.org/solrsearch/select?q=g:%22org.springframework%22&rows=1&wt=json",
  { timeout_ms = 5000 },
  function(r)
    if not r.ok then
      arbor.log.warn("HTTP " .. r.status .. ": " .. (r.error or ""))
      return
    end
    local data = arbor.json.decode(r.body)
    arbor.log.info("Latest: " .. data.response.docs[1].latestVersion)
  end
)

-- With auth header
arbor.http.get(
  "https://api.github.com/repos/foo/bar/issues",
  { headers = { Authorization = "Bearer " .. token, Accept = "application/vnd.github+json" } },
  function(r) ... end
)
```

## arbor.terminal.exec â€” blocking shell

Requires the `terminal` permission. Always blocks the calling Lua coroutine â€” use `arbor.job.spawn` for anything that may take more than a second.

```lua
local r, err = arbor.terminal.exec{ command = "git status --short", cwd = arbor.repo.current() }
if err then
  arbor.log.error("exec failed: " .. err)
  return
end
-- r.exit_code : number
-- r.stdout    : string
-- r.stderr    : string
```

## arbor.issues â€” issue tracker

Provides synchronous Lua wrappers around the Linear and Jira APIs. The active provider for each repo is resolved transparently â€” the same code works for both trackers. Requires `issues = "read"` or `issues = "write"` in `[permissions]`.

| Function | Permission | Description |
| --- | --- | --- |
| arbor.issues.search(filters?) | issues = "read" | Linear-only. Search issues. Returns an array of issue tables. All filter fields are optional. Pass a number or identifier (e.g. "ENG-42") in query to find by id. There is no identifier filter â€” use arbor.issues.lookup for exact-id resolution that also routes to Jira when the active repo is bound to it. |
| arbor.issues.get(id) | issues = "read" | Linear-only. Fetch by Linear UUID (NOT the human identifier). For "ENG-42"-style lookups use arbor.issues.lookup. |
| arbor.issues.lookup(identifier) | issues = "read" | Routes by the active repo's issue_tracker config (linear or jira). Returns the matching issue table, nil on miss / unconfigured tracker, or (nil, err) on auth failure. Linear: candidates are filtered to the exact identifier match; Jira: hands the key straight to GET /issue/{key}. Use this whenever you have a human key like "PROJ-123". |
| arbor.issues.transition(id, status_id) | issues = "write" | Move an issue to a new workflow state. Returns updated issue. |
| arbor.issues.comment(issue_id, body) | issues = "write" | Add a comment. Returns the new comment table. |
| arbor.issues.branch_name(issue) | â€” | Pure-computation helper: generates a git branch slug from an issue table. |

```lua
local issues = arbor.issues.search({
  query        = "login",      -- title text OR ticket ID ("42", "ENG-42")
  assigneeMe   = true,
  statusIds    = { "10001", "10002" },   -- Jira status IDs or Linear workflow-state UUIDs
  labelIds     = { "bug" },             -- Jira: label name; Linear: label UUID
  issueTypeIds = { "Bug", "Story" },    -- Jira only (ignored on Linear)
  teamId       = "PROJ",               -- Jira: project key; Linear: team UUID
  limit        = 25,
})

for _, issue in ipairs(issues) do
  print(issue.identifier, issue.title, issue.status.name)
end

-- Transition issue (Jira resolves status ID â†’ workflow transition automatically)
arbor.issues.transition(issue.id, status_id)

-- Add a comment
arbor.issues.comment(issue.id, "Deployed to staging âœ“")

-- Branch name slug
local branch = arbor.issues.branch_name(issue)
-- Linear: "arb-123-fix-login-bug"
-- Jira:   "proj-456-fix-login-bug"
```

## arbor.cloud â€” object storage (cloud-storage plugin)

Lua surface exposed by the bundled **cloud-storage** plugin. The plugin itself owns the UI (sidebar tree, config form, transfer dialogs); these APIs let other plugins talk to GCS / S3 / Azure Blob through the same opendal-backed host commands. v1 only exposes GCS in the connection form, but every namespace function accepts the multi-provider `CloudConnection` shape so adding S3 / Azure later is a frontend-only change.

*Earmarked for WASM migration:* when the WASM plugin runtime lands, these calls plus the host crate (`opendal`) move into the cloud-storage plugin's own WASM crate. The Lua surface is designed to stay backwards-compatible across that move.

### Connection envelope

Every operation takes a `conn` table â€” the cloud-storage plugin builds this from its own settings, other plugins can build it manually:

```lua
local conn = {
  provider   = "gcs",                      -- "gcs" | "s3" | "azblob"
  config_id  = "cfg_abc",                  -- opaque id used for keyring scoping
  project_id = "my-gcp-project",           -- optional
  gcs = {
    -- Pick ONE of:
    method = "sa_file",       path = "/abs/path/sa.json",
    -- method = "sa_inline",  secret_ref = "gcs/cfg_abc",   -- value lives in keyring
    -- method = "adc",
    -- method = "gcloud_cli",
    -- method = "oauth",      secret_ref = "gcs/cfg_abc/oauth",
  },
}
```

| Function | Description |
| --- | --- |
| arbor.cloud.test_connection{ conn, bucket? } | Probes auth + bucket reachability. Returns (report, nil) where report = { ok, error?, auth_method?, identity? }. |
| arbor.cloud.list{ conn, bucket, prefix?, limit? } | Folder-style listing (non-recursive). Returns { items: CloudObject[], truncated }. Default limit is 200. Prefer list_stream for interactive UI â€” this command blocks until the full listing arrives. |
| arbor.cloud.list_stream{ conn, bucket, prefix?, stream_id } | Streaming list â€” fires opendal in the background and delivers batches of ~1000 entries to the cloud-storage plugin via the cloud-storage:list-chunk hook (payload: { stream_id, items, done, truncated?, error? }). Hard-capped at 20 000 entries to avoid runaway memory on huge prefixes. The caller chooses the stream_id (typically a monotonic counter) and uses it to filter stale chunks when re-navigating. |
| arbor.cloud.search_stream{ conn, bucket, root_prefix?, pattern, stream_id } | Recursive wildcard search under root_prefix (default: bucket root). Pattern grammar: * = same-segment, ** = cross-segment, ? = one non-separator. The backend extracts the literal prefix to scope opendal's listing as tight as possible, then regex-filters the rest. Results delivered to the same cloud-storage:list-chunk hook with kind = "search" in the payload (plus scanned count, matched count, truncated flag). Hard-capped at 5000 matches. |
| arbor.cloud.cancel(stream_id) | Flip the cooperative-cancel flag for a running list_stream (or transfer job). The next batch boundary breaks the loop; no further chunks are emitted. |
| arbor.cloud.stat{ conn, bucket, path } | Fetch metadata for one object: { path, is_dir, size?, etag?, content_type?, last_modified? }. |
| arbor.cloud.delete{ conn, bucket, path, recursive? } | Delete an object or, with recursive = true, every object under a prefix. |
| arbor.cloud.copy{ conn, bucket, src, dst } | Server-side object copy within a bucket. |
| arbor.cloud.download{ conn, bucket, path, ["local"] } | Stream an object to disk. Returns a (job_id, nil) tuple; progress is surfaced via arbor://cloud-progress + the JobOutputPanel. |
| arbor.cloud.upload{ conn, bucket, path, ["local"], overwrite? } | Stream a local file up. Same progress events as download. |
| arbor.cloud.sync{ conn, bucket, remote_prefix, ["local"], direction = "up"\|"down", delete? } | Recursive directory sync. With delete = true the destination is mirrored exactly; off, it's a merge. |
| arbor.cloud.secret_set(ref, value) | Write a secret string to the OS keychain under the cloud-storage namespace. |
| arbor.cloud.secret_exists(ref) | Check whether a secret is present without exposing its value. |
| arbor.cloud.secret_delete(ref) | Remove a secret. |
| arbor.cloud.oauth_start{ secret_ref, client_id, client_secret? } | Kick off the Google installed-app OAuth flow on loopback 127.0.0.1:7732. Returns the authorization URL; the host emits arbor://cloud-oauth-done {ok, error?} when the user finishes. |

### Progress hook

Every transfer/sync fires the `cloud-storage:progress` hook at ~5 Hz. Subscribe from any plugin (you don't need to be cloud-storage itself, the hook fires on whoever subscribed):

```lua
arbor.events.on("cloud-storage:progress", function(p)
  -- p = { job_id, config_id, kind = "download"|"upload"|"sync",
  --       bucket, path, bytes_done, bytes_total, speed_bps, eta_sec? }
  arbor.log.info(string.format("%s %s/%s @ %dB/s",
    p.kind, p.bytes_done, p.bytes_total, p.speed_bps))
end)
```

Completion fires `cloud-storage:job-done` with `{ job_id, ok, error? }`; OAuth flows fire `cloud-storage:oauth-done` with `{ ok, error?, secret_ref? }`.

### Example â€” list a bucket and stream a download

```lua
local conn = {
  provider  = "gcs",
  config_id = "cfg_abc",
  gcs       = { method = "adc" },
}

local page, err = arbor.cloud.list&#123; conn = conn, bucket = "my-bucket", prefix = "logs/" &#125;
if err then return arbor.log.error(err) end
for _, obj in ipairs(page.items) do
  arbor.log.info(obj.path .. (obj.is_dir and "  (folder)" or string.format("  (%d B)", obj.size or 0)))
end

local job_id, err = arbor.cloud.download&#123;
  conn   = conn,
  bucket = "my-bucket",
  path   = "logs/2026-05-11.log",
  ["local"] = "C:/temp/log.txt",
&#125;
if err then arbor.notify&#123; message = err, level = "error" &#125; end
```

---

# Plugin Development â€” Toolchains

The toolchain API manages versioned runtime installations (JDKs, Node.js, Rust toolchains). Entries are stored per-kind at `~/.config/arbor/toolchains/<kind>.json`. One entry per kind can be marked *active* â€” it is used automatically when no more specific selection is set.

## Sharing settings between plugins

Two complementary mechanisms cover cross-plugin settings access:

- **Cross-plugin reads** â€” declare `settings_read_others = true` in `[permissions]` and call `arbor.settings.read("other-plugin", "key")` / `arbor.settings.read_project(...)`.
- **Cross-plugin writes** â€” the target plugin opts in by exposing a service via `arbor.service.export({ name = ..., handler = ... })`; the caller invokes it through `arbor.service.call`. Writing without consent is not supported.
- **Shared settings UI** â€” a member plugin can contribute sections to another plugin's settings panel via `arbor.ui.contribute("<owner>:settings:section", ...)`. Each plugin still owns its own settings store.

## arbor.toolchain â€” runtime toolchains

### Permissions required

- `toolchain = "read"` â€” for `list`, `active`, `env`, `detect`
- `toolchain = "write"` â€” for `add`, `remove`, `set_active` (implies read)

```toml
# plugin.toml
[permissions]
toolchain = "write"
```

| Function | Description |
| --- | --- |
| arbor.toolchain.list(kind) | Returns all entries for kind as a Lua table. Each entry: { id, label, path, version?, active, env? } |
| arbor.toolchain.active(kind) | Returns the active entry for kind, or nil |
| arbor.toolchain.env{ kind, id? } | Returns an env table for the given entry (e.g. { JAVA_HOME = "..." }). Uses the active entry when id is omitted |
| arbor.toolchain.detect(kind) | Auto-detects installed toolchains of this kind and returns candidate entries |
| arbor.toolchain.add(kind, entry) | Register a new entry. Entry must have at least id, label, path |
| arbor.toolchain.remove(kind, id) | Remove an entry by id |
| arbor.toolchain.set_active(kind, id) | Mark an entry as the active one for its kind |

Supported kind values: `"jdk"`, `"node"`, `"rust"`. Custom kinds are stored but have no built-in detection or env injection.

```lua
-- list all registered JDKs
local jdks = arbor.toolchain.list("jdk")
for _, j in ipairs(jdks) do
  arbor.log.info(j.id .. "  " .. j.path .. (j.active and "  [active]" or ""))
end

-- get JAVA_HOME from the active JDK
local env = arbor.toolchain.env{ kind = "jdk" }  -- uses active entry
-- env = { JAVA_HOME = "/usr/lib/jvm/java-21-openjdk" }

-- add a new JDK
arbor.toolchain.add("jdk", {
  id    = "temurin21",
  label = "Eclipse Temurin 21",
  path  = "C:/Program Files/Eclipse Adoptium/jdk-21.0.3.9-hotspot",
})
arbor.toolchain.set_active("jdk", "temurin21")

-- auto-detect installed JDKs
local candidates = arbor.toolchain.detect("jdk")
for _, c in ipairs(candidates) do
  arbor.log.info("found: " .. c.label .. " at " .. c.path)
end
```

## Profile combo (variant = "profile")

Register a combo with `variant = "profile"` to render it as a colored pill badge in RepoActions instead of the standard run+chevron split button. This is useful for environment selectors (dev / prod / test) that convey state rather than triggering an action.

```lua
arbor.ui.add_graph_combo({
  id            = "active-profile",
  run_action    = "my_plugin:set_profile",
  select_action = "my_plugin:set_profile",
  target        = "repo_actions",
  variant       = "profile",
  tooltip       = "Active build profile",
  options = {
    { value = "dev",  label = "dev",  color = "dev"  },
    { value = "prod", label = "prod", color = "prod" },
    { value = "test", label = "test", color = "test" },
    { value = "none", label = "none", color = "none" },
  },
})

-- handle selection
arbor.events.on("my_plugin:set_profile", function(ctx)
  arbor.settings.project.set("active_profile", ctx.value)
end)
```

Semantic `color` values: `"dev"` â†’ green, `"prod"` â†’ red, `"test"` â†’ accent blue, `"none"` â†’ muted. Any other value falls back to the default accent style.

---

## Plugins

### bevy-brp

# Bevy Remote Protocol (BRP) — Phase 6

Connects Arbor to a running Bevy 0.18 game via the
  `bevy_remote` crate. This phase is still
  **read-only** — full editing arrives in Phase 3a/3b — but
  the entity tree now sits next to a **live** component
  panel: the selected entity is streamed over an SSE
  `world.get_components+watch` subscription, and the plugin
  auto-reconnects with backoff if the game disappears.

## Setup on the game side

Add the BRP plugins to your Bevy app:

```
use bevy::prelude::*;
use bevy::remote::{RemotePlugin, http::RemoteHttpPlugin};

App::new()
    .add_plugins((DefaultPlugins, RemotePlugin::default(), RemoteHttpPlugin::default()))
    .run();
```

Default endpoint is `http://127.0.0.1:15702`. Override with
  `RemoteHttpPlugin::default().with_address(...)` or
  `.with_port(...)` if you need to.

## Using the panels

1. Open the **Bevy** icon in the right-side ActivityBar
      (top section) to reveal the *entity tree*.
2. Hit the *Plug* icon in the panel header to connect — the
      pencil icon edits the endpoint, the refresh icon polls now, the
      unplug icon disconnects.
3. Once connected, the tree shows every entity grouped by its
      Children/ChildOf hierarchy, labelled with `Name` when
      present, with a small badge counting direct children.
4. Click any entity row — the **Bevy detail** panel auto
      reveals in the bottom dock and immediately subscribes to a
      `world.get_components+watch` stream for that entity.
      Component cards update in real time as the game mutates them; a
      green *● live* chip on the header row confirms the
      subscription is up.
5. The tree itself still refreshes once per second — BRP 0.18 doesn't
      expose a spawn/destroy event natively, so the cheapest correct
      thing is to re-poll `world.query`. When the next
      iteration of BRP ships a top-level entity-list watch, the tree
      will switch over.
6. If the game crashes or restarts, a *"Reconnecting in Xs · attempt
      N"* row appears at the top of the tree and the plugin retries
      with a 5 / 10 / 30 s backoff. Clicking *Disconnect* stops
      the loop.

## Plugin host APIs introduced in Phase 2

These are generic — usable by any plugin, not specific to BRP — but
  the BRP plugin is the first consumer. Future plugins that talk to a
  long-lived JSON-RPC + SSE service (Bevy, OBS, foundryvtt, …) can reuse
  them verbatim.

- `arbor.brp.watch(method, params?, callback) → sub_id` —
      open a server-sent-events stream against a BRP `*+watch`
      method. The callback fires repeatedly with envelopes of shape
      `{ ok = true, event = "open" }` /
      `{ ok = true, event = "data", result = … }` /
      `{ ok = true, event = "close" }`, or
      `{ ok = false, event = "error", error = { kind, message, … } }`.
      Errors with `kind = "transport"` mean the stream died and
      a reconnect is appropriate; `kind = "rpc"` means the
      server replied with a JSON-RPC error inside the stream (e.g. the
      entity disappeared) but the subscription may still keep firing.
- `arbor.brp.unwatch(sub_id)` — abort a watch. Idempotent;
      returns `true` when the id matched a live subscription.
      Subscriptions you forget are also torn down automatically on
      `arbor.brp.disconnect()` and on plugin unload, so
      forgetting an unwatch on shutdown won't leak.
- Single-game singleton: replacing the active session via a fresh
      `connect` aborts every existing watch. Plugins are
      expected to re-subscribe after a successful reconnect — see
      `start_detail_watch` in this plugin's
      `main.lua` for the canonical shape.

## Phase 5 — Diagnostics + State machine inspector

Two more sidebar panels light up the moment you connect. They share
  one cheap 1Hz polling loop that batches every relevant resource into
  a single `world.get_resources` call.

### Bevy diagnostics panel

- Open the **Activity** icon in the right ActivityBar
      to reveal a stack of cards — one per `Resource` whose
      type path matches the diagnostic patterns.
- Default patterns (substring match against the full type path):
      `Diagnostic`, `FrameTime`,
      `FrameCount`, `EntityCount`,
      `FrameTimeDiagnostic`. They cover
      `bevy_diagnostic::DiagnosticsStore` plus most third-party
      diagnostics out of the box.
- Override the list by setting `diagnostic_patterns` in
      the plugin's global settings (comma-separated substrings).
- Scalar fields render as *key : value* rows;
      nested tables collapse into *JSON* dumps so the panel
      never overflows when a resource exposes deep structures.
- Hide the panel entirely with the boolean setting
      `diagnostic_enabled`.

### State machine inspector

- Open the **GitFork** icon to see one card per state
      machine. Auto-discovery walks `world.list_resources` at
      connect time, buckets paths whose short name starts with
      `State<` or `NextState<`, and pairs
      them by their generic parameter.
- Each card shows the **current** variant and (when
      present) the **next** variant Bevy will swap to on
      the following tick.
- When `registry.schema` exposes the enum variants for
      the state type, the card lists them with the current one
      highlighted (● vs ○).
- A rolling 20-entry *Transitions* log records every variant
      change detected by diff polling — no game-side instrumentation
      required.
- Hide with the boolean setting `states_enabled`.

## Phase 5.2 — Live charts, console, variant graph

### Pin field → live chart

- Every numeric leaf in the **Bevy detail** panel grows a
      small pin icon next to the value. Click it to capture the field
      into a session-scope ring buffer (240 samples, ≈ 4 min at 1 Hz of
      diagnostics or instant at the SSE tick for entity components).
- A *Pinned* section appears at the top of the detail panel
      listing one sparkline per pinned field, plus a composite
      `<LineChart>` below them so you can compare series
      at a glance. *Hide chart* collapses to sparklines only;
      *Clear all pins* drops the entire set for that panel.
- Each sparkline shows the latest value to the right of its label
      so a glance is enough — open the chart only when you want the
      tooltip / hover-guide overlay.
- Pins are **session only**: on a game restart entity
      ids change, so persisting them would just point at wrong places.
      Re-pin after a restart.

### Diagnostic time-series

- Same pin button is rendered on every numeric value inside a
      Diagnostics card — pin `fps`, `frame_time`,
      whatever else is exposed. Samples land via the existing 1 Hz
      resource polling loop, no new traffic.
- The Diagnostics panel grows its own *Pinned* section at
      the top with the same sparkline + chart layout. Detail-panel and
      Diagnostics-panel pins are tracked independently so clearing one
      doesn't touch the other.

### BRP Console

- Bottom-section panel. Open via the **Terminal**
      icon in the right-side ActivityBar's bottom group; the panel
      docks across the full width like the Stage / Diff / Jobs drawer.
- Input format: `method [json-params]`. Examples:
      
        `rpc.discover`
        `world.query {"data":{"components":[]}}`
        `world.get_components {"entity":42,"components":["bevy_transform::components::transform::Transform"]}`
- **Autocomplete** pulls from the capability matrix
      populated at connect — every method discovered via
      `rpc.discover` is suggested as you type. `Tab`
      accepts the top match; `↑`/`↓` moves the
      highlight; `Esc` dismisses.
- **History**: `↑`/`↓` walk the
      last 50 inputs when the suggestion dropdown is hidden (or
      empty). Duplicates against the most-recent entry are skipped.
- Output is pretty-printed JSON, newest first, capped at 80
      entries. Click *Clear* in the panel footer to drop the
      buffer.

### State variant graph

- Each card in the *State machines* panel now renders an
      SVG ring: every variant is a node, every observed transition is
      an arrowed edge between two nodes.
- The **current** variant is filled accent-coloured
      with a slightly larger circle; the **pending**
      `NextState<T>` variant (when distinct) carries
      a dashed accent ring. Transitions seen most recently (top 3 by
      recency) are drawn in accent / bolder; older ones fade.
- Edge labels show the `count×` of how many times that
      transition fired during the session — quick eyeball check for
      flap loops vs one-shot moves.
- The flat *Variants* list is still available as a
      collapsible section underneath, useful when the graph gets dense.

## Phase 6 — Time-travel, drag-drop, world export

### Time-travel scrubber

- Every `world.get_components+watch` tick for the selected
      entity lands in a ring buffer (default **300 frames** —
      ≈5 s at 60 Hz, ≈5 min at 1 Hz). Bump `snapshot_capacity`
      in plugin settings to keep more history (cap 5000).
- The detail panel header gains a **range slider** when
      at least two frames are cached. Drag left to step into the past —
      the status chip switches from ●
      live to ⏸ T-N (Xs
      ago), and every field becomes read-only so a stray mutate
      can't be sent against a stale value.
- Cells whose value differs from the next-older snapshot get a
      **yellow highlight** — so "what mutated during this
      tick" pops out visually. The diff covers plain
      `field` nodes plus the compound `vec_field`
      and `color_field` widgets (the whole compound flags as
      changed if any sub-axis / channel moved).
- Click *Back to live* (inline button while scrubbing) to
      snap `scrub_offset` back to 0. The view follows the
      stream again.
- **UX note:** the offset is relative to live — at
      T-5, new captures keep arriving and the surfaced frame shifts to
      match. "Pause at this exact moment" is a Phase 6.2.x follow-up.

### Drag-drop reparent

- Drag an entity row onto another entity row to set the dropped
      entity's parent (BRP `world.reparent_entities`). The
      hover target shows an accent dashed outline; self-drops are
      blocked by the tree widget.
- Errors (cycles, dead entities) come back as a warning toast. The
      tree refreshes after a successful reparent so the new hierarchy is
      visible immediately.
- Works in both *hierarchy* and *flat* view modes; in
      flat mode you flip back to hierarchy to see the result land.

### World export / import

- Toolbar icons *Download* / *Upload* (after the view-mode
      toggle) open the native save/open dialog. JSON only in this cut —
      `.scn.ron` is queued behind Bevy 0.18 scene-format
      pinning (Phase 6.3.x).
- **Export** issues a single bulk
      `world.query` against every type in the capability
      matrix (`strict=false`, so entities only carry the
      components they actually own). Output:
      `{
  "arbor_brp_world": 1,
  "exported_at": 1715942412,
  "endpoint": "http://127.0.0.1:15702",
  "entity_count": 42,
  "type_count": 387,
  "entities": [
    { "entity": 4294967296, "components": { "Transform": {…}, … } },
    …
  ]
}`
- **Import** validates the
      `arbor_brp_world` sentinel, then fires one
      `world.spawn_entity` per record. Replies aggregate into
      a single success / partial-failure toast at the end.
- **Limitations:** entity ids are *not*
      preserved on re-import — Bevy hands new ids on spawn, so any
      cross-entity `Entity` handles in the dump are stale
      after import. Treat this as "snapshot the visible state, restore a
      similar state" rather than a perfect save/load. A full snapshot
      that preserves references would need
      `world.despawn_all` + id remapping, deferred.

### Phase 6 follow-ups still open

- Pause-at-moment (lock the scrub anchor to an absolute capture so
      new live frames don't shift the surfaced view).
- `.scn.ron` export/import (needs Bevy 0.18 scene format
      pinning + asset-tag handling).
- Import id-remapping so cross-entity `Entity` refs
      survive a round-trip.
- Drop-zones for "before/after sibling" reorder (only "drop on
      parent" is wired today).

### Phase 5.2 follow-ups still open

- Persisted-across-restart pin set keyed by Name + archetype
      (today entity ids forget on restart).
- Diagnostic-value Y-axis units (ms vs FPS toggle) inferred from
      the resource type path.
- State graph: per-edge animated pulse when the transition fires
      live (today the recent-3 highlight is the only visual signal).
- Console multi-line input + JSON-syntax validation while typing.

## ⚠ Security

BRP is unauthenticated and exposes `world.spawn_entity`,
  `world.mutate_components`, `world.despawn_entities`
  and friends. Treat the endpoint as **effective RCE on the game
  process**: anyone who can reach the HTTP port can hijack the running
  world. The default is loopback only (`127.0.0.1`); the plugin
  refuses non-loopback hosts unless you add them to the plugin's network
  allowlist explicitly via `plugin.toml`.

## Roadmap

- **Phase 2** ✓ — SSE watch streaming, granular reactivity, auto-reconnect
- **Phase 3a/3b** — schema-driven editor: `world.mutate_components`, color picker, Vec drag, Entity ref nav, Transform card
- **Phase 4**  — grouping by archetype/filter/tag, saved views
- **Phase 5 MVP** ✓ — diagnostics + state-machine inspector
- **Phase 5.2** ✓ — pinnable live charts (detail + diagnostic), state-variant graph, BRP console
- **Phase 5b** — event/message observability (requires `bevy_arbor` crate)
- **Phase 6**  — time-travel snapshot scrubber, drag-drop reparent, world export
- **Phase 7**  — AI report builder (deterministic first, LLM layer on top)

---

### ron-studio

# RON Studio

IntelliJ-style viewer and editor for [Rusty Object Notation](https://github.com/ron-rs/ron) documents. Designed for the things `cat` and a plain editor can't easily do: walk the structural tree, validate against a Rust schema you load from your own crate, diff against the original on disk, save in place or fork via Save As, and convert to / from JSON.

## How to open a document

- **Open RON file in Studio…** — pick a `.ron` file from disk.
- **Paste RON in Studio…** — paste any RON text into a textarea.

Both commands appear in the Command Palette (`Ctrl`+`K`) under the “RON Studio” group.

## The modal

Four views over the same parsed document, switched via the toolbar:

- **Tree** — structural navigation with type badges (`Struct`, `Map`, `List`, `Option`, `Char`, primitives). Lazy: only expanded nodes are pulled over IPC. Click a node to inspect its value.
- **Text** — primary edit surface. A plain editable text view with RON-aware syntax highlighting. Comments and exact formatting are preserved on save.
- **Diff** — side-by-side comparison of the original loaded text against the current edit state. Use `F3` / `Shift+F3` or the prev/next chevrons in the toolbar to jump between chunks, just like the stage panel's diff.
- **Errors** — when parsing fails, this view shows the location and message reported by the `ron` parser.

## Editing model

The Text view is the source of truth. Every keystroke triggers a debounced re-parse; the tree updates live. The Save and Save As actions write exactly what's in the textarea, so anything you typed — comments included — is preserved verbatim.

**Format** and **RON ↔ JSON** normalise the text through the parser and serialiser. They warn before running because the round-trip drops comments and any custom formatting. They never touch the file on disk until you click Save.

## Save / Save As

The Save button in the header writes the current Text view content back to the file that was opened. The dropdown next to it (**▾**) exposes Save As — pick a new path; the document then tracks the new location so subsequent Save clicks write there.

When the document was opened via *Paste*, Save is disabled (there's no source path); use Save As to commit the buffer to disk.

## Schema loaded from Rust sources

The schema panel lets you pick any `.rs` file from your project. RON Studio walks up to the enclosing `Cargo.toml` and then descends through every `mod` declaration from `lib.rs`/`main.rs` to index every `struct`, `enum` and `type` alias in the crate.

You then choose a **root type** from the dropdown (populated with the public/private types defined in the file you picked). The closure of types reachable from that root is computed and used to:

- Annotate tree rows with real Rust types (you'll see `u16`, `Option<Vec<Server>>`, etc., not just “Number”).
- Highlight unknown fields and variants the RON file uses but the schema doesn't define.
- List schema fields that the document is missing (and whether they have `#[serde(default)]`).

### What works (best-effort)

- Cross-file resolution through `mod` declarations, including `#[path = "..."]`.
- `use` aliases (including `as` renames) and `pub use` re-exports.
- Standard generics: `Option<T>`, `Vec<T>`, `HashMap<K, V>`, `BTreeMap`, tuples, fixed-size arrays.
- Transparent wrappers: `Box`, `Rc`, `Arc`, `Cell`, `RefCell`, `Mutex`, `RwLock`.
- Common `#[serde(...)]` attributes: `rename`, `default`, `skip_serializing_if`, `flatten`.

### Honest limits

- Types from other crates surface as `External(path)` — the rest of the schema still works; that single branch just isn't validated.
- Macro-generated types are invisible to the parser; they appear as `Unknown`.
- `#[cfg(...)]` is ignored: everything is indexed regardless of features.
- Generics that aren't instantiated at the root (`Foo<T>` with a free parameter) are reported as unresolved — pick a concrete root type.

## Lua API

```
arbor.ron_studio.open{ path = "/abs/path/to/config.ron" }
arbor.ron_studio.open{ text = '(name: "x", port: 8080)', title = "scratch" }
```

That's the entire surface. Everything else is driven from the modal.

## Earmarks

Like the JSON Studio and cloud-storage plugins, RON Studio relies on Rust crates pulled into the Arbor host: `ron` for parsing/serialising and `syn` for walking Rust sources. The team's direction is to migrate these heavy plugins to a subprocess-based runtime so the host stops accreting dependencies — when that lands, this plugin moves out as a self-contained binary.

---

### compile-action

# compile-action

Build runner. Auto-detects the project type and lets you compile/package it
  directly from the Activity Bar — no terminal required. Output streams in
  real time to the built-in Jobs panel.

The sibling **run-action** plugin handles launching your
  application and depends on this plugin for the build step.

## Getting Started

Open a repository. The plugin scans the working directory on every repo open
  and tab switch, detects the build system automatically, and adds a
  **Build** combo button to the **RepoActions** row
  (just below the branch name in the sidebar).

1. Open a repo — the combo appears with a default build config selected.
2. Click the **🔨** icon to build the selected config.
3. Click the **▾** dropdown arrow to switch configuration.
4. Watch output in the **Job Output Panel** (status-bar badge → ↗).

Keyboard shortcut: `Ctrl`+`F9` triggers the selected build configuration.

## Build & Run sidebar

An IntelliJ-style tree sidebar registered on the **right**
  ActivityBar (Hammer icon, "Build & Run"). The body shows two top-level
  groups for the active repo:

- **Build configurations** — every saved build config from
      *project* + *global* storage, icon per template
      (`Hammer` for Maven/Gradle, `Box` for Cargo,
      `Package` for npm, `Wrench` for Make). The
      currently-selected config carries a `default` badge. Click
      a row to run it; double-click and Enter trigger
      `compile:run`.
- The detected toolchain section — Maven, Cargo, Gradle, npm or Make
      with their canonical lifecycle / tasks / scripts. Multi-module projects
      are walked recursively: parent + children for Maven
      (`<modules>` in pom.xml), workspace members for Cargo
      (`[workspace] members`, including `"crates/*"`
      style globs — expanded by listing the directory and keeping subdirs
      that contain a `Cargo.toml`), npm `workspaces`.
      Each module exposes its own Lifecycle / Tasks / Scripts subsection.

The header has a search field and toolbar buttons populated by
  contributions — `compile-action` ships *Refresh project
  tree*, *New run configuration…*, *Settings*; other
  plugins (e.g. `run-action`) contribute their own buttons via
  the contribution model documented in *Plugin Development → API: UI →
  Tree-kind sidebars*.

### Contribution points exposed by this plugin

| Point | Used by | Purpose |
| --- | --- | --- |
| compile-action:compile:toolbar | Any plugin | Buttons in the sidebar header. |
| compile-action:compile:tree.section | e.g. run-action for "Run configurations" | Top-level section nodes appended to the tree. |
| compile-action:compile:node_action | e.g. run-action, maven-update-deps | Hover-revealed icon buttons per row, filtered by node kind / data. |
| compile-action:compile:node_decorator | Any plugin | Always-visible badge / icon decorators per row. |
| compile-action:compile:context_menu | e.g. maven-update-deps | Right-click menu items per row. |
| compile-action:compile:dependency_provider | e.g. maven-update-deps | Adds Show dependencies to the right-click menu and provides the modal's tree. |
| compile-action:compile:footer | Any plugin | Items in the sidebar footer. |

## Supported templates

Each build configuration is backed by a *template*. The template declares
  the editable fields (goals, profiles, toolchain, env, …) and generates the
  final command string on save.

| Template | Template-specific fields | Toolchain |
| --- | --- | --- |
| maven | goals, profiles, skip_tests | JDK |
| gradle | tasks, refresh_deps, init_script | JDK |
| cargo | subcommand, features, release, target, backtrace | Rust |
| npm | package_manager (npm / yarn / pnpm), script | Node.js |
| make | target | — |

Detection seeds a starter set on first repo open: `pom.xml` → 5 Maven
  configs, `build.gradle(.kts)` → 3 Gradle, `Cargo.toml` →
  5 Cargo, `package.json` → 3 npm, `Makefile` → 3 Make,
  `src-tauri/tauri.conf.json` → Cargo + frontend set, `go.mod`
  → single Make-style entry.

## Build Configurations modal

Opens from the combo's **⚙ Project settings** entry. The modal
  uses an IntelliJ-style tree layout: templates group the configs on the left,
  the selected config's editor appears on the right. A toolbar at the top of
  the nav provides `+▾` (new from template), `−` (remove) and
  `📋` (duplicate). Add / remove / duplicate update the tree and
  content *in place* — no modal flicker.

Each config stores `toolchain_id` (optional pin to a specific
  JDK / Node / Rust registered in **Plugin Preferences**). When
  empty, the active toolchain for the template kind is used; when set, its env
  (e.g. `JAVA_HOME`) is injected. Explicit keys in the config's
  `env` map always win.

## JDK / Node / Rust toolchains

Register installations in **Plugin Preferences** (gear icon in
  the Plugin Manager). The "Detection" card auto-discovers JDK via
  `JAVA_HOME`, Node from `PATH`, Rust from
  `~/.cargo/bin/cargo`. The active entry per kind is the default
  when no `toolchain_id` is pinned on the config.

## Exposed services

These services can be consumed by other plugins via `arbor.service.call`:

- `compile-action.spawn_build({ repo_path, build_id? })` — starts a build.
    Returns `{ ok, build_cfg, job_id, java_home, already_running? }`.
    If a build is already running for the repo, reuses it (`already_running = true`).
- `compile-action.get_build_config({ id })` → full config table or `nil`.
- `compile-action.list_build_configs()` → `{ project, global }`.
- `compile-action.get_selected_build_id()` → `{ id }`.
- `compile-action.is_building({ repo_path })` → `{ building, job_id? }`.
- `compile-action.resolve_java_home({ build_id? })` →
    `{ ok, java_home, build_id, template_id }` or
    `{ ok = false, error }` when the active build isn't a JVM
    template. Mirrors the same toolchain rules `spawn_build`
    uses (per-config `toolchain_id` → active JDK fallback) so
    callers like `deps-explorer` can run `mvn` /
    `gradle` under the JDK the user actually selected.

## Plugin events

Emitted via `arbor.events.emit`:

- `compile-action:build-started` —
    `{ repo_path, build_cfg, job_id }`
- `compile-action:build-done` —
    `{ repo_path, success, cancelled, exit_code, build_cfg, job_id, java_home }`

---

### cloud-storage

## Cloud Storage

Browse, upload, download and synchronise objects between Arbor and the cloud. Backed by [Apache OpenDAL](https://opendal.apache.org/), with first-class support for **Google Cloud Storage**, **Amazon S3 (and S3-compatible services like R2 / MinIO)**, and **Azure Blob Storage**.

### Setting up a connection

Click the **Cloud** icon in the right activity bar to open the sidebar, then click the **Manage connections** button (gear icon) in the toolbar. A two-pane modal opens — left rail groups every saved connection by provider, right pane edits the selected one. Click **+ Add Google Cloud Storage** to create a new connection draft. The same modal also exposes *edit* (just click a row) and *delete* (hover, click trash). Click **Save** at the bottom to commit all pending changes, **Close** to discard them. You can also reach it from the Command Palette via *Cloud Storage · Manage connections…*.

For each connection you set:

- **Name** — a friendly label shown in the picker.
- **Provider** — currently only GCS is selectable.
- **Default bucket** — shown when the sidebar first opens. You can still browse other buckets at runtime by editing the connection.
- **Project id** — optional. Most object ops don't need it.
- **Authentication** — pick one of five methods:

| Method | When to use | What we persist |
| --- | --- | --- |
| Service account file | Most common for CI / server roles. Download the JSON key from the GCP console once. | Just the path to the file. The key itself stays on disk. |
| Service account inline | You want to roam between machines and prefer the key in your keyring. | The JSON content goes to the OS keychain (cloud-storage / gcs/<config-id>). |
| Application Default Credentials (ADC) | You already ran gcloud auth application-default login or set GOOGLE_APPLICATION_CREDENTIALS. | Nothing — the file is discovered fresh on every connect. |
| gcloud CLI | You have the Google Cloud SDK installed and just want to ride your gcloud session. | Nothing — the CLI is spawned on every connect for a fresh access token. |
| OAuth user | End-user accounts where you can't issue a service account. Register a Desktop OAuth client in GCP. | Refresh token JSON in the keychain (cloud-storage / gcs/<config-id>/oauth). Access tokens are refreshed automatically. |

Use **Test connection** at the bottom of the form to validate auth and bucket reachability before saving — the report names the auth method and (when known) the service-account email or user identity behind the token.

### Browsing

- **Double-click a folder** to navigate into it; the header row shows the breadcrumb of clickable chips.
- **Double-click an object** to download — you'll be asked where to save it locally.
- **Type a path directly**: click the pencil icon at the right of the breadcrumb (or double-click anywhere on the breadcrumb band) to flip into edit mode, type something like `data/2024/chunks/` and press `Enter`. If the prefix has no objects, a non-blocking warning notification surfaces so you can tell a typo from "empty folder".
- **Listings are capped** by the *Max entries per folder* preference (Settings → Cloud Storage → Browser). The sidebar warns you if more exist; refine the breadcrumb or use Remote search.
- **Right-click any row** for the per-item context menu.

### Uploading

1. Click the **↑** button in the sidebar toolbar.
2. Pick a local file in the native picker.
3. Confirm the target key (prefilled with the current breadcrumb + filename) and whether to overwrite if it exists.

The upload streams in 256 KiB chunks; the Jobs overlay shows live progress, throughput and ETA. Cancel from there at any time and the transfer aborts on the next chunk boundary.

### Recursive sync

From the Command Palette (`Ctrl+K`):

- **Cloud Storage · Sync down** — pulls a remote prefix into a local folder.
- **Cloud Storage · Sync up** — pushes a local folder under a remote prefix.

Both flows ask you to confirm the remote prefix and offer an optional **Delete files at the destination that don't exist at the source** checkbox. With that on, the sync is a mirror (matches one side exactly); off, it's a merge.

### Background jobs

Every upload, download and sync registers a job in the Arbor Jobs registry. The status bar's spinner counts them; click it to open the floating overlay with per-job cancel buttons. The Job Output panel shows a line-per-chunk progress feed (current bytes, throughput) so you can watch large transfers without leaving Arbor.

### Security notes

- Secrets that don't fit in plain plugin settings (inline SA JSON, OAuth refresh tokens) live exclusively in your OS keychain under the service name `arbor-cloud-storage`. They are never written to disk by this plugin and never appear in exported settings.
- The OAuth flow uses installed-app PKCE with a loopback listener on **127.0.0.1:7732**. Make sure no other service is listening on that port while you authorize.
- Service-account JSON pasted into the inline form is wiped from the textarea on save and never re-displayed.

### Wildcard search

The search row at the top of the sidebar has two modes — toggle between them with the filter/globe icon on the right of the input:

- **Local** (default) — substring filter on rows already loaded; cheap, no network.
- **Remote** — input shows an accent stripe; Enter runs a wildcard search against the bucket, scoped to the current breadcrumb folder. If you start typing `*` or `?` while still in Local mode, a one-time hint appears with a *Search remote* button that promotes the query for you.

Pattern semantics:

| Pattern | Meaning |
| --- | --- |
| * | matches any sequence of characters — including /, so it walks across sub-folders |
| ** | alias of * (kept for users coming from Ant/gitignore-style globs) |
| ? | matches exactly one non-separator char |

Examples:

- `*/0` — every object whose path ends in `/0` at any depth
- `data/2024/*/chunk_*` — every `chunk_*` file anywhere under `data/2024/`
- `*error.log` — every `error.log` anywhere under the search scope
- `*.bak` — every `.bak` file under the current folder (or bucket if scope = entire)

The matcher is permissive on purpose: a single `*` walks the whole sub-tree, so `chunk_*` finds chunks in any nested folder under the current breadcrumb — you rarely need `**`.

**Scope:** by default the search runs under the current breadcrumb folder. Switch to "Entire bucket" only when needed — recursive listing of a large bucket can take seconds or minutes.

Results render as a flat list (full paths) in the sidebar. Double-click downloads; right-click works as on regular file rows (multi-select supported — useful for picking N chunks scattered across folders and feeding them to *Download chunks (custom order)*). Click the *clear ✕* chip in the breadcrumb to exit search mode and go back to browsing.

The result list is capped at 5000 matches; if you hit the cap, refine the pattern to narrow the scope.

### Bulk operations & chunk-merge

Select multiple files in the sidebar (`Ctrl`+click, `Shift`+click for ranges, `Esc` to clear). The context menu switches to bulk mode:

- **Download files…** — pick a local folder; every selected object is downloaded in parallel (capped at *parallel downloads*, set in Settings → Cloud Storage → Preferences, default 4). A floating progress modal shows per-file bars, aggregate throughput and ETA, and has a Cancel button that aborts at the next chunk boundary.
- **Delete files…** — confirms once for the whole batch, deletes each object, refreshes the listing.

When at least one **chunk-merger plugin** is installed, two extra entries appear:

- **Download chunks (auto-order by date)** — sorts the selected objects by last-modified ascending, downloads them to a temp dir (`<output>.chunks/`), then hands the local paths to the chunk-handler plugin which writes the merged output. Tie-break is alphabetic on path when timestamps match.
- **Download chunks (custom order…)** — opens a drag-reorder picker so you can place the parts manually. Same flow as above once you click Continue.

The progress modal switches to a "Merge" phase once the downloads finish; the chunk-handler can push a per-step note (e.g. *Concatenating 2/3…*) through `arbor.cloud.report_progress`.

### Extending the plugin (chunk-merger contributions)

The plugin exposes one contribution point: `cloud-storage:cloud:chunk-handlers`. A handler plugin contributes a record and exports a service:

```
arbor.ui.contribute("cloud-storage:cloud:chunk-handlers", {
  id = "binary-concat",
  payload = {
    label   = "Binary concatenation",
    icon    = "Combine",
    service = "my-chunk-plugin.merge",
  },
})

arbor.service.export("merge", function(args)
  -- args.stream_id    : string (also used for arbor.cloud.is_cancelled checks)
  -- args.inputs       : [string]  local paths in the chosen order
  -- args.output       : string    user-picked target path
  -- args.source_paths : [string]  original remote paths (for logging)
  -- args.tempdir      : string    where `inputs` live (cleaned up on ok)
  local ok, err = arbor.cloud.concat_files{ inputs = args.inputs, output = args.output }
  return ok and { ok = true } or { ok = false, error = tostring(err) }
end)
```

If more than one chunk-handler plugin is installed, the user is prompted to pick one each time. The contribution registry is re-scanned every time the sidebar opens, so installing or disabling a handler is reflected on the next visit.

### Heads-up — early version

This plugin currently ships with its heavy dependencies (opendal) bundled directly in the Arbor host binary, the same way **JSON Studio** does. When the WASM plugin runtime lands, the entire host-side cloud module is deleted and the plugin gains its own WASM crate. The Lua surface (`arbor.cloud.*`) is designed to stay backwards-compatible across that migration, but the in-process Tauri commands (`cloud_list`, `cloud_download`, …) will go away.

---

### json-studio

## JSON Studio

An IntelliJ-style inspector for JSON documents — lazy tree view, JSONPath query, and a syntax-highlighted text view, all in one modal. Designed to stay responsive on multi-megabyte payloads (parse runs through `simd-json` on the host).

### Opening a document

From the Command Palette (`Ctrl+K`):

- **Open JSON file in Studio…** — file picker, scoped to common JSON extensions.
- **Paste JSON in Studio…** — small form with a textarea, useful for ad-hoc inspection of API responses copied from the browser.

### Inside the modal

- **Tree view** — every container is loaded lazily. Click a row's chevron to expand; clicking the row itself selects the node and shows its full value in the right-hand strip. Built on the same virtualised tree the file panel uses, so 100k+ keys still scroll smoothly.
- **Text view** — pretty-printed JSON, syntax-highlighted via Prism. Read-only; use the Copy button in the header to grab the formatted text.
- **Query bar** — full [RFC 9535 JSONPath](https://datatracker.ietf.org/doc/rfc9535/) via `serde_json_path`. Type and the modal queries on the fly; click a hit to jump to that node in the tree.
    
      **Basics:** `$`, `$.foo.bar`, `$.arr[0]`, `$.arr[*]`, `$..key`
      **Filters:** `$.users[?@.age > 30]`, `$.users[?@.role == "admin"]`, `$.books[?@.price < 10 && @.in_stock]`
      **Existence / negation:** `$.users[?@.banned]`, `$.users[?!@.deleted]`
      **Slice:** `$.arr[1:5]`, `$.arr[::-1]` (reverse)
      **Multi-select:** `$[0, 2, 4]`, `$["foo","bar"]`
      **Functions:** `length(@)`, `count(@.tags[*])`, `match(@.email, ".*@.*")`, `search(@.text, "TODO")`
      **Combine:** `$..book[?@.price < 10].title` — recursive descent → filter → property
    

    **Common recipe — "find X anywhere where some descendant has Y == Z":**
    `$..*[?@.Y == "Z"].X`
    Example — given a survey where each question has a `controlType` and a `questionCode`, but nested at varying depths, get the codes of all printpdf questions:
    `$..*[?@.controlType == "printpdf"].questionCode`
    The `$..*` part is the key: `..` walks every descendant, `*` matches at every level. Just `$.foo[?...]` would only filter direct children of `foo`.
    **Shorthands** (typed as you'd think, rewritten before parsing):
    
      `foo` → `$..foo` (find `foo` anywhere)
      `.foo` / `[0]` → auto-prefix `$`
      `users[?@.x]` → `$.users[?@.x]`
    
    Results are capped at 500 hits — refine the expression for narrower results.

### Plugin authors

The Lua API is intentionally minimal — one call:

```
arbor.json_studio.open{
  text  = "{\"hello\":\"world\"}",  -- OR
  path  = "/abs/path/to/data.json",
  title = "scratch",                -- optional; defaults to filename or "JSON Studio"
}
```

Pass either `text` or `path`. The modal opens immediately; parsing happens asynchronously on the host. Only one document is held at a time — opening a second one closes the first.

### Roadmap

This plugin is the reference case for the planned WASM plugin runtime. Today the JSON parser lives in arbor's Rust core because pure-Lua parsing is too slow for multi-MB payloads; once WASM lands the parser will move into the plugin's own module and the host will lose all JSON-specific code. None of that affects the API above — `arbor.json_studio.open` stays the same.

---

### source-export

Source Export

# Source Export

**Source Export** is a workflow engine for exporting source
    code to external repositories. You define reusable *profiles*
    (per-repo) with declarative stages and steps that clone, transform,
    validate, commit and push the codebase to a customer's remote — all
    visible, resumable and auditable.

## Capabilities

- Per-repo profile CRUD with full editor UI (Info / Regole / Cronologia tabs).
- A catalog of ~45 step operations grouped by category with a searchable palette.
- ActivityBar split-button with the profile selector and primary Run action.
- Plugin-global settings (output folder, run retention, external `ju` tool path, template library).
- JSON import/export of profiles and save-as-template.
- Integration with Arbor's extended pipeline runtime: concurrency lock per
      profile, resume-from-failed-step, parallel jobs inside a stage, structured
      logging with per-run `log_level`, persistence across restarts.
- Run history per profile with Resume / Discard / Open log actions, auto-trimmed
      by the global `keep_last_n_runs` policy.

## Implementation status

Phase 2 delivers the end-to-end flow — profiles compile into live
    `arbor.pipeline` runs. The initial operation set covers the
    primary "export source to customer repo" scenario:

- **File**: delete_pattern · delete_file · copy_file · move_file ·
      create_file · touch_file · append_file · prepend_file
- **Content**: replace_in_file · replace_on_glob
- **Git**: init · clone · commit · tag · push · checkout ·
      cherry-pick · merge · submodule update
- **Build**: Offline M2 (via external `ju`)
- **Validation**: assert_file_exists · assert_cmd_exit_zero ·
      assert_env_set · assert_branch_clean
- **Flow**: log_message · notify_toast · shell_command

The remaining ops (chmod, normalize_eol, strip_bom, JSON/YAML/TOML/XML edit,
    properties_edit, env_merge, strip_comments, template_render, insert_at_anchor,
    maven/gradle/npm commands, advanced asserts, docker_build/push, lua_inline,
    set_variable, try_on_error) are declared in the palette but return
    *"not implemented"* at run time. Running a profile that contains one of
    them fails fast with a precise error listing the offending steps — add your
    own via `shell_command` until they ship.

## How profiles are stored

- **Per-repo profiles**: `<repo>/.arbor/plugins/source-export/settings.json`.
- **Plugin-global settings & templates**: `~/.config/arbor/plugins/source-export/settings.json`.
- **Runs**: persisted by Arbor's pipeline runtime under `~/.config/arbor/pipeline_runs/<run_id>.json`.

## Concurrency model

Each profile uses a lock key `<plugin>:<id>`. Only
    one run per profile can be in state `running` at a time;
    `failed`/`success`/`cancelled` runs
    release the lock immediately. A failed run remains *resumable*: its
    state is persisted to disk and you can resume it with the Resume action in
    the Cronologia tab. If a newer run has been started in the meantime, the
    resume waits until the lock is free again.

## Variables & placeholders

Every step can reference built-in variables (always available) or user
    variables declared in the profile's Info tab.

| Variable | Meaning |
| --- | --- |
| $SOURCE_PATH | Active Arbor repo (or the CLI-supplied cwd) |
| $OUTPUT_PATH | <settings.output_folder>/<profile>_<timestamp> |
| $BRANCH_SRC | Source branch of the profile (or the active branch) |
| $BRANCH_DEST | Destination branch (optional; empty when unset) |
| $PROFILE | Profile name |
| $RUN_ID | Current run id |
| $TIMESTAMP | Unix seconds at run start |
| $COMMIT_SHA | Head SHA of the source repo at run start |
| $REPO_NAME | Tail folder name of the source repo |

## Operation catalog

The palette groups ~45 steps across FILE, CONTENUTO, GIT, BUILD/DEP,
    VALIDATION, EXECUTION and FLOW categories. A search box filters in real
    time. See the Regole export tab for the live list in your build.

## Variable expansion

Every string field in every step parameter goes through the expander.
    The resolver covers built-ins, profile vars, sequence globals, per-item
    overrides, and any `set_variable` rebind — all in one
    namespace (built-ins always win on name collision).

| Form | Meaning |
| --- | --- |
| $NAME | Greedy match [A-Za-z0-9_]. Unresolved → left literal for debuggability. |
| ${NAME} | Explicit brace form — required when NAME is followed by letters/underscore. |
| ${NAME:default} | Fallback when NAME is unset or empty (bash :- semantics). Default runs verbatim to the next }; splitting uses only the first : so URLs/paths with colons in the default are fine. |
| ${NAME:} | Empty default — forces empty string when NAME is unset. |
| ${env:NAME} | System env var lookup (os.getenv). Useful to reference user-level paths like ${env:JAVA_HOME_11} without baking them into the saved profile. |
| ${env:NAME:default} | Same with fallback when the system env var is unset or empty. |
| $$ | Literal $ escape. |

## Environment overrides

The **Environment** section in the Info tab holds a list of
    process env vars applied to every shell `command` step in the
    profile. Auto-clone steps are excluded so a typo in `PATH`
    can't break the initial git clone. Values support the full expansion
    syntax above — combine `${env:JAVA_HOME_11}` with profile
    variables to pin a Java toolchain without hard-coding host paths.

```
JAVA_HOME = ${env:JAVA_HOME_11}
PATH      = ${env:JAVA_HOME_11}\bin;${env:PATH}
```

The expander is applied to `profile.branch_src` too — so you
    can write `${RELEASE_BRANCH:main}` and have the auto-clone
    stage pick the right branch at run time based on sequence variables.

## Sequences (cross-repo meta-runs)

A **Sequence** is an ordered list of profile runs — possibly
    across different repositories — that share a single output folder and a
    matrix of variable overrides. Use it when a nightly build has to export
    several projects in a specific order, or when the same profile needs to
    run with several variable combinations.

Sequences live **exclusively in the right-side ActivityBar**
    under the *Workflow* icon. Clean separation: the RepoActions combo
    is per-repo (profiles), the right sidebar is cross-repo (sequences).

### The sidebar

One compact card per sequence with title + item count + inline ghost
    icon toolbar (Run / Edit / Duplicate / Delete). The footer has
    *+ New sequence* and *History…* — the latter opens a
    full-width modal with every run across all sequences.

### The editor (3-column Items tab)

- **Info tab** — name, description, fail-fast toggle,
      output root override, and sequence-level *Global variables*.
- **Items tab**:
      
        *Palette* (left): collapsible card per known repo with
          ≥1 profile. Each profile is a click-to-add row.
        *Sequence items* (middle): ordered tree of picked items;
          click to focus.
        *Detail* (right): move up / down / remove toolbar, a
          *Profile* card with click-to-copy repo path, a *Runtime*
          card (enabled / allow-failure), and *Variable overrides for
          this item* — the per-item kv_list that layers on top of the
          sequence globals.
- **History tab** — this sequence's runs only, newest
      first.

Known repos are discovered via the workspace registry; profile lists
    are read from each repo's
    `.arbor/plugins/source-export/project.json` on demand. No
    need to open a repo as a tab before you can add its profiles.

### Matrix variables

Merge order, last writer wins:

1. Profile's own `variables` (tab Info of the profile)
2. Sequence's `Global variables`
3. Per-item `Variable overrides for this item`

Use `${NAME:default}` for optional values — makes it easy to
    express "override in some items, fall back in the rest".

### Output folder

Every item in a sequence writes its output under
    `<output_root>/NN_profile/…`. Leave `output_root`
    empty and the runtime auto-creates
    `<plugin.output_folder>/sequence_<name>_<ts>`.
    This override wins over the profile's own output logic only for the
    duration of the sequence run — the profile itself stays untouched.

### Fail-fast

Off by default. With fail-fast OFF, every enabled item runs regardless
    of the outcome of the ones before, and the run status is
    `success`, `partial`, or `failed`
    depending on the mix. With fail-fast ON, the first failure halts the
    run and the rest are marked `skipped`.

### Running a sequence

Click **Run** on a sequence card in the sidebar (or the
    Play icon in the editor's tree toolbar). The History modal opens
    automatically so you can watch per-item progress with colored status
    glyphs. Each item row is a clickable ghost button with an
    `ExternalLink` glyph — click to deep-link to that specific
    pipeline run's detail modal (graph + streaming output).

The output folder for each run appears inline — click the path to copy
    to clipboard, or the trailing `FolderOpen` icon to reveal
    it in the OS file manager.

### Persistence

Sequences are GLOBAL (stored in
    `~/.config/arbor/plugin_data/source-export/global.json`) —
    they fan out across workspaces. Per-profile data stays per-repo.
    Sequence runs are capped at the last 50 entries and persist across
    restarts; orphaned "running" runs left by a crash are swept to
    `failed` at plugin load.

---

### cipher-studio

## Cipher Studio

Encode and decode text with classical ciphers and old-school encodings.
  No AES / GCM / PGP — this plugin is for ROT13-era fun, CTF warmups and
  quick decoding of suspicious-looking strings.

### How to use

1. Open the Command Palette and run **Cipher Studio: open…**.
2. Pick an algorithm from the dropdown (grouped by family).
3. Type a key in the *Key / parameter* box if the algorithm needs one
      — the hint below the algorithm name tells you whether a key is required
      and what shape it takes.
4. Paste your text in the **Input** area, hit **Encode** or **Decode**,
      result lands in **Output**.
5. **Swap** moves Output back to Input; **Use output as input** chains
      multiple algorithms (e.g. Base64 → ROT13 → Hex).

### Algorithm catalog

#### Encoding (reversible, no key)

- **Base64** — standard RFC 4648.
- **Base32** — RFC 4648 alphabet.
- **Base16 / Hex** — uppercase hex.
- **Binary** — 8-bit groups separated by spaces.
- **Octal** — 3-digit groups separated by spaces.
- **Decimal ASCII** — space-separated code points.
- **URL** — percent-encoding.
- **HTML entities** — `&#NN;` form.
- **Unicode escape** — `\uXXXX` form.
- **Morse** — letters / digits / common punctuation; `/` = word separator.
- **A1Z26** — A=1, B=2, … Z=26.
- **Reverse** — string reversed character-wise.

#### Substitution ciphers

- **ROT13** — Caesar with shift 13. Encode == Decode.
- **ROT47** — like ROT13 but across all printable ASCII (33–126).
- **ROT5** — only digits, shift 5.
- **ROT18** — ROT13 on letters + ROT5 on digits.
- **Caesar** — generic Caesar; key = shift (integer, default 3).
- **Atbash** — A↔Z, B↔Y, … self-inverse.
- **Affine** — `E(x) = a·x + b mod 26`; key = `a,b` (a coprime with 26).
- **Vigenère** — repeating-keyword Caesar; key = word.
- **Beaufort** — Vigenère variant `E(x) = k − x mod 26`; self-inverse.
- **Autokey** — Vigenère where the plaintext extends the key.

#### Steganographic

- **Bacon** — each letter → 5-bit A/B group. 26-letter variant.

#### Transposition

- **Rail fence** — zig-zag over N rails; key = rails (integer ≥ 2).
- **Columnar** — write plaintext in rows under a keyword, read columns
      in keyword-letter order; key = keyword.
- **Scytale** — wrap text around a rod of given diameter; key = rod size.

#### Grids

- **Polybius** — 5×5 letter grid (I/J merged); pairs of digits.
- **Nihilist** — Polybius coordinates + Vigenère-style numeric sum; key = word.

#### Bonus

- **Playfair** — 5×5 keyed grid on digrams; key = keyword.
- **Bifid** — Polybius + transposition of coordinates.
- **XOR** — bytewise XOR with repeating key; output as hex.

### Notes

- All algorithms are pure-Lua — no Rust dependencies were added to the
      Arbor host. Each lives in `plugins/cipher-studio/algos/<id>.lua`
      and can be hacked / extended without touching the runtime.
- Classical ciphers preserve only letters; punctuation and whitespace pass
      through unchanged (with the exceptions noted above).
- **Do not use these for actual security**. They're all broken — that's
      the point.

---

### chunk-merger-bin

## Chunk Merger — Binary concatenation

Companion plugin to **Cloud Storage**. Reassembles a remote object that was previously uploaded in multiple parts by concatenating the downloaded chunks *byte-for-byte* in the order chosen by the user (or by last-modified date in the *auto* mode).

### When this handler fits

- Split archives (`foo.tar.gz.001`, `foo.tar.gz.002`, …) where every part is a raw byte slice of the final file.
- Manually-chunked uploads produced by a pipeline that just sliced a single blob into N parts.
- Any concatenation-safe format (HLS `.ts` playlists, log roll-ups, raw binary streams).

### When it does NOT fit

- ZIP / 7z / RAR multi-volume archives — those need the original tool to reopen the catalog.
- Video / audio container muxing (MP4, MKV, WebM) — needs `ffmpeg -c copy` or similar.
- Any format where the parts are independent files that must be merged structurally, not bytewise.

If multiple chunk-handler plugins are installed, the cloud-storage sidebar prompts you to pick one each time you run *Download chunks…*. Install handlers tailored to your specific format alongside this one — they coexist.

### Cancellation

The Stop button on the OperationsOverlay card aborts the operation cooperatively: the handler checks the shared cancel flag (`arbor.cloud.is_cancelled`) before touching the filesystem, so an early cancel never produces a partial output file. Cancelling *during* the host-side concat is bounded by the chunk size opendal uses internally and resolves on the next chunk boundary.

### Cleanup

On success, cloud-storage deletes the per-stream temp directory (`<output>.chunks/`) and the chunk files inside it. On failure the temp directory is preserved so you can inspect the partial state or retry without re-downloading.

---

### number-studio

## Number Studio

Convert integers between numeral systems — positional bases, classical
  numerals (Roman, Greek, Egyptian, Babylonian, Mayan, Hebrew) and
  non-Latin digit scripts (Arabic-Indic, Devanagari, Chinese, Thai, …).
  No Rust dependencies — every system lives in its own Lua file under
  `plugins/number-studio/algos/<id>.lua`.

### How to use

1. Open the Command Palette and run **Number Studio: open…**.
2. Pick a system from the dropdown (grouped by family).
3. Paste one integer per line in **Input**.
4. Hit **To system** to convert decimals → that system, or
      **To decimal** to parse them back. Errors on a single line are
      emitted inline as `⚠ <reason>` so a partially-bad
      batch still produces useful output.
5. **Swap** exchanges input/output, **Use output as input**
      chains conversions (e.g. decimal → Roman → decimal).

### System catalog

#### Numeric bases

- **Binary** (base 2)
- **Ternary** (base 3)
- **Quaternary** (base 4)
- **Senary** (base 6)
- **Octal** (base 8)
- **Duodecimal** (base 12) — digits 0-9, A, B
- **Hexadecimal** (base 16) — digits 0-9, A-F
- **Vigesimal** (base 20) — digits 0-9, A-J
- **Base32 (positional)** — not the RFC encoding, just radix-32
- **Base36** — digits 0-9, A-Z
- **Sexagesimal** (base 60) — comma-separated digits, e.g.
      `3661 → 1,1,1`
- **Custom base** — pick any radix 2-36 via the *Key* field

#### Historical numerals

- **Roman** — standard subtractive notation, 1-3999. Decoding
      validates that the input matches the canonical form
      (so `IIII` is rejected, expects `IV`).
- **Greek alphabetic (Milesian)** — α=1 … θ=9, ι=10 … ϟ=90,
      ρ=100 … ϡ=900; lower keraia `͵` for thousands;
      keraia `ʹ` marks the numeral. Range 1-999 999.
- **Attic Greek (acrophonic)** — Ι=1, Π=5, Δ=10, 𐅄=50, Η=100,
      𐅅=500, Χ=1000, 𐅆=5000, Μ=10 000, 𐅇=50 000. Additive.
- **Egyptian hieroglyphic** — 𓏺=1 𓎆=10 𓍢=100 𓆼=1000 𓂭=10⁴
      𓆐=10⁵ 𓁨=10⁶. Additive, 1-9 999 999.
- **Babylonian cuneiform** — base 60 positional: 𒁹=1, 𒌋=10
      composed within each sexagesimal digit; 𒑊 for zero;
      positions separated by ASCII spaces.
- **Mayan** — base 20 positional, Unicode glyphs 𝋠..𝋳;
      positions separated by spaces.
- **Hebrew (gematria)** — letter-based, 1-999, with
      `׳`/`״` punctuation; preserves the
      15→ט״ו / 16→ט״ז avoidance of sacred names.

#### Eastern digit scripts

Positional, base 10, with the script's own digit glyphs.

- **Arabic-Indic** ٠١٢٣٤٥٦٧٨٩
- **Persian / Extended Arabic-Indic** ۰۱۲۳۴۵۶۷۸۹
- **Devanagari** ०१२३४५६७८९
- **Bengali** ০১২৩৪৫৬৭৮৯
- **Gujarati** ૦૧૨૩૪૫૬૭૮૯
- **Tamil** ௦௧௨௩௪௫௬௭௮௯
- **Thai** ๐๑๒๓๔๕๖๗๘๙
- **Khmer** ០១២៣៤៥៦៧៨៩
- **Burmese** ၀၁၂၃၄၅၆၇၈၉
- **Lao** ໐໑໒໓໔໕໖໗໘໙
- **Tibetan** ༠༡༢༣༤༥༦༧༨༩

#### East Asian

- **Chinese (simplified)** — 零一二三四五六七八九十百千万亿兆;
      handles 一十X→十X at the start, internal-zero collapse to 零.
      Range 0..10¹⁶-1.
- **Chinese financial (大写)** — same algorithm with
      壹貳叁肆伍陸柒捌玖拾佰仟萬億兆 (the anti-fraud forms used on
      cheques and contracts).

#### Spelled out

- **English words** — "one hundred twenty-three thousand four
      hundred fifty-six". Range 0..10¹⁵-1, signed
      ("negative …"). Round-trips.
- **Italian (parole)** — "milleduecentotrentaquattro",
      "due milioni"; standard elisions (ventuno, ventotto) and
      accented tré at the end of compounds. Encode only.
- **NATO digits** — digit-by-digit aviation spelling
      ("Zero One Two Three Fower Fife Six Seven Eight Niner").

### Adding a new system

Drop `algos/<id>.lua` with this shape:

```
local U = require("lib.util")

return {
  id     = "myradix",
  label  = "My base-7 thing",
  group  = "Numeric Bases",
  encode = function(s) return U.per_line(s, function(l)
    return U.to_base(U.parse_int(l), 7)
  end) end,
  decode = function(s) return U.per_line(s, function(l)
    return tostring(U.from_base(l, 7))
  end) end,
}
```

Then add the `id` to `ALGO_IDS` in
`main.lua` and reload the plugin.

### Notes

- All conversions are pure-Lua. No host capability is needed.
- Lua 5.4 integers are 64-bit, so the practical ceiling is
      ≈ 9.2 × 10¹⁸. Individual systems narrow that further
      (Roman 1-3999, Hebrew 1-999, …) — limits are listed in the
      catalog above.
- Input is line-oriented: paste a column of values to batch-convert.

---

### deps-explorer

# Dependency Explorer

IntelliJ-style cross-toolchain dependency analyzer. Right-click any module
  in the **Build & Run** sidebar (owned by
  `compile-action`) and pick **Analyze dependencies…**
  to open the modal.

## What it shows

Two-pane modal modeled after IntelliJ's *Resolved Dependencies*
  view:

- **Left pane** — every resolved artifact in the dependency
      graph, one row per `group:artifact`. Each row shows the
      version(s) seen, the scope chip (compile / runtime / test / dev …),
      a *conflict* badge if the same coordinate was resolved at
      multiple versions, and an *outdated* badge with the latest
      Maven Central version when newer than the one in use.
- **Right pane** — *Usages of <selected>*:
      every path from the project root down to an occurrence of the
      currently-selected artifact, so you can see which dependency is
      pulling it in.

## Filters & grouping

- **Search** — filters by group / artifact substring.
- **Scope filter** — single-scope drop-down with all
      scopes seen in the current graph.
- **Group by** — None, Scope, or Group / namespace.
- **Outdated only** — keeps just the artifacts whose
      current version is older than the latest on Maven Central.
- **Conflicts only** — keeps just the artifacts pulled in
      at multiple versions.
- The footer shows the running totals: *N deps*,
      *M outdated*, *K with conflicts*.

## Supported toolchains

| Toolchain | Command | Latest-version registry |
| --- | --- | --- |
| Maven | mvn -B -f <pom> dependency:tree -DoutputType=text -DoutputFile=… | Maven Central maven-metadata.xml |
| Gradle | gradle dependencies --configuration runtimeClasspath (uses ./gradlew when present) | Maven Central maven-metadata.xml |
| Cargo | cargo tree --workspace --charset ascii --color never --offline --frozen --manifest-path <Cargo.toml> (auto-fallback to non-offline run when the local registry cache is missing deps) | crates.io /api/v1/crates/<name> |
| npm / pnpm | npm ls --all --json or pnpm list --depth=Infinity --json | npm registry /<pkg>/latest |

Cargo workspaces are fully supported: when the manifest declares a
  `[workspace]`, every member crate gets its own tree wrapped
  under a synthetic `<workspace> (N crates)` root. Per-member
  analyses (right-clicking a single crate in the sidebar) skip the
  `--workspace` flag and resolve only that crate's tree.

The npm parser silently drops "ghost" optional dependencies that npm
  lists but didn't actually install (the per-platform binary set of
  `esbuild`, `rollup`, etc.) so the modal isn't
  flooded with rows that have no installed version to compare against.

## Java version selection

For Maven and Gradle, the resolver runs under the **same JDK**
  the user sees in the compile combo. We ask `compile-action` for
  the resolved `JAVA_HOME` via the
  `compile-action.resolve_java_home` service — that mirrors the
  exact toolchain rules the build itself uses (per-config
  `toolchain_id`, falling back to the active JDK). When no JVM
  build is selected, the resolver falls back to whatever `java`
  is on `PATH`.

## Latest-version check

After the dependency tree lands, the plugin fetches the latest published
  version for every unique artifact in the graph from the appropriate
  registry. One backend per ecosystem, all driven through
  `arbor.http.get` (native reqwest) so the requests do
  **not** create Jobs panel entries no matter how big the
  graph is.

| Ecosystem | Endpoint | Field consumed |
| --- | --- | --- |
| Maven / Gradle | https://repo1.maven.org/maven2/<group/with/slashes>/<artifact>/maven-metadata.xml | <latest> with fallback to <release> |
| npm | https://registry.npmjs.org/<pkg>/latest | version |
| Cargo | https://crates.io/api/v1/crates/<name> | crate.max_stable_version with fallback to max_version |

Each backend keeps its own settings-backed cache. Found versions live for
  7 days; HTTP 404s ("not in this registry" — common for internal Maven
  artifacts, private npm packages, path/proc-macro Cargo deps) cache for
  24h so transient failures self-heal. A per-backend semaphore caps
  concurrency at 6–8 parallel requests so the first analysis on a 200-dep
  graph doesn't open 200 sockets at once.

Outdated rows surface a green *↑ <latest>* badge; the row
  passes the *Outdated only* filter when the in-use version compares
  lower than the published one (numeric segments compared first, fallback
  to lexicographic). Artifacts with no installed version (e.g. npm optional
  deps that didn't make it into `node_modules` on this OS) are
  never flagged as outdated — there's nothing to compare against.

The modal's **Refresh** button bypasses the on-disk tree
  cache, flushes every "miss" entry from all three registry caches
  (Maven / npm / Cargo) and re-runs the resolver — useful if a previous
  network glitch poisoned cache entries with bogus 404s.

## Performance notes

- The modal opens immediately with a *loading* state —
      first-run resolves can take 5–30s (`mvn dependency:tree`
      on a fat reactor; `cargo tree --workspace` on a 10-crate
      workspace; `npm ls --all` on a heavy node_modules).
- The plugin streams updates: tree first, registry badges 3–8s later.
- All work runs under the Job system, so the UI stays responsive and
      you can cancel the underlying jobs from the Jobs panel.
- Cargo runs `--offline --frozen` opportunistically when
      `Cargo.lock` is present, falling back to a normal run
      transparently if the local registry cache is missing deps.

## How it talks to the modal

The plugin pushes its result via
  `arbor.ui.tree.set("deps:<request_id>", …)`. The frontend
  store `depsExplorerStore` filters the unified
  `arbor://contributions-changed` event for
  `point="arbor:tree-state"`, recognises the
  `deps:` prefix and pops the modal up. Subsequent updates with
  the same id (e.g. when Maven Central data lands) patch the open modal
  reactively.

## Permissions

- `terminal = "any"` — runs `mvn`,
      `gradle`/`./gradlew`, `cargo`,
      `npm`/`pnpm`, `curl`.
- `filesystem = "sandbox"` — reads `pom.xml`,
      `Cargo.toml`, `package.json`; writes per-request
      temp files to the OS temp directory.
- `service_call = true` — calls
      `compile-action.resolve_java_home`.
- `toolchain_read = true` — fallback when no JVM build is
      active.
- `env_read = true` — inherits `PATH`,
      `JAVA_HOME`, `TEMP`.

## Limitations

- Only the first dependency tree found in the output is parsed — for
      multi-module Maven, click **Analyze dependencies** on the
      specific child module you care about.
- Cargo glob workspace members (`crates/*`) are not yet
      enumerable from the sidebar; you can still right-click any literal
      member.
- Gradle output is read from `runtimeClasspath`; falling
      back to `compileClasspath` when the former is missing.
      Other configurations need a future flag.

---

### run-action

### Run Action

Application runner that orchestrates the *build → run* flow. Depends on
  `compile-action` for the build step.

#### What it does

- Auto-detects the project type (Maven / Gradle / Rust / npm / Tauri) and
      seeds default run configurations on first open.
- Exposes a **Play** combo button in the repo actions row; the
      dropdown lists the run configs for the active repo plus any global ones.
- For non-Tomcat runs it asks `compile-action` to build first
      (via `arbor.service.call`) and launches the run command when
      the build succeeds (subscribing to the
      `compile-action:build-done` event via
      `arbor.events.on`).
- For Tomcat runs the entire *build → clean webapps → deploy WAR →
      start Tomcat* sequence is expressed as a single
      `arbor.pipeline` with the build as stage 1, so the user
      sees one unified progress timeline (and resume / cancel work across
      both phases). Catalina itself is spawned out of
      `on_pipeline_done` as a long-running service — it never
      becomes a pipeline stage.

#### Build & Run sidebar contributions

Extends the *Build & Run* tree owned by `compile-action`
  through the contribution model:

- Toolbar — leftmost **Run application** button (green
      `Play`, `Shift`+`F10`) that runs the
      currently-selected run config, plus a companion **Debug**
      button (`Bug`, `Shift`+`F9`) that launches
      the same config with the JDWP / Node inspector agent forced on.
- *Run configurations* tree section listing this repo's saved
      configs grouped by template type (Spring Boot / Tomcat / Java JAR /
      Rust / Node.js); single-group projects skip the group header.
      Each row uses the template's icon (`Leaf` for Spring,
      `Server` for Tomcat, `Package` for JAR / Node,
      `Box` for Cargo) and carries a `default` badge
      on the selected one. Double-click runs.
- Per-row hover actions on `kind = "run_config"` nodes:
      **Restart** (`RotateCw`); on Tomcat configs
      additionally **Start without building**
      (`SkipForward`, also `Ctrl`+`Shift`+`F10`)
      and **Open Tomcat root** (`FolderOpen`).
- Right-click context menu on run config rows: *Run with arguments…*
      opens the project run-settings form pre-filtered to that config.

See the contribution-point reference in the `compile-action`
  docs for what each slot accepts; the *Plugin Development → API: UI →
  Tree-kind sidebars* page covers the model in depth.

#### Supported templates

Each run configuration is backed by a *template*. The template
  declares the editable fields and generates the final command (or drives the
  Tomcat deploy) at save time.

| Template | Template-specific fields | Debug |
| --- | --- | --- |
| simple_java | jar_path, main_class, vm_args | JDWP agent flag |
| spring | tool (maven / gradle), active_profile, extra_args, vm_args | JDWP via -Dspring-boot.run.jvmArguments (maven) or JAVA_OPTS (gradle) |
| tomcat | tomcat_home, war_relative_path, vm_args | JPDA (catalina jpda run + JPDA_ADDRESS) |
| cargo | bin, features, release, args | — |
| npm | package_manager, script, args | NODE_OPTIONS=--inspect |

All Java templates expose a `debug_port` field (empty = disabled);
  npm exposes the same for Node inspector. The plugin composes the correct
  flag / env for the template at save time.

The mode chosen at launch time wins over the configured port:
  `Shift`+`F10` (*Run*) always disables the agent,
  even on Tomcat configs where `debug_port` is set;
  `Shift`+`F9` (*Debug*) always enables it, falling
  back to `5005` for JDWP / `9229` for Node when no
  port is configured.

#### Run Configurations modal

Opened from the combo's **⚙ Run settings** entry. IntelliJ-style
  tree layout: templates group the configs on the left, the selected config's
  editor appears on the right. Toolbar: `+▾` (new from template),
  `−` (remove), `📋` (duplicate). Add / remove / duplicate
  update the tree in place — no modal flicker. The **Behaviour**
  card at the top of the content is always visible and controls the
  auto-stop policy (inherit / always stop / never stop).

#### Per-config “Build before run”

- **Use currently-selected build** (default) — runs the build
      config currently active in the *compile* combo, then the run.
- **Skip build** — default for commands that compile inline
      (`cargo run`, `mvn spring-boot:run`, `npm run dev`,
      `cargo tauri dev`).

#### Keybindings

| Shortcut | Action |
| --- | --- |
| Shift+F10 | Run the selected configuration (debug agent forced off). |
| Shift+F9 | Debug the selected configuration (debug agent forced on, default port if unset). |
| Ctrl+Shift+F10 | Tomcat only — start catalina against the existing WAR (skip build & deploy). |

#### Relationship with compile-action

`run-action` declares `compile-action` as a hard
  dependency and uses two cross-plugin APIs:

- `arbor.service.call("compile-action.spawn_build", { repo_path, build_id, silent? })`
    — starts a build as a Job and resolves with the build config snapshot.
    Used for non-Tomcat runs. The optional `silent` flag (passed
    by `run-action`) suppresses `compile-action`'s own
    success / failure notifications, so the run flow emits a single unified
    set instead of layering two.
- `arbor.service.call("compile-action.resolve_build", { repo_path, build_id })`
    — resolves a build config + env + cwd + command WITHOUT spawning anything.
    Used for Tomcat runs so the build can run as the first stage of the
    deploy pipeline (instead of as a separate Job).
- `arbor.events.on("compile-action:build-done", fn)`
    — fired by `compile-action` with `{ repo_path, success, exit_code, build_cfg, java_home }`
    whenever any build Job finishes. Used by the non-Tomcat path to dequeue
    and execute the queued run.

---

### run-monitor

## Run Monitor

A dedicated bottom panel for running application services. Adds a
  **Server** icon to the right ActivityBar (bottom group); clicking it
  opens a panel listing every job in the `Services` category with its
  current status, elapsed time, and per-row actions.

### Behaviour

- Click a service card → opens the streaming *Job Output* panel
      focused on that job.
- Per-row *Stop* button cancels the underlying process.
- The list refreshes every 1.5 s plus on every panel open.

### Interaction with run-action

At load time this plugin calls `run-action.set_hide_services` with
  `value = true`. From then on, every `Services`-category
  job that *run-action* spawns enters the registry as
  `hidden = true`, so the global Jobs overlay and the status-bar
  job badge skip them. They remain visible *here*, plus under the
  "Show hidden" toggle on the Jobs overlay as an escape hatch.

run-action also consults `arbor.meta.plugin_loaded("run-monitor")`
  at every spawn site as a synchronous fallback, so even if the async
  `set_hide_services` call hasn't landed yet (startup race,
  run-action reloaded mid-session, host mutex contention), service jobs are
  still spawned hidden whenever this plugin is enabled.

Disabling or unloading *run-monitor* automatically restores the
  default behaviour: the unload hook calls
  `set_hide_services(false)`, and run-action's subsequent service
  jobs go back into the global overlay.

### Permissions

- `service_call = true` — to call
      `run-action.set_hide_services`.

### Notes

- This panel monitors *any* plugin's `Services` jobs,
      not only run-action's. The hide flag, however, is run-action specific:
      other plugins continue to surface their service jobs in the global
      overlay unless they implement an equivalent hand-off.
- There is no UI in run-action's settings to flip
      `hide_services` manually — the only way to toggle it is to
      enable / disable this plugin (or another plugin that calls the same
      service).

---
