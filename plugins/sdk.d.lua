---@meta
-- =============================================================================
-- Arbor Plugin SDK — EmmyLua type definitions
-- Compatible with lua-language-server (sumneko/LuaLS)
--
-- This file is a pure declaration file (---@meta).
-- It provides autocomplete and type checking for the `arbor` global API
-- injected into every Arbor plugin sandbox.
--
-- Hook naming:
--   Every built-in hook is `<product>:<event>` — `corvus:commit`,
--   `garrulus:note_saved`, `arbor:plugin_load`. Subscribe with
--   `arbor.events.on(name, fn)`; there is no `arbor.on`. The `<product>:`
--   prefix is optional and resolves against the product hosting the plugin,
--   so `on("commit", fn)` inside a Corvus plugin means `corvus:commit`;
--   names the host runtime owns fall back to `arbor:`, so `on("plugin_load",
--   fn)` is `arbor:plugin_load` under every product. A `*` anywhere in the
--   name makes it a pattern, matched as written: `on("garrulus:*", fn)`.
--   In plugin.toml the `[hooks]` keys are the same names, quoted:
--   `"corvus:commit" = true` (a colon is not legal in a bare TOML key).
--
-- Built-in modules available via require():
--   require("arbor.schema")        → arbor.Schema
--   require("arbor.async")         → arbor.Async
--   require("arbor.event")         → arbor.Event
--   require("arbor.core.edit")     → arbor.CoreEdit     (pipeline JSON/YAML/TOML/XML ops)
--   require("arbor.core.assert")   → arbor.CoreAssert   (pipeline assertions)
--
-- Sandboxed standard library notes:
--   • io table is removed — use arbor.fs instead
--   • os.execute, os.exit, os.remove, os.rename, os.tmpname are removed
--   • os.getenv requires env_read in plugin.toml. Accepts:
--       env_read = true             (all variables readable)
--       env_read = false            (os.getenv is removed)
--       env_read = ["PATH", ...]    (allowlist — others return nil)
--   • require() is restricted to files within the plugin's own directory
-- =============================================================================


-- =============================================================================
-- Shared data types
-- =============================================================================

---@class arbor.FsEntry
---@field name    string  File or directory name
---@field is_file boolean True when the entry is a regular file
---@field is_dir  boolean True when the entry is a directory

---@class arbor.ExecResult
---@field exit_code integer Process exit code (0 = success)
---@field stdout    string  Standard output
---@field stderr    string  Standard error

---@class arbor.JobInfo
---@field id          string
---@field name        string
---@field plugin_name string
---@field command     string
---@field started_at  integer Unix timestamp in seconds
---@field status      "running"|"success"|"failed"|"cancelled"

---@class arbor.JobResult
---@field success   boolean
---@field exit_code integer
---@field job_id    string

---@class arbor.ComboOption
---@field value  string          Option value (passed to the action)
---@field label  string          Display label
---@field group  string|nil      Section header — renders as a non-selectable divider
---@field color  string|nil      Semantic color hint for profile pills ("dev"|"prod"|"test")
---@field action boolean|nil     When true, clicking this option fires run_action directly
---                              (opens a modal/settings) and does NOT become the persisted
---                              selection. Renders in a visually separated footer — same
---                              pattern as "New Workspace" in the workspace dropdown.

---@class arbor.SchemaRule
---@field required boolean|nil  Fail when field is nil or empty string
---@field pattern  string|nil   Lua pattern — field must match
---@field min_len  integer|nil  Minimum string length
---@field max_len  integer|nil  Maximum string length
---@field min      number|nil   Minimum numeric value
---@field max      number|nil   Maximum numeric value
---@field message  string|nil   Custom error message shown on failure


-- =============================================================================
-- Hook context tables  (passed as `ctx` to arbor.events.on callbacks)
-- =============================================================================

---@class arbor.HookCtxRepo
---@field tab_id string  Internal tab identifier
---@field path   string  Absolute repository path

---@class arbor.HookCtxTabSwitch
---@field tab_id string
---@field path   string|nil  nil when no repo is open in the new tab

---@class arbor.HookCtxCommit
---@field tab_id string
---@field oid    string  Full 40-character commit SHA

---@class arbor.HookCtxPreCommit
---Payload of the `corvus:pre_commit` hook. Handlers may **veto** the commit
---by returning a non-empty string from the handler — the host aborts
---the commit and surfaces the string back to the user. Returning nil
---(or no value) lets the commit proceed. Multiple plugins each see the
---same payload; every veto is concatenated into the final error.
---@field tab_id  string
---@field message string   Proposed commit message
---@field amend   boolean  True when the commit will amend HEAD

---@class arbor.HookCtxPush
---@field tab_id string
---@field branch string

---@class arbor.HookCtxCheckout
---@field tab_id string
---@field branch string

---@class arbor.HookCtxFetch
---@field tab_id string

---@class arbor.HookCtxFlow
---@field tab_id string
---@field name   string  Feature / release / hotfix branch name

---@class arbor.HookCtxView
---Payload of the `arbor:view_open` / `arbor:view_close` hooks, fired on the owning
---plugin when one of its `arbor.ui.add_view` views is opened / closed. Respond
---to `arbor:view_open` by pushing the body with `arbor.ui.set_panel_content`.
---@field view_id string   Id of the view (matches the `add_view` config)
---@field label   string|nil  Display label of the view

---@class arbor.HookCtxFileOpened
---Payload of the `bennu:file_opened` hook — the editor's active file changed.
---
---Fired on a tab switch, on opening a file, and on reopening one from history: any
---way the file under the caret changes. A panel about the file being edited follows
---this; without it a preview opened on one source keeps showing it while you edit
---another. `bennu:file_closed` is the other end and carries no context.
---@field path string      Absolute path of the file now being edited
---@field name string      File name, without the directory
---@field ext  string|nil  Lower-case extension without the dot, when the name has one

---@class arbor.HookCtxVault
---Payload of the `garrulus:vault_opened` hook (Garrulus note vaults).
---@field vault_id   string   Stable vault id — also the key of its index cache
---@field path       string   Absolute vault root on disk
---@field name       string   Display name shown in the vault switcher
---@field note_count integer  Notes indexed at open

---@class arbor.HookCtxVaultClosed
---Payload of the `garrulus:vault_closed` hook. Not fired when no vault was open.
---@field path string  Absolute root of the vault that closed

---@class arbor.HookCtxVaultNote
---Payload of the vault-note hooks `garrulus:note_created`, `garrulus:note_saved` and
---`garrulus:note_deleted`. `path` is always **vault-relative** with POSIX
---separators — these are Garrulus notes, not git notes, so there is no `tab_id`
---and no `commit_oid`. The `corvus:` hooks of the same event name are the
---git-note ones and carry `{ tab_id, commit_oid, namespace }` instead.
---@field path     string       Vault-relative path of the note
---@field bytes    integer|nil  Bytes written — `garrulus:note_saved`, ordinary save only
---@field source   string|nil   "trash" (garrulus:note_created, restored) | "conflict" (garrulus:note_saved, remote side adopted)
---@field trash_id string|nil   Trash entry id — `garrulus:note_deleted` only, for a later restore

---@class arbor.HookCtxVaultNoteRenamed
---Payload of the `garrulus:note_renamed` hook. The `[[wikilinks]]` that pointed at the
---note are rewritten as ordinary saves by the rename flow, not by this hook.
---@field old_path string  Vault-relative path the note had
---@field new_path string  Vault-relative path the note now has

---@class arbor.HookCtxTypeApplied
---Payload of the `garrulus:type_applied` hook. Fires even when the note already carried
---that type and nothing was rewritten.
---@field path string  Vault-relative path of the note
---@field type string  Note type id that was applied

---@class arbor.HookCtxSync
---Payload of the vault sync hooks `garrulus:sync_started` and `garrulus:sync_done`. Never
---fired by the background probe: the probe is read-only, and every sync in
---Garrulus is a handler a user's click reached.
---@field op        string       "pull" | "push" | "sync" (pull then push)
---@field notes     integer|nil  Push batch size — `garrulus:sync_started`, push only; 0 means "everything changed"
---@field applied   integer|nil  Notes the pull brought in — `garrulus:sync_done`, pull and sync only
---@field conflicts integer|nil  Conflicts the pull could not merge — `garrulus:sync_done`, pull and sync only; non-zero means a "sync" skipped its push half

---@class arbor.HookCtxSyncConflict
---Payload of the `garrulus:sync_conflict` hook, fired before the matching
---`garrulus:sync_done`. No merge marker is ever written into a note: each remote side
---lands as its own file beside it and the user resolves from the Conflicts panel.
---@field count integer  Number of conflicted notes

---@class arbor.HookField
---@field name        string   Field name in the ctx table
---@field type        string   "string" | "number" | "boolean" | "string[]" | "object"
---@field required    boolean  False if the field is optional / context-dependent
---@field description string

---@class arbor.HookDef
---@field name        string             Fully-qualified hook name, "<product>:<event>" (e.g. "corvus:commit", "arbor:repo_open")
---@field category    string             Grouping for docs (e.g. "repo", "branch", "pipeline")
---@field description string
---@field ctx         arbor.HookField[]  Ordered list of fields the ctx table carries


-- =============================================================================
-- Levels
-- =============================================================================
-- Notification level strings accepted by `arbor.notify{ level = ... }`.
-- ----------------------------------------------------------------------------

---@alias arbor.NotifyLevel "info"|"success"|"warning"|"error"

-- arbor.log.LEVELS — symbolic constants matching the level strings used by
-- the logging functions and arbor.notify. Use these instead of bare string
-- literals when you want autocomplete and a single source of truth.

---@class arbor.LogLevels
---@field DEBUG string  "debug"
---@field INFO  string  "info"
---@field WARN  string  "warn"
---@field ERROR string  "error"


-- =============================================================================
-- arbor.log
-- =============================================================================

---@class arbor.Log
---@field LEVELS arbor.LogLevels
local Log = {}

---Emit a debug log line (visible when RUST_LOG=debug).
---@param message string
function Log.debug(message) end

---Emit an info log line.
---@param message string
function Log.info(message) end

---Emit a warning log line.
---@param message string
function Log.warn(message) end

---Emit an error log line.
---@param message string
function Log.error(message) end


-- =============================================================================
-- arbor.json
-- =============================================================================

---@class arbor.Json
local Json = {}

---Encode a Lua value (table, string, number, boolean, nil) to a JSON string.
---@param  value any
---@return string|nil encoded  nil on error
---@return string|nil err
function Json.encode(value) end

---Decode a JSON string to a Lua value.
---@param  s string
---@return any|nil  value  nil on error
---@return string|nil err
function Json.decode(s) end


-- =============================================================================
-- arbor.json_studio
-- =============================================================================

---@class arbor.JsonStudio
local JsonStudio = {}

---Open the JSON Studio modal on a parsed document. Pass either `text` or
---`path` (host reads the file). The modal renders host-side: lazy tree,
---JSONPath query, syntax-highlighted text view. Only one document is held
---at a time — opening a second closes the first.
---
---Backed by simd-json on the host. Earmarked to migrate to a self-contained
---WASM plugin once that runtime lands; the API stays the same.
---
---@param opts table
---  text  : optional string — JSON document body
---  path  : optional string — absolute path to a JSON file on disk
---  title : optional string — header label (defaults to filename or "JSON Studio")
---
---Example:
---  arbor.json_studio.open({ path = "/abs/data.json" })
---  arbor.json_studio.open({ text = response_body, title = "API response" })
function JsonStudio.open(opts) end


-- =============================================================================
-- arbor.fs
-- =============================================================================

---@class arbor.Fs
local Fs = {}

---Return true if path exists (file or directory).
---Requires `fs = "read"` (or `"write"`) in plugin.toml.
---@param  path string
---@return boolean
function Fs.exists(path) end

---Return true if path is a regular file.
---@param  path string
---@return boolean
function Fs.is_file(path) end

---Return true if path is a directory.
---@param  path string
---@return boolean
function Fs.is_dir(path) end

---Read the full contents of a file as a UTF-8 string. A leading UTF-8 BOM is
---stripped. Fails on input that isn't valid UTF-8 — use `read_bytes` for
---non-UTF-8 / binary files or when the BOM matters.
---@param  path string
---@return string|nil content
---@return string|nil err
function Fs.read(path) end

---Read the exact bytes of a file as a Lua string, with NO UTF-8 validation
---and NO BOM stripping. Use for non-UTF-8 / binary content, or when a check
---needs to see the raw bytes (encoding / mojibake / BOM inspection).
---@param  path string
---@return string|nil content
---@return string|nil err
function Fs.read_bytes(path) end

---Write content to a file, creating any missing parent directories.
---`content` must be valid UTF-8 — use `write_bytes` for raw / non-UTF-8 bytes.
---Requires `fs = "write"` in plugin.toml.
---@param path    string
---@param content string
function Fs.write(path, content) end

---Write the exact bytes of `content` (a Lua string taken as a raw byte
---buffer, NO UTF-8 validation), creating any missing parent directories.
---Pairs with `read_bytes` for byte-faithful read-modify-write of non-UTF-8 /
---legacy-encoded / binary files. Requires `fs = "write"` in plugin.toml.
---@param path    string
---@param content string
---@return boolean|nil ok
---@return string|nil  err
function Fs.write_bytes(path, content) end

---List directory contents. Returns an array of { name, is_file, is_dir }.
---@param  dir string
---@return arbor.FsEntry[]
function Fs.list(dir) end

---Join path segments using the OS path separator. No filesystem permission needed.
---@param  ... string
---@return string
function Fs.join(...) end

-- --- Low-level primitives added for plugin-authored pipeline LuaOps ---------
-- Each requires the matching permission (read for reads, write for mutating
-- ops). Functions throw on failure.

---Append raw bytes to a file. Creates the file and any missing parent
---directories. Writes are UTF-8 **without BOM** regardless of how the caller
---content is encoded.
---@param path    string
---@param content string
function Fs.append(path, content) end

---Create an empty file (and missing parents) when `path` does not exist, or
---bump its `mtime` to now when it does. Mirrors POSIX `touch`.
---@param path string
function Fs.touch(path) end

---Rename / move a file or directory. Atomic on same volume. When
---`overwrite = true` an existing destination is removed beforehand (lets
---Windows' `rename` succeed — default semantics refuse to replace).
---@param src       string
---@param dest      string
---@param overwrite boolean|nil   Default: false
function Fs.move(src, dest, overwrite) end

---Recursively walk `root` and collect paths whose **basename** matches
---`pattern`. Glob syntax: `*` / `?` / `[abc]` / `[a-z]` (negate with `[!...]`).
---Directories are skipped unless `opts.include_dirs = true`.
---@param root    string
---@param pattern string
---@param opts    { include_dirs: boolean|nil, max_depth: integer|nil }|nil
---@return string[]
function Fs.glob(root, pattern, opts) end


-- =============================================================================
-- arbor.text — regex-backed string helpers (regex crate, PCRE-ish)
-- =============================================================================

---@class arbor.Text
local Text = {}

---Replace occurrences of `pattern` in `content`. Regex by default
---(`replacement` may reference groups with `$1`, `$name`). With
---`plain = true` the pattern is treated as a literal string.
---@param content     string
---@param pattern     string
---@param replacement string
---@param plain       boolean|nil   Default: false (regex mode)
---@return string new_content
---@return integer count            Number of substitutions performed
function Text.replace(content, pattern, replacement, plain) end

---Test whether `content` contains `pattern`. Regex by default.
---@param content string
---@param pattern string
---@param plain   boolean|nil   Default: false (regex mode)
---@return boolean
function Text.contains(content, pattern, plain) end

---Return every non-overlapping regex match found in `content` as strings.
---@param content string
---@param pattern string
---@return string[]
function Text.find_all(content, pattern) end

---Regex-escape a literal string so it can be pasted verbatim into a pattern.
---@param s string
---@return string
function Text.escape(s) end

-- --- Structured file edits (serde-backed) -----------------------------------
-- Require `fs = "write"` (sandboxed to the active repo by default; widen with
-- `fs_scope = ["*"]` for unrestricted access). Intermediate containers are
-- auto-created for missing path segments.

---Edit a JSON file at a dotted path. `value` can be any Lua value; strings
---that parse as JSON (`42`, `true`, `{"x":1}`) are promoted to their JSON
---shape, otherwise stored as a string.
---Path syntax: `$.foo.bar`, `foo.bar`, `items.0.name`, `servers[1].host`.
---@param path   string  absolute path to the JSON file
---@param jpath  string  dotted / jq-style path
---@param value  any     Lua value (table / string / number / boolean)
---@param pretty boolean|nil  pretty-print output. Default: true
function Fs.json_set(path, jpath, value, pretty) end

---Edit a YAML file at a dotted path. Same syntax as `json_set`. Uses
---`serde_yaml` internally; comments are NOT preserved on rewrite.
---@param path   string
---@param ypath  string
---@param value  any
function Fs.yaml_set(path, ypath, value) end

---Edit a TOML file at a dotted path. Comments are NOT preserved.
---@param path   string
---@param tpath  string
---@param value  any
function Fs.toml_set(path, tpath, value) end

---Edit an XML file via a minimal XPath-like expression. Sets the text of the
---matching element, or the value of the targeted attribute.
---Supports: `/a/b/c`, `//c`, `/a/b/@attr`, `/a/b[@k='v']/c`.
---Does not handle XML namespaces or multi-element mutation — use
---`shell_command` + `xmlstarlet` for complex documents.
---@param path  string
---@param xpath string
---@param value string  attribute value or element text
function Fs.xml_set(path, xpath, value) end


-- =============================================================================
-- arbor.repo
-- =============================================================================

---@class arbor.Repo
local Repo = {}

---Return the absolute path of the currently open repository, or nil if none.
---@return string|nil
function Repo.current() end

---Return the current branch name (short ref). Requires `git = "read"` (or higher).
---@return string|nil
function Repo.branch() end

---Return true if the working tree has any uncommitted changes. Requires `git = "read"` (or higher).
---@return boolean
function Repo.is_dirty() end

---Return the URL of a named remote, or nil if not found. Requires `git = "read"` (or higher).
---@param  name string  Remote name, e.g. "origin"
---@return string|nil
function Repo.remote(name) end

---Fetch "origin" for the currently active UI tab and emit arbor://graph-refresh
---so the commit graph reloads. Returns false silently when no tab is active,
---the tab has no "origin" remote, or the fetch fails.
---Requires `git = "write"` (or higher).
---@return boolean success
function Repo.fetch_active_tab() end

---@class arbor.BranchInfo
---@field name      string   Short name, e.g. "main" or "origin/develop"
---@field is_remote boolean  True for refs/remotes/* branches
---@field is_head   boolean  True when this is the currently checked-out branch

---List local + remote branches of the currently active repository.
---Requires `git = "read"` (or higher). Empty table when no repo is open.
---@return arbor.BranchInfo[]
function Repo.branches() end

---@class arbor.TagInfo
---@field name   string
---@field target string|nil   Target SHA (when resolvable)

---List tags of the currently active repository.
---Requires `git = "read"` (or higher). Empty table when no repo is open.
---@return arbor.TagInfo[]
function Repo.tags() end

---@class arbor.RepoCommitsOptions
---@field from?           string   Exclusive lower bound (commit/tag/branch). Default: walk to root.
---@field to?             string   Inclusive upper bound. Default: "HEAD".
---@field limit?          integer  Max commits returned. Default: 1000.
---@field include_merges? boolean  When false, skip commits with >1 parent. Default: true.

---@class arbor.CommitInfo
---@field oid          string
---@field short_oid    string   First 7 chars of `oid`.
---@field summary      string   First line of the commit message.
---@field message      string   Full message (subject + body).
---@field author_name  string
---@field author_email string
---@field author_time  integer  Unix epoch seconds.
---@field parents      string[] Parent OIDs.

---List commits in a range, newest-first by author time.
---Returns `(commits, nil)` on success and `(nil, err)` when revparse / revwalk fails.
---Requires `git = "read"` (or higher). Empty range yields an empty table.
---@param  opts? arbor.RepoCommitsOptions
---@return arbor.CommitInfo[]|nil commits
---@return string|nil             err
function Repo.commits(opts) end

---Return the relative paths of files in the working tree that are
---untracked AND not ignored. Useful for housekeeping plugins (e.g.
---gitignore-suggester) that propose new ignore entries.
---Requires `git = "read"` (or higher).
---@return string[]|nil paths
---@return string|nil   err
function Repo.untracked() end

---@class arbor.StagedFile
---@field path   string   Repo-relative path. Renames report the NEW path.
---@field status "added"|"modified"|"deleted"|"renamed"|"typechange"

---Return every file whose **index** differs from HEAD — exactly what
---`git diff --cached --name-only` would list. Each entry carries the
---path and the kind of change ("added", "modified", "deleted",
---"renamed", "typechange") so the caller can filter (e.g. skip
---deletions when inspecting file contents).
---
---The canonical caller is an `corvus:pre_commit` hook: this is the precise
---set about to enter the next commit. Working-tree-only changes
---(`git add` not yet run) are NOT included.
---
---Requires `git = "read"` (or higher). Empty array when the index is
---clean. `(nil, err)` on libgit2 failure.
---@return arbor.StagedFile[]|nil files
---@return string|nil             err
function Repo.staged_files() end

---Drop every libgit2 `Repository` handle Arbor currently holds. libgit2
---memory-maps packfiles and some index files; on Windows those handles
---block other processes (`git clone`, `rm -rf`, `mv …`, Explorer) from
---deleting or renaming the underlying files, surfacing as "file in use".
---
---Call this **before** mutating the filesystem of the active repo with an
---external tool (CLI git, shell, or a child process spawned through
---`arbor.job.spawn`). The handles are re-opened transparently the next time
---Arbor itself needs them, so the call is effectively free.
---
---No permission required — it only drops in-memory state.
function Repo.release_handles() end

---@class arbor.RepoCloneOptions
---@field url                string               Remote URL to clone (required)
---@field dest               string               Destination directory; parent must exist (required)
---@field branch             string|nil           Branch to clone (maps to --branch)
---@field shallow            boolean|nil          When true, perform a --depth 1 shallow clone
---@field recurse_submodules boolean|nil          When true, pass --recurse-submodules
---@field name               string|nil           Display name in the Jobs overlay (default "Clone: <url>")
---@field category           string|nil           Grouping label in the Jobs overlay (default "Clone")
---@field on_done            fun(ctx: arbor.RepoCloneResult)|nil  Lua callback fired when the job ends

---@class arbor.RepoCloneResult
---@field job_id    string
---@field success   boolean
---@field exit_code integer
---@field cancelled boolean
---@field dest      string
---@field url       string

---Clone a remote repository in the background. Progress streams via the Jobs
---overlay and the Job Output panel (arbor://job-output events). Uses the system
---`git` binary so SSH keys and credential helpers (including the Arbor keyring)
---work transparently.
---
---Returns the job_id string, usable with `arbor.job.list()` / `arbor.job.cancel(id)`.
---Requires `git = "write"` (or higher).
---@param  opts arbor.RepoCloneOptions
---@return string job_id
function Repo.clone(opts) end


-- =============================================================================
-- arbor.meta
-- =============================================================================

---@class arbor.Meta
local Meta = {}

---Return this plugin's name as declared in plugin.toml.
---@return string
function Meta.plugin_name() end

---Return the numeric Arbor API version this plugin was loaded with.
---@return integer
function Meta.api_version() end

---Return the running Arbor application version string (e.g. "0.3.0").
---@return string
function Meta.app_version() end

---Return the absolute path to this plugin's directory.
---@return string
function Meta.plugin_dir() end

---Synchronously check whether another plugin (by manifest name) is currently
---loaded AND enabled. Useful for sibling plugins that need to branch on
---another plugin's presence WITHOUT going through the async, fire-and-forget
---`arbor.service.call` mechanism. Returns false on unknown names, dormant
---entries, or any lookup failure.
---
---Safe to call from any hook, including `arbor:plugin_load` — which is where
---a plugin usually wants it, to decide whether an optional companion is there.
---@param name string  manifest name of the plugin to check
---@return boolean
function Meta.plugin_loaded(name) end

---Return "windows" | "macos" | "linux".
---@return string
function Meta.os() end


-- =============================================================================
-- arbor.credentials — a plugin's own secrets, and only its own
-- =============================================================================
--
-- A plugin reaches EXACTLY the slots its plugin.toml declared:
--
--   [[credentials]]
--   key   = "oauth"
--   label = "Google account"
--
-- Arbor's own credentials — git provider tokens, refresh tokens, issue tracker
-- keys — live in the same store and cannot be named from here. Not filtered:
-- every name this API can build is `plugin/<your-name>/<key>`, so there is no
-- way to spell one that is outside it.
--
-- Values are stored in the OS keychain, never in settings and never on disk in
-- the plugin's own directory.

---@class arbor.CredentialSlot
---@field key    string   The slot key, as declared in plugin.toml.
---@field filled boolean  Whether a value is currently stored in it.

---@class arbor.Credentials
local CredentialsApi = {}

---Read one of your own credentials. Returns nil when the slot is empty.
---Raises if `key` was not declared in your plugin.toml.
---@param  key string
---@return string|nil
function CredentialsApi.get(key) end

---Create or replace one of your own credentials.
---Raises if `key` was not declared, or if `value` is empty — use `delete` to
---clear a slot, so a stored-but-blank secret never has to be special-cased.
---@param key   string
---@param value string
function CredentialsApi.set(key, value) end

---Remove one of your own credentials. Clearing an already-empty slot succeeds.
---@param key string
function CredentialsApi.delete(key) end

---List your declared slots and whether each currently holds a value.
---Returns which slots are FILLED, never the values — a settings panel asks
---"is this connected?", and that question does not need the secret to move.
---@return arbor.CredentialSlot[]
function CredentialsApi.list() end


-- =============================================================================
-- The `embed` form node — a page your package ships
-- =============================================================================
--
-- Arbor gives the folder a URL, isolates the frame and relays messages. It never
-- reads what crosses: whatever runs inside is yours.
--
-- The files are served on Arbor's own `plugin:` scheme, with real content types —
-- including `application/wasm`, so a WebAssembly module streams instead of being
-- buffered whole and compiled in one blocking go. Only paths inside a plugin
-- root are served.
--
--   {
--     type       = "embed",
--     id         = "viewport",
--     src        = arbor.fs.join(arbor.meta.plugin_dir(), "web", "index.html"),
--     height     = 380,                         -- px, or "fill" (see below)
--     min_height = 260,                         -- floor when filling; default 320
--     send        = outbox,                     -- appending is what sends
--     on_message  = "myplugin:message",         -- scoped slot, fired with what it posts
--     same_origin = false,                      -- see below; default false
--   }
--
-- `height = "fill"` takes whatever vertical space the surface has left, down to
-- `min_height`. That is what a viewport in a PANEL wants — the user drags the
-- split to make the picture bigger and a fixed number ignores them. A modal
-- still wants a number: a modal is sized by its content, not the reverse.
--
-- `send` is an OUTBOX, not a value: the node remembers how many entries it has
-- already delivered, so a patch that changes one slider does not replay the
-- first message. Anything appended before the page is listening is queued.
--
-- Appending to your own copy is NOT what delivers — the node has to see the new
-- array. Rebuilding the panel (`set_panel_content`) does that, but it remounts
-- the frame and throws away whatever was running inside. To reach a frame that
-- is already up, patch the node in place:
--
--   arbor.ui.form.patch{ { id = "viewport", set = { "send" }, value = outbox } }
--
-- Give the node a stable `id` for that. Handing the node a SHORTER list than it
-- has already delivered means "this is the new full replay set" and it starts
-- again from the beginning — which is how you stop an outbox growing without
-- bound: when a message supersedes everything before it, make it the only entry.
--
-- Better still, stamp each message with a `seq` that only goes up. Delivery is
-- then "everything above the highest seq already delivered", so the array can be
-- REWRITTEN rather than appended to, and the outbox can be kept as the answer to
-- "what would a frame mounting right now need" — the scene, plus the latest of
-- each thing after it. A surface driven at pointer rate otherwise appends
-- thousands of entries that exist only to be skipped, and re-serialises the
-- whole list on every one:
--
--   seq = seq + 1
--   message.seq = seq
--   if message.type == "open" then replay = { open = message }
--   else replay[message.type] = message end
--   -- flatten `replay` sorted by seq → outbox, then patch
--
-- The frame runs `allow-scripts` WITHOUT `allow-same-origin`, so it has an
-- opaque origin: no storage, no cookies, no reach into the app around it.
-- postMessage is the only way through — which is also why the page inside has to
-- be written to talk that way.
--
-- Set `same_origin = true` if your page FETCHES its own files — a wasm module, a
-- texture, a data file. WebKit refuses custom-scheme sub-resource loads from an
-- opaque origin, and the symptom is a 403 plus "Not allowed to download due to
-- sandboxing", which reads like a permissions bug rather than the fetch failure
-- it is. The frame then shares the `asset:` origin with other plugin files —
-- never with Arbor itself, which is on a different scheme. A page that only
-- draws and posts messages should leave it off.

-- =============================================================================
-- arbor.shader — what a WGSL material declares  (Bennu only)
-- =============================================================================
--
--   local u = arbor.shader.uniform{ source = text }   -- or { path = "…" }
--
-- `nil` when the shader binds nothing in the material's group. Otherwise:
--
--   {
--     group     = "#{MATERIAL_BIND_GROUP}",  -- verbatim; there is no number
--     struct    = "SpiralHoverParams",       -- absent if there is no parameter block
--     variable  = "params",
--     binding   = 0,
--     size      = 64,                        -- bytes, rounded up as a uniform buffer is
--     fields    = { { name, type, offset, size, columns, rows, column_stride, hints? }, … },
--     resources = { { binding, name, type, kind }, … },  -- textures, samplers, storage
--   }
--
-- Offsets honour WGSL's alignment, which is the part worth not writing yourself: a `vec3`
-- aligns to 16 and not 12, and a `mat3x3<f32>` is three columns each padded to 16 — 48
-- bytes, not 36. Getting it wrong is quiet: every value lands in the next field along and
-- the shader draws something plausible that is not what you asked for.
--
-- `resources` is there because a material is two things. A parameter block is values you
-- write; a texture is something you bind. Both live in the same group, and a panel built
-- from the parameters alone omits inputs the pipeline still refuses to run without.
--
-- Published by Bennu, not by the host: it is the product that already reads WGSL, for
-- highlighting and for checking a material's Rust half against its shader half. Under any
-- other product `arbor.shader` is nil.
--
--   local p = arbor.shader.preview{ source = text }   -- or { path = "…" }
--
-- The same shader RENUMBERED onto the fixed bind-group layout a previewer has, plus the map
-- back. `nil` when there is no material group.
--
--   {
--     source     = "…",      -- the rewritten copy; identical when nothing had to move
--     rewritten  = true,     -- whether anything did
--     owns_group = false,    -- it extends StandardMaterial rather than owning the group
--     group      = "#{MATERIAL_BIND_GROUP}",
--     layout     = { binding = 100, uniforms = 8, textures = 12, samplers = 3 },
--     uniforms   = { { name, type, slot, from, to, hint? }, … },
--     textures   = { { name, type, kind, index, from, to, key, image, aliased, hint? }, … },
--     samplers   = { { name, type, slot, from, to }, … },
--     rejected   = { { name, type, binding, reason }, … },
--   }
--
-- Why it exists: `AsBindGroup::bind_group_layout_entries` is a STATIC method, so one material
-- type has exactly one layout and a viewer cannot build one to match whatever indices a
-- shader happens to use. Widening a layout answers "the binding is missing" and "the binding
-- is too small"; it cannot answer "the binding is the wrong kind", because binding 101 is a
-- buffer in one material and a sampler in the next. Moving the SHADER is the answer that
-- works for all three.
--
-- Nothing is written back — the rewrite is a copy, and a preview replaces its shader asset on
-- every keystroke anyway. Names, offsets and `// @preview` lines are untouched: only the
-- numbers inside `@binding(…)` move.
--
-- `key` is WHAT a texture is — `diffuse`, `normal`, `pbr`, `ao`, `height` — guessed from the
-- variable's name and overridable with `// @preview <key>` above the declaration. Textures
-- with the same key SHARE a slot, deliberately: a preview has no assets, so `top_normal` and
-- `side_normal` would be handed byte-identical generated pictures and giving each its own
-- spends a scarce slot on nothing. An author who wants them apart writes two keys
-- (`normal.top`, `normal.side`).
--
-- `image` is the picture that key opens on (`white` `black` `grey` `normal` `checker` `noise`
-- `uv`); a panel may offer any of them instead. `index` is the position in the runtime's FLAT
-- slot list — the 2D slots, then the array textures, then the cubes — which is the list a
-- viewer fills by position; several textures sharing a key share an index. `aliased` means
-- something else: the previewer ran OUT of slots for a new kind, so this one reads a picture
-- meant for something different. That is worth showing; sharing by key is not.
--
-- `rejected` is what no renumbering reaches: a storage buffer, a storage texture, a
-- comparison sampler, a depth texture, or anything past the slot counts. Each carries the
-- sentence to say why, because a layout mismatch inside a viewport is a validation abort and
-- a dead canvas rather than a message.

-- =============================================================================
-- arbor.ext — calling an installed extension
-- =============================================================================
--
-- An extension is a compiled WebAssembly component that IMPLEMENTS an interface
-- rather than consuming the Lua API — a shader translator, a mesh generator, a
-- format backend. It answers; it does not decide. Which one to call, with what,
-- and what to do with the result is YOUR plugin's, because your plugin is the
-- one with a panel and a user in front of it.
--
-- Arbor knows nothing about what any of them do. It checks that you are allowed
-- to call one, resolves the address, and passes JSON through in both directions.
-- That is why adding a kind of extension is installing a package rather than a
-- new version of Arbor.
--
--   local kinds = arbor.ext.call{
--     interface = "mesh-source", id = "fulcrum", method = "catalogue",
--   }
--
--   local mesh = arbor.ext.call{
--     interface = "mesh-source", id = "fulcrum", method = "build",
--     args = { "geode", '{"facets":9}' },
--   }
--
-- REQUIRES `service_call = true` in [permissions]. Calling an extension is
-- invoking another package's code, and an installed extension carries its own
-- credentials and its own network allowlist — a plugin that could call one
-- unasked could use them.
--
-- `args` is POSITIONAL. A component's type information carries parameter types
-- but not their names, so there is nothing to key a table on. The shapes inside
-- are still named: a record argument is an ordinary table keyed by its fields.
--
-- Field names cross in BOTH spellings, in BOTH directions. A WIT identifier is
-- kebab-case, so a record field is `params-schema`; a table you pass IN may use
-- either that or `params_schema`, and a record you get BACK carries both keys.
-- Read whichever reads better in Lua — `entry.params_schema` needs no brackets.
--
-- Until recently only the hyphen came back, and that asymmetry cost a mesh
-- package every one of its parameter controls with nothing anywhere reporting
-- it: in Lua a missing key is not a failure, it is `nil`, and a schema that
-- never arrived is indistinguishable from a shape that declares no knobs.

---@class arbor.ExtFunc
---@field name    string  The function name, as the interface declares it.
---@field params  integer How many positional arguments it takes.
---@field results integer 0 or 1 — a WIT function returns at most one value.

---@class arbor.ExtInterface
---@field name  string             Full export name, e.g. "arbor:extensions/mesh-source@1.0.0".
---@field funcs arbor.ExtFunc[]

---@class arbor.ExtEntry
---@field interface string             What it provides, from the package's [[provides]].
---@field version   integer
---@field id        string
---@field plugin    string             The package that provides it.
---@field exports   arbor.ExtInterface[]  Read from the module itself, not from the manifest —
---                                       a package that claimed an interface it does not
---                                       export shows up here with nothing in it.

---@class arbor.ExtCallSpec
---@field interface string    Which contract, e.g. "mesh-source".
---@field version   integer?  Defaults to 1.
---@field id        string    Which member of it, e.g. "fulcrum".
---@field method    string    The function to call.
---@field args      any[]?    Positional arguments. Omit for none.
---@field export    string?   Which exported interface to look in, when the module
---                           exports more than one and the names are ambiguous.

---@class arbor.Ext
local ExtApi = {}

---Every installed extension and what it actually exports.
---@return arbor.ExtEntry[]
function ExtApi.list() end

---Call one function on one extension. Returns whatever it returns, as Lua.
---Raises when the extension is not installed, the function does not exist, an
---argument does not fit the shape it declares, or the extension itself fails —
---in which case the error is the one IT reported.
---@param  spec arbor.ExtCallSpec
---@return any
function ExtApi.call(spec) end


-- =============================================================================
-- arbor.settings (global + project scopes)
-- =============================================================================

---@class arbor.SettingsScope
local SettingsScope = {}

---Get a stored value by key. Returns nil if the key does not exist.
---@param  key string
---@return any|nil
function SettingsScope.get(key) end

---Store a value under key. Pass nil to delete the key.
---Tables are JSON-serialized. Strings, numbers, and booleans are stored directly.
---@param key   string
---@param value any
function SettingsScope.set(key, value) end

---Return all stored settings as a Lua table.
---@return table<string, any>
function SettingsScope.get_all() end

---Delete a key from settings.
---@param key string
function SettingsScope.clear(key) end

---@class arbor.Settings
---@field global  arbor.SettingsScope  Persisted at: ~/.config/arbor/plugin_data/<name>/global.json
---@field project arbor.SettingsScope  Persisted at: <repo>/.arbor/plugins/<name>/project.json
local SettingsApi = {}

---Read a single key from another plugin's `global.json`. Reading own settings
---is always allowed; reading other plugins' requires the `settings_read_others`
---permission in plugin.toml. Cross-plugin WRITE is not exposed here — the
---target plugin must opt in by exporting a service via `arbor.service.export`,
---which the caller then invokes through `arbor.service.call`.
---@param  plugin_name string
---@param  key         string
---@return any|nil
function SettingsApi.read(plugin_name, key) end

---Same as `read` but for the project-scoped `project.json` file under the
---active repo. Returns nil when no repo is open.
---@param  plugin_name string
---@param  key         string
---@return any|nil
function SettingsApi.read_project(plugin_name, key) end


-- =============================================================================
-- arbor.terminal
-- =============================================================================

---@class arbor.Terminal
local Terminal = {}

---Execute a shell command synchronously and return its output.
---Requires `terminal = "commands"` or `terminal = "any"` in plugin.toml.
---In "commands" mode only basenames listed in `terminal_scope` are allowed.
---@param  command string  Full shell command string
---@param  cwd     string|nil  Working directory (nil = inherit from process)
---@return arbor.ExecResult
function Terminal.exec(command, cwd) end


-- =============================================================================
-- arbor.job
-- =============================================================================

---@class arbor.JobSpawnConfig
---@field name           string                          Human-readable name shown in the Jobs UI
---@field command        string                          Shell command to run in the background
---@field cwd            string|nil                      Working directory
---@field env            table<string,string>|nil        Extra environment variables
---@field category       string|nil                      Group label in the Jobs overlay (excludes "system")
---@field on_done_action string|nil                      Plugin action name fired when the job finishes (sugar)
---@field on_done        fun(result:arbor.JobResult)|nil Lua callback on completion (sugar — also resolves the returned JobHandle)

---@class arbor.JobHandle : arbor.Promise
---@field id string
local JobHandle = {}

---Best-effort cancel — terminates the underlying process if it's still running.
function JobHandle:cancel() end

---@class arbor.Job
local Job = {}

---Spawn a background job. The job streams its output to the Jobs UI.
---Returns a `JobHandle`: a Promise that resolves with the on-done context
---(`{ job_id, success=true, exit_code }`) when the job exits cleanly, or
---rejects with the same shape on failure / cancellation. The handle also
---exposes `.id` and `:cancel()`.
---On spawn failure (lock error, missing app handle), returns `(nil, err)`.
---`config.on_done` (Lua function) and `on_done_action` (action name) still
---fire alongside the promise as zucchero — useful when the same logic must
---also run from outside the consumer that started the job.
---@param  config arbor.JobSpawnConfig
---@return arbor.JobHandle|nil handle
---@return string|nil          err
function Job.spawn(config) end

---Return a snapshot of all known jobs (running and recently finished).
---@return arbor.JobInfo[]
function Job.list() end


-- =============================================================================
-- arbor.timer
-- =============================================================================

---@class arbor.Timer
local Timer = {}

---Call fn once after delay_ms milliseconds. Returns a cancellable timer ID.
---@param  delay_ms integer
---@param  fn       fun()
---@return string   timer_id
function Timer.after(delay_ms, fn) end

---Call fn repeatedly every interval_ms milliseconds. Returns a cancellable timer ID.
---@param  interval_ms integer
---@param  fn          fun()
---@return string      timer_id
function Timer.every(interval_ms, fn) end

---Cancel a timer created by after() or every(). Safe to call with an invalid ID.
---@param id string
function Timer.cancel(id) end


-- =============================================================================
-- arbor.scheduler — Spring-style background schedules
--
-- Manifest opt-in (plugin.toml):
--   [scheduler]
--   enabled = true
-- =============================================================================

---@alias arbor.SchedulerDuration string|integer  e.g. "30s", "5m", "2h", "PT1H30M", or seconds

---@class arbor.SchedulerConfig
---@field action            string                  Required. Plugin action fired on each tick.
---@field fixed_rate        arbor.SchedulerDuration|nil  Fire every N. Next fire = previous start + N.
---@field fixed_delay       arbor.SchedulerDuration|nil  Fire N AFTER previous handler returns.
---@field cron              string|nil              6-field Spring cron: "sec min hr dom mon dow".
---@field initial_delay     arbor.SchedulerDuration|nil  Wait before first fire (fixed_rate / fixed_delay only).
---@field on_load           boolean|nil             Also fire once at plugin load. Default false.
---@field only_when_focused boolean|nil             Skip ticks while the window is unfocused. Default false.

---@class arbor.SchedulerTriggerFixedRate
---@field kind         "fixed_rate"
---@field interval_sec integer

---@class arbor.SchedulerTriggerFixedDelay
---@field kind      "fixed_delay"
---@field delay_sec integer

---@class arbor.SchedulerTriggerCron
---@field kind "cron"
---@field expr string

---@alias arbor.SchedulerTrigger arbor.SchedulerTriggerFixedRate|arbor.SchedulerTriggerFixedDelay|arbor.SchedulerTriggerCron

---@class arbor.SchedulerEntry
---@field action            string
---@field trigger           arbor.SchedulerTrigger
---@field initial_delay_sec integer
---@field on_load           boolean
---@field only_when_focused boolean

---@class arbor.Scheduler
local Scheduler = {}

---Register a background schedule. Exactly one of `fixed_rate`, `fixed_delay`,
---or `cron` must be provided. Re-calling with the same `action` replaces the
---previous registration.
---@param config arbor.SchedulerConfig
function Scheduler.register(config) end

---Snapshot of the schedules registered so far by this plugin.
---@return arbor.SchedulerEntry[]
function Scheduler.list() end


-- =============================================================================
-- arbor.ui
-- =============================================================================

---@class arbor.UiConfirmConfig
---@field message         string                            Dialog body (required)
---@field confirm_label   string|nil                        Button label (default: "Confirm")
---@field confirm_variant "primary"|"danger"|"ghost"|nil    (default: "primary")
---@field state           any|nil                           Arbitrary data echoed back unchanged (debug aid)

---@class arbor.UiContextMenuItemConfig
---@field target string|nil  Context where the item appears: "commit" (default) or other targets
---@field label  string      Menu item text
---@field action string      Plugin action name fired on click
---@field icon   string|nil  Lucide icon name (e.g. "GitBranch")

---@class arbor.UiMenuItemConfig
---@field label  string
---@field action string
---@field icon   string|nil

---Config for `arbor.ui.add_toolbar_action` — registers an inline action button
---on one of Arbor's toolbars (or any custom plugin toolbar via a passthrough
---target). All toolbars share the same payload shape; the renderer uses the
---fields it cares about (e.g. `color` is only meaningful for status / title
---bar pills; `label` is optional for icon-only diff toolbar buttons).
---@class arbor.UiToolbarActionConfig
---@field id      string                                                                                                Unique id within (plugin, target)
---@field target  "diff"|"status-bar:left"|"status-bar:right"|"title-bar:left"|"title-bar:right"|"commit-detail"|"commit-form"|string  Which toolbar
---@field action  string                                                                                                Plugin action fired on click
---@field label   string|nil                                                                                            Optional — omit for icon-only buttons
---@field icon    string|nil                                                                                            Lucide name or emoji
---@field tooltip string|nil                                                                                            Hover tooltip (fallback to `label`)
---@field color   "info"|"success"|"warning"|"error"|"muted"|"accent"|nil                                               Tint hint (status / title bar only)

---Config for `arbor.ui.add_sidebar` — registers a plugin panel with its own
---ActivityBar icon. Target either side of the app and either position within
---the ActivityBar.
---@class arbor.UiSidebarConfig
---@field id       string                      Unique id within the plugin
---@field label    string|nil                   Display label (fallback to `id`)
---@field icon     string|nil                   Lucide icon name or single-char emoji
---@field tooltip  string|nil                   Hover tooltip (fallback to `label`)
---@field side     "left"|"right"|nil           Which ActivityBar hosts the icon (default "right")
---@field position "top"|"bottom"|nil           Sidebar panel (top) or unique bottom slot (default "top")
---@field action   string|nil                   Optional override for the fired action name
---                                             (defaults to `panel:open:<id>`)
---@field collapsable boolean|nil               Reserved — sidebar panels don't collapse today

---Body pushed by `arbor.ui.set_panel_content`. Rendered by the lightweight
---sidebar form-DSL renderer (for `add_sidebar` panels) or the full
---FormNodeRenderer (for `add_view` main-area views).
---@class arbor.UiPanelContent
---@field title   string|nil    Header shown above the body (optional)
---@field nodes   table[]|nil   Form-DSL node tree (list of node tables)
---@field actions table[]|nil   Optional footer buttons: `{label, action, variant?}`

---Config for `arbor.ui.add_view` — registers a main-area view. Unlike a
---sidebar (a side rail), a view occupies the body of the window where the
---commit graph lives, and renders its content through the FULL FormNodeRenderer
---(every node type + the dispatch / scoped-event / patch protocol).
---@class arbor.UiViewConfig
---@field id        string                   Unique id within the plugin
---@field label     string|nil               Display label (fallback to `id`)
---@field icon      string|nil               Lucide icon name / emoji / plugin icon ref
---@field placement "graph"|"main"|nil       Body footprint (default "graph")
---@field tooltip   string|nil               Hover tooltip (fallback to `label`)

---@class arbor.UiGraphComboConfig
---@field id         string                  Unique combo ID (scoped per plugin)
---@field run_action string                  Action name fired when the run button is clicked
---@field run_icon   string|nil              Lucide icon name for the run button
---@field tooltip    string|nil              Tooltip shown on the run button
---@field target     "activity_bar"|nil      Placement target (default: "activity_bar")
---@field options    arbor.ComboOption[]|nil Initial dropdown options

---@class arbor.Ui
local Ui = {}

---Open a plugin form dialog. The form emits plugin:form.
---Refer to plugin form documentation for the full config schema.
---
---`arbor.ui.form` is also a table exposing mutation helpers that target the
---currently-open form of this plugin. Each helper is a no-op when no form is
---open or when the open form belongs to another plugin.
---
---Available helpers:
---  arbor.ui.form.set_options(name, options)            -- legacy positional (field name)
---  arbor.ui.form.set_options({ id|name = "…", options = … })  -- explicit cfg form
---  arbor.ui.form.set_disabled(name, bool)              -- legacy positional
---  arbor.ui.form.set_disabled({ id|name = "…", disabled = … })
---  arbor.ui.form.set_value(name, value)                -- legacy positional
---  arbor.ui.form.set_value({ id|name = "…", value = … })      -- explicit cfg form
---  arbor.ui.form.replace(cfg)                -- swap the whole node tree in-place
---  arbor.ui.form.patch(ops)                  -- granular node-tree mutations (no re-mount)
---  arbor.ui.form.set_state_path(segs, value) -- mutate one slice of the opaque state
---  arbor.ui.form.set_loading(arg)            -- toggle the busy overlay (cheap, no re-render)
---  arbor.ui.form.close()                     -- programmatically dismiss the modal
---
---`set_value` / `set_options` / `set_disabled` accept BOTH a legacy
---`(name, payload)` positional call (field NAME — not node id) and a single
---cfg-table call `{ name | id, <payload_key> = ... }`. The cfg form is the
---recommended pattern when the caller already tracks the node id (same key
---used by `arbor.ui.form.patch`): the host resolves id → field name by
---walking the node tree, so plugins don't need a parallel "field names"
---table. Passing a name / id that doesn't match any current field logs a
---warning in the host devtools console (the write still goes through) so
---typos surface immediately.
---
---Top-level config fields include `title`, `description`, `submit_action`,
---`submit_label`, `cancel_action`, `cancel_label`, `width`, `height`,
---`hide_submit`, `hide_cancel`, `sidebar`, `state`, `css`, plus
---`loading = true|false` and `loading_label = "..."` to surface a busy
---overlay above the form body — useful while fanning out to the network
---after open. Toggle the overlay live via `arbor.ui.form.set_loading(...)`
---or by passing `loading` / `loading_label` alongside `nodes` to
---`arbor.ui.form.replace`.
---
---For long-running async work outside of forms, use `arbor.ui.operation.*`
---(see below) — same overlay used by Pull / Fetch-all / Pull-all so the
---progress card looks identical to built-in Arbor flows.
---
---Pair `arbor.ui.form.close()` with `keep_open = true` on the form config
---when submit launches a follow-up flow (file picker, confirm, second form):
---the modal stays mounted while the secondary flow is up, and you call close()
---once the flow completes (or on a hard error path).
---
---Live `actions.change` on `select` / `checkbox` / `toggle` / `radio`: each
---of those form-nodes accepts an `actions = { change = "..." }` field. When
---set, the action fires on every selection / flip (not just Submit) with
---`{ value, node_id }` in the payload — `node_id` so one handler behind
---several fields can tell which one moved — handy for "window picker" / live-filter
---controls that should re-fetch immediately, or boolean toggles / segmented
---switches that should re-render dependent content. (For pure show/hide of
---dependent nodes you usually do NOT need actions.change — gate them with
---`show_if` instead and the swap happens client-side without a round-trip.)
---
---Radio appearance: `appearance = "segment"` renders a pill-style toggle bar
---(IntelliJ studio-style View switcher), `appearance = "card"` renders
---title+description cards, `appearance = "radio"` (the default) renders
---classic radio dots. Pair `inline = true` + `appearance = "segment"` for
---compact mode switches.
---
---Scoped dispatch (high-frequency slots): a value slot's change can target a
---DISPATCH instead of a bare action string — `change = { kind = "action",
---name = "..." }` or `{ kind = "command", id = "..." }`. A dispatch slot ships
---a SCOPED payload `{ node_id, slot, value, state? }` (not the whole form) and
---is tracked per node, so concurrent edits on different nodes never block each
---other and a fast-firing widget isn't gated by a global lock (latest-wins).
---Add `scope_state = { "k1", "k2" }` on the node to ride a slice of the opaque
---form state along in `state`. Honoured today by the leaf `field` node (via a
---node-level `dispatch = …`), `vec_field`, `select` / `checkbox` / `toggle` / `radio`
---`actions.change`, and the `editor` widget (`on_edit` slot `edit` /
---`on_select` slot `select`); bare-string actions keep the legacy whole-form
---payload unchanged.
---
---Builder mode: `arbor.ui.form()` (no arg) or `arbor.ui.form("id")` returns a
---chainable `arbor.FormBuilder`; `:open()` emits the modal via the same path.
---@overload fun(): arbor.FormBuilder
---@overload fun(id: string): arbor.FormBuilder
---@param config table
function Ui.form(config) end

---Replace the option list of a select / radio / autocomplete field in the
---currently-open form of this plugin.
---
---Accepts either positional `(name, options)` or a cfg table
---`{ name | id, options }`. Pass `id` when the caller already tracks the
---node id (same key used by `patch`) — the host resolves id → field name.
---@overload fun(cfg: { id?: string, name?: string, options: arbor.FormOptionInput[] })
---@param name    string  Field name (matches the node's `name` attribute)
---@param options arbor.FormOptionInput[]
function Ui.form.set_options(name, options) end

---Toggle the disabled state of a field in the currently-open form.
---
---Accepts either positional `(name, disabled)` or a cfg table
---`{ name | id, disabled }`. See `set_options` for the id-vs-name guidance.
---@overload fun(cfg: { id?: string, name?: string, disabled: boolean })
---@param name     string
---@param disabled boolean
function Ui.form.set_disabled(name, disabled) end

---Programmatically set the value of a field in the currently-open form.
---
---Accepts either positional `(name, value)` or a cfg table
---`{ name | id, value }`. The cfg form with `id` is the recommended pattern
---when the caller already tracks the node id (same key used by `patch`):
---the host resolves id → field name by walking the node tree.
---@overload fun(cfg: { id?: string, name?: string, value: any })
---@param name  string
---@param value any
function Ui.form.set_value(name, value) end

---Swap the root `nodes` tree of the currently-open form in-place, without
---unmounting the modal. Field values whose `name` still exists are preserved;
---new fields get their declared defaults; gone fields are discarded. Collapse /
---tabs / wizard state is kept by node id; tree expansion (keyed by `field::value`)
---is never cleared.
---
---Ideal for IntelliJ-style tree layouts where add / remove / duplicate must
---update the structure without closing and reopening the modal.
---
---Payload shape:
---  {
---    nodes         = { ... new top-level nodes ... },
---    state         = { ... optional — replaces the echoed opaque state ... },
---    set_values    = { field_name = value, ... },  -- optional overrides applied AFTER rebuild
---    loading       = true|false,                   -- optional — toggle busy overlay
---    loading_label = "Fetching 3/12: foo…",        -- optional — overlay label
---  }
---@param cfg table
function Ui.form.replace(cfg) end

---A single granular patch op for `arbor.ui.form.patch`. Targets a node by its
---stable `id` and applies exactly one verb — pick one of merge / set / append /
---remove. A node without a stable `id` can't be patched (use `replace`).
---@class arbor.FormPatchOp
---@field id      string            Stable id of the target node.
---@field merge?  table             Shallow-merge these props onto the node (label, options, disabled, variant…).
---@field set?    (string|number)[] Path of segments INSIDE the node for a deep assign, e.g. { "options", 1, "label" }.
---@field value?  any               Value for `set` (required when `set` is present).
---@field append? table             A child node to push into an array-valued prop.
---@field to?     string            Array prop that `append` targets (default "children"; e.g. "nodes" for a tree).
---@field remove? boolean           When true, splice the targeted node out of its parent. To remove a CHILD, target it by its own id.

---Granular, in-place mutations of the currently-open form's node tree —
---sibling to `replace`, but surgical: no re-mount, addressed by stable node id.
---Patches touch the node tree ONLY; field values go via `set_value`, opaque
---state via `set_state_path`. Ideal for high-frequency UIs (log streams, lazy
---trees) where a full `replace` per update would re-mount the subtree.
---
---  arbor.ui.form.patch({
---    { id = "status", merge  = { label = "Running…", variant = "warning" } },
---    { id = "log",    append = { type = "paragraph", text = line }, to = "children" },
---    { id = "opt3",   set    = { "label" }, value = "Renamed" },
---    { id = "row7",   remove = true },
---  })
---@param ops arbor.FormPatchOp[]
function Ui.form.patch(ops) end

---Mutate a single slice of the form's opaque liveState without replacing the
---whole blob (sibling to `replace { state = ... }`). `segments` is an array of
---string/number keys addressing the slot; a `nil` value DELETES the key (Lua
---has no JSON-null literal, so nil unambiguously means "drop it").
---
---  arbor.ui.form.set_state_path({ "filters", "branch" }, "main")  -- set
---  arbor.ui.form.set_state_path({ "filters", "branch" }, nil)     -- delete
---@param segments (string|number)[]
---@param value    any
function Ui.form.set_state_path(segments, value) end

---Toggle the busy overlay above the open form. Cheaper than `replace`
---because it does NOT re-render the node tree — use it for per-step
---progress ticks during a tight fan-out loop.
---
---Accepts:
---  arbor.ui.form.set_loading(true)                        -- show overlay, default label
---  arbor.ui.form.set_loading(false)                       -- hide overlay
---  arbor.ui.form.set_loading("Fetching 3/12…")            -- show + custom label
---  arbor.ui.form.set_loading{ loading = true, label = "…" }
---@param arg boolean|string|table|nil
function Ui.form.set_loading(arg) end

---Switch the active activity-bar sidecar in a Studio-shaped modal. Pass
---`nil` to close any open pane (only effective when `activity_bar.always_open`
---is not set). Passing an unknown id logs a host-side warning and is a no-op.
---
---  arbor.ui.form.set_sidecar("inspector")
---  arbor.ui.form.set_sidecar(nil)             -- close the rail
---@param id string|nil
function Ui.form.set_sidecar(id) end

---Substitute the body of a Studio-shaped modal with a fallback block
---(loading / error / empty). Mutually exclusive at render time — calling
---this with a non-nil `name` switches to that block; calling with `nil`
---clears the override and renders the body again.
---
---  arbor.ui.form.set_state_block("loading", { label = "Parsing…" })
---  arbor.ui.form.set_state_block("error",   { label = "Parse failed" })
---  arbor.ui.form.set_state_block("empty",   {
---    title = "No document", body = "Use Open file…",
---    cta_label = "Open file…", cta_action = "open_file",
---  })
---  arbor.ui.form.set_state_block(nil)         -- back to the body
---@param name string|nil  "loading" / "error" / "empty" / nil
---@param cfg  table|nil   Shape depends on `name` (see arbor.FormStateBlockCfg).
function Ui.form.set_state_block(name, cfg) end

---Open a confirmation dialog. Returns a Promise that resolves with `true`
---when the user clicks the confirm button and `false` on cancel.
---@param  config arbor.UiConfirmConfig
---@return arbor.Promise
function Ui.confirm(config) end

---Open the native-feeling FileExplorerModal and round-trip the chosen path back
---to the plugin via a fire-and-forget action. On cancel, the action is still
---fired but with `path = ""` so the plugin can distinguish it from a successful
---pick without wiring two handlers.
---
---Options:
---  mode         : "file" | "folder" | "save"  (default "file")
---  title        : dialog title
---  extensions   : string[]  (e.g. {"json","yaml"}) — honoured in file/save mode
---  initial_path : preselect a starting directory
---  action       : REQUIRED — plugin action name to invoke with the result
---  extra        : optional table, merged into the action's ctx alongside `path`
---
---Example:
---  arbor.ui.pick_file({
---    mode = "file", title = "Select JSON", extensions = { "json" },
---    action = "my-plugin:on_picked",
---    extra  = { target = "profile" },
---  })
---
---  arbor.events.on("my-plugin:on_picked", function(ctx)
---    if ctx.path == "" then return end   -- user cancelled
---    arbor.log.info("picked: " .. ctx.path .. " for " .. ctx.target)
---  end)
---@param opts table
function Ui.pick_file(opts) end

---Register a context menu item (e.g. on right-click of a commit).
---@param config arbor.UiContextMenuItemConfig
function Ui.add_context_menu_item(config) end

---Register a global application menu item.
---@param config arbor.UiMenuItemConfig
function Ui.add_menu_item(config) end

---Register an inline action button on one of Arbor's toolbars.
---
---`target` is one of the well-known short names:
---  * `"diff"`              → diff viewer header (next to Copy / Maximize)
---  * `"status-bar:left"`   → status bar, left segment (after built-in chips)
---  * `"status-bar:right"`  → status bar, right segment (before jobs / notifications / version)
---  * `"title-bar:left"`    → title bar, between the workspace dropdown and the spacer
---  * `"title-bar:right"`   → title bar, before docs / theme / settings
---  * `"commit-detail"`     → commit detail panel (action row below the body)
---  * `"commit-form"`       → commit form, between the Amend toggle and the Commit button
---
---Any other string is forwarded verbatim, so plugins can target their own
---custom toolbars (e.g. `"compile-action:tree:toolbar"`) through the same
---API. Internally this is sugar for `arbor.ui.contribute("<point>", { id, payload })`.
---
---@param config arbor.UiToolbarActionConfig
function Ui.add_toolbar_action(config) end

---Register a plugin panel attached to one of the ActivityBars.
---
---`side` chooses which ActivityBar hosts the icon:
---  * `"right"` (default) — plugin-expansion side
---  * `"left"`            — same bar as the built-in Arbor sections
---
---`position` chooses where the panel lives:
---  * `"top"` (default) — opens a side panel next to the ActivityBar
---  * `"bottom"`        — opens the unique bottom panel (shared across both
---                        sides — clicking overrides whichever panel was open)
---
---When the user clicks the icon Arbor fires `panel:open:<id>` on the plugin.
---The plugin responds with `arbor.ui.set_panel_content(id, {title, nodes})`.
---
---@param config arbor.UiSidebarConfig
function Ui.add_sidebar(config) end

---Register a main-area view — a body surface (where the commit graph lives)
---that renders form-DSL content through the FULL FormNodeRenderer, so it has
---parity with modals (every node type + dispatch / scoped events / patch).
---
---`placement` chooses how much of the body it occupies:
---  * `"graph"` (default) — replaces the commit graph, keeps tab bar + bottom panel
---  * `"main"`            — takes over the whole body column
---
---The view surfaces as an activity-bar icon (left rail), a Command Palette
---"Open View: <label>" entry, and the `Alt+Shift+V` toggle. Only one view
---occupies the body at a time; the selection persists across tab / workspace
---switches and requires a repo open.
---
---When the view opens Arbor fires `arbor:view_open` on the plugin (and
---`arbor:view_close` on teardown). Respond by pushing the body with
---`arbor.ui.set_panel_content(id, {title, nodes, actions?})` — the SAME channel
---sidebar panels use. View ids must be distinct from sidebar ids. Drive live,
---high-frequency updates with `arbor.ui.form.{patch,set_state_path,set_value}`.
---
---@param config arbor.UiViewConfig
function Ui.add_view(config) end

---Push form-DSL content into a panel registered via `add_sidebar` OR a view
---registered via `add_view`. Arbor re-renders in place and caches the content
---so subsequent opens display immediately while the plugin recomputes. Call it
---from the `panel:open:<id>` hook (sidebar) / `arbor:view_open` hook (view) — or
---any time the underlying state changes.
---
---Sidebar panels use a lightweight renderer (`heading`, `label`, `paragraph`,
---`divider`, `button`, `list`, `section`); views use the full FormNodeRenderer
---(every node type). For a view, `set_panel_content` is a full rebuild — use
---`arbor.ui.form.patch` / `set_state_path` for surgical, high-frequency updates.
---
---@param id   string                           Panel/view id (matches `add_sidebar` / `add_view`)
---@param body arbor.UiPanelContent             Panel body
function Ui.set_panel_content(id, body) end

-- ─── arbor.ui.operation — push to the global progress overlay ────────────
-- Same overlay used by single-repo Pull, workspace Fetch-all / Pull-all,
-- and linked-worktree sync. Plugin operations get a step-by-step card
-- with the same chrome — no separate widget.
--
-- Status values for `update_step`:
--   "pending"   — dot, waiting (default position-derived)
--   "active"    — spinner (avoid setting explicitly; use set_current instead)
--   "completed" — check, done
--   "skipped"   — dashed circle, intentionally no-op
--   "error"     — red x, error detail shown inline
---@class arbor.OperationStepInput
---@field key     string   Stable key referenced by update_step / set_current
---@field label   string   Short row label
---@field detail? string   Initial inline detail
---@field status? string   Initial status (defaults to position-derived)

---@class arbor.OperationStartConfig
---@field id        string                       Plugin-scoped id (we'll prepend the plugin name)
---@field title     string                       Card title
---@field subtitle? string                       Card subtitle (defaults to plugin name)
---@field steps     arbor.OperationStepInput[]   At least one — the row strip
---@field current?  string                       Step key to mark as active at start

---@class arbor.OperationStepPatch
---@field status? string   "pending"|"completed"|"skipped"|"error"
---@field detail? string   Inline row detail

---@class arbor.OperationFinishOpts
---@field summary? string   Single-line summary shown under the stepper when done
---@field error?   string   Top-level error message (turns the card red)

---@class arbor.UiOperation
local UiOperation = {}

---Open a progress card in the operations overlay.
---@param cfg arbor.OperationStartConfig
function UiOperation.start(cfg) end

---Move the active-step pointer to `step_key` and optionally update the
---inline detail. The stepper auto-completes earlier steps and leaves
---later ones pending; do NOT set `status = "active"` explicitly via
---update_step (sticky → step would spin forever after finish).
---@param id        string
---@param step_key  string
---@param detail?   string
function UiOperation.set_current(id, step_key, detail) end

---Patch a single step (status / detail) without moving the pointer.
---@param id        string
---@param step_key  string
---@param patch     arbor.OperationStepPatch
function UiOperation.update_step(id, step_key, patch) end

---Mark the operation as complete. The card lingers a few seconds with
---the summary / error visible, then auto-dismisses (longer delay on
---errors so the user has time to read).
---@param id    string
---@param opts? arbor.OperationFinishOpts
function UiOperation.finish(id, opts) end

---Register a split combo button (run button + dropdown) in the activity bar.
---@param config arbor.UiGraphComboConfig
function Ui.add_graph_combo(config) end

---Dynamically update the dropdown options of an existing combo button.
---Thin sugar over `arbor.ui.contribute_patch("arbor:activitybar", id,
---{ options = ... })`. When `selected_value` is provided AND it appears in
---the new options, also adopts it as the current pick (mirrors plugin-side
---selection state into the UI on `arbor:repo_open`).
---@param id              string
---@param options         arbor.ComboOption[]
---@param selected_value  string|nil
function Ui.set_combo_options(id, options, selected_value) end

---Insert a visual horizontal separator in the activity bar after the last registered item.
function Ui.add_separator() end

---Push a fresh list of suggestions to an open autocomplete form field.
---The field identifies itself by `id` (declared in the form node). Options
---may be bare strings (auto-expanded to { value = s, label = Capitalised s })
---or full { value, label, group? } tables.
---
---Typical flow: form field declares `source_action = "my_plugin:search"`;
---the plugin subscribes to that action and, given the user's query, calls
---this function with the matching suggestions.
---
---@param id      string
---@param options arbor.FormOptionInput[]
function Ui.set_autocomplete_options(id, options) end


-- =============================================================================
-- arbor.ui.set_branding / clear_branding / set_theme_tokens / clear_theme_tokens
--
-- RAM-only branding overlay: replace the app mark and overlay extra CSS
-- variables on top of the active theme. Nothing is persisted — reloading
-- Arbor restores the bundled identity unless the same plugin re-applies
-- the overrides during its `arbor:plugin_load` handler.
-- =============================================================================

---@class arbor.UiBrandingConfig
---@field svg              string|nil  Inline SVG markup for the in-app mark.
---                                     Mutually exclusive with `svg_path`.
---                                     Must start with `<svg`.
---@field svg_path         string|nil  Absolute path to an SVG file the host
---                                     reads off disk (no `fs.read` perm
---                                     required; same trust model as
---                                     `window_icon_path`). Mutually
---                                     exclusive with `svg`.
---@field window_icon_path string|nil  Absolute path to a *raster* image
---                                     (PNG / ICO) handed to the OS
---                                     window-icon API — taskbar, Alt-Tab,
---                                     window chrome on Windows / Linux.
---                                     SVG is rejected here because the
---                                     platforms need a rasterised buffer.
---                                     macOS dock icons come from
---                                     Info.plist and require a build-time
---                                     swap, so this is a no-op there.

---Replace the default Arbor app mark for this session.
---
---At least one of `svg` / `svg_path` / `window_icon_path` is required.
---Each surface updates independently: a follow-up call that only sets
---`window_icon_path` swaps the OS icon without touching the in-app SVG,
---and vice-versa. The `svg`-painted surfaces are: title-bar slot,
---welcome screen, About modal, and the HTML stats export.
---
---@param config arbor.UiBrandingConfig
function Ui.set_branding(config) end

---Restore both the bundled SVG mark and the bundled window icon. No-op
---when the current override belongs to another plugin — protects against
---a plugin nuking another plugin's branding when it unloads.
function Ui.clear_branding() end

---@class arbor.UiThemeTokensConfig
---@field vars table<string, string>  CSS custom properties to overlay.
---                                    Every key must start with `--`.

---Layer a CSS-variable overlay on top of the active theme. Overlays
---survive theme switches: when the user picks a new theme Arbor reapplies
---the active theme first and then re-merges every plugin overlay. Each
---plugin owns one overlay slot — calling `set_theme_tokens` twice replaces
---the previous payload, and `clear_theme_tokens` releases just this
---plugin's slot.
---@param config arbor.UiThemeTokensConfig
function Ui.set_theme_tokens(config) end

---Drop this plugin's theme overlay; other plugins' overlays remain.
function Ui.clear_theme_tokens() end


-- =============================================================================
-- arbor.ui.contribute / contribution_point / unregister_contribution
--
-- Cross-plugin extension slots. A plugin (the "host") names a `point` and
-- contributors push `{id, payload, priority}` items to it. The host reads
-- the merged list at render time.
--
-- Naming convention: `<owner>:<scope>` (kebab + colon)
--   "arbor:context-menu"                  -- built-in context menu items
--   "arbor:command-palette"               -- built-in Ctrl+K commands
--   "compile-action:settings:section"     -- plugin-owned slot
--
-- Re-contributing with the same `(plugin, point, id)` REPLACES the previous
-- payload — the contribution model is idempotent on update.
--
-- ── Built-in contribution points (mirrored by every sugar API) ───────────────
-- Every `arbor.ui.add*` / `set*` / `register` call below also writes to one of
-- these points, so a plugin may contribute directly via `arbor.ui.contribute`
-- if it prefers. The sugar APIs and the contribute API are interchangeable.
--
--   Point                              Sugar API                         Payload shape
--   ─────────────────────────────────  ────────────────────────────────  ────────────────────────
--   arbor:context-menu:<target>        arbor.ui.add_context_menu_item       {target, label, action, icon?}
--                                      (target ∈ commit | branch | tag | stash | file
--                                       | remote | submodule | worktree | line | hunk | tab
--                                       | <plugin-defined>)
--   arbor:menu                         arbor.ui.add_menu_item              {label, action, icon?}
--   arbor:sidebar                      arbor.ui.add_sidebar               {action, label, icon?, side?, position?, kind?, …}
--   arbor:view                         arbor.ui.add_view                  {label, icon?, placement?, tooltip?}  (body content via set_panel_content)
--   arbor:diff-toolbar                 arbor.ui.add_toolbar_action(target="diff")           {label?, icon, action, tooltip?}
--   arbor:status-bar:left              arbor.ui.add_toolbar_action(target="status-bar:left")    {label?, icon?, action, tooltip?, color?}
--   arbor:status-bar:right             arbor.ui.add_toolbar_action(target="status-bar:right")   ›
--   arbor:title-bar:left               arbor.ui.add_toolbar_action(target="title-bar:left")     ›
--   arbor:title-bar:right              arbor.ui.add_toolbar_action(target="title-bar:right")    ›
--   arbor:commit-detail:action         arbor.ui.add_toolbar_action(target="commit-detail")  {label, icon?, action, tooltip?}     (ctx: oid)
--   arbor:commit-form:action           arbor.ui.add_toolbar_action(target="commit-form")    {label, icon?, action, tooltip?}     (ctx: staged summary)
--   arbor:editor-toolbar               arbor.ui.add_toolbar_action(target="editor")         {icon, action, label?, tooltip?, color?, path_pattern?}  (ctx: {path})
--   arbor:activitybar                  arbor.ui.add_graph_combo / Separator{kind="combo"|"separator", …}
--   arbor:command-palette              arbor.command.register            {title, description?, icon?, group?}
--   arbor:keybinding                   arbor.keybinding.register         {key, ctrl?, shift?, alt?, action, description?}
--   arbor:icon                         arbor.ui.icon.register            {svg}
--   arbor:tree-state                   arbor.ui.tree.set                 {title?, nodes[], version}
--   arbor:panel-content                arbor.ui.set_panel_content          {title?, nodes, actions?}
--   arbor:settings:panel               arbor.ui.settings.panel           {id, title, icon?, width?, …}
--
-- ── Decorator points (no sugar yet — use arbor.ui.contribute directly) ──────
--   arbor:branch-decorator             {branch_pattern?, label?, icon?, color?, tooltip?}
--   arbor:file-decorator               {path_pattern?, label?, icon?, color?, tooltip?}
--   arbor:welcome-action               {title, description?, icon?, action}
-- =============================================================================

---@class arbor.WhenClause
---@field kind        string|string[]|nil  Match if the context kind equals (or is in) this.
---@field data_field  { key: string, value: any }|nil  Match if ctx.data[key] == value.

---@class arbor.UiContributionItem
---@field id       string  Unique within (plugin, point). Required.
---@field priority integer|nil  Ascending order; lower renders first. Default 100.
---@field payload  any|nil  Free-form data shaped by the consumer of the point.
---@field when     arbor.WhenClause|nil  Optional gate — consumers that pass a
---                                       whenContext skip the item if no match.
---                                       Top-level since Phase 5; previously
---                                       lived inside `payload.when`.
---@field disabled boolean|nil  When true, consumers skip rendering this item
---                              while it stays in the registry. Top-level
---                              since Phase 5.
---@field group    string|nil   Optional group label for consumers that bucket
---                              contributions (palette sections, keybinding
---                              groups, …). Top-level since Phase 5.

---Push or replace a contribution under a named point.
---@param point string
---@param item  arbor.UiContributionItem
function Ui.contribute(point, item) end

---Shallow-merge `partial` into the existing payload of a previously
---contributed item at (this plugin, point, id). When no prior item exists,
---`partial` becomes the full payload. Use this to update one or two fields
---without having to re-specify the entire payload (e.g. swap a combo's
---`options` while preserving `target`/`run_action`/`variant`).
---@param point   string
---@param item_id string
---@param partial any
function Ui.contribute_patch(point, item_id, partial) end

---Remove a previously contributed item by id.
---@param point   string
---@param item_id string
function Ui.unregister_contribution(point, item_id) end

---@class arbor.UiContributionPointConfig
---@field name        string
---@field description string|nil  Free-form documentation hint
---@field schema      any|nil      Documentation-only schema; never validated

---Declare a contribution point so other plugins can discover it. Purely
---informational — contributing to a non-declared point is allowed.
---@param config arbor.UiContributionPointConfig
function Ui.contribution_point(config) end

---List all contributions currently pushed to a point. Useful for hosts that
---need to fold contributions into their own state at runtime.
---@param point string
---@return arbor.UiContributionItem[]
function Ui.list_contributions(point) end


-- =============================================================================
-- arbor.ui.settings — contribution-based settings panels
--
-- Replaces the legacy `[ui] has_settings / settings_action` manifest fields
-- and the per-plugin `arbor.ui.form()` settings flow. The modal is an
-- IntelliJ-style two-pane layout: a left SIDEBAR listing categories, and
-- a right CONTENT pane stacking the section cards of the selected category.
--
-- Three contribution slots define the panel surface:
--
--   `arbor:settings:panel`             — host registers the panel itself.
--   `<host>:settings:category`         — sidebar entries (one per language,
--                                         sub-system, plugin add-on, …).
--                                         Payload: { label, icon?, priority?,
--                                         description? }.
--   `<host>:settings:section`          — content cards. Payload:
--                                         { category, label?, icon?, count?,
--                                           add_action?, nodes (FormNode[]),
--                                           on_load?, on_save?, priority? }.
--                                         Sections without `category` go to a
--                                         synthetic "general" entry.
--   `<host>:settings:on_open`          — pre-open hooks. Payload: { action }.
--                                         Each contributed action is fired
--                                         SYNCHRONOUSLY before the modal
--                                         opens — use it to re-contribute
--                                         your sections with current state.
--
-- Anyone can contribute to any of the four points. External plugins can
-- (a) add a new sidebar entry, (b) drop a card into an existing entry, or
-- (c) replace an existing card by re-contributing with the same id.
--
-- Field name namespacing is automatic: every contributor's field names are
-- rewritten to `<contributor>::<field>` so two plugins can ship sections
-- without colliding. The modal's settings dispatcher un-prefixes on submit:
-- each section's `on_save` receives its own un-prefixed slice; the panel's
-- `on_save` (if any) receives the full state grouped by contributor:
--   ctx.sections = {
--     ["compile-action"]    = { jdk_id = "21", node_id = "20", ... },
--     ["maven-update-deps"] = { mirror_url = "https://…", ... },
--   }
-- =============================================================================

---@class arbor.UiSettingsPanelConfig
---@field id           string  Unique within this plugin (e.g. "main")
---@field title        string|nil  Modal title
---@field icon         string|nil  Lucide icon name
---@field width        string|nil  CSS width — default "960px"
---@field submit_label string|nil  Save button label — default "Save"
---@field on_load      string|nil  Plugin action fired BEFORE the orchestrator
---                                 reads contributions. Typical use: re-
---                                 contribute the host's own categories /
---                                 sections so they reflect current state.
---@field on_save      string|nil  Action fired with `{ sections, state }` on
---                                 Save. Per-section persistence should live
---                                 in each contributor's `on_save`; the host
---                                 only handles cross-cutting work.

---@class arbor.UiSettingsCategoryPayload
---@field label       string       Sidebar entry label
---@field icon        string|nil   Lucide icon name
---@field priority    integer|nil  Ascending — lower renders first (default 100)
---@field description string|nil   Muted intro paragraph above the section cards

---@class arbor.UiSettingsSectionPayload
---@field category   string|nil  Sidebar entry id this section pins to
---                              (default "general"). Must match the id of a
---                              `<host>:settings:category` contribution to be
---                              visible in the right pane.
---@field label      string|nil  Card header
---@field icon       string|nil  Lucide icon shown beside the header
---@field count      integer|nil Numeric badge in the card header
---@field add_action string|nil  Plugin action fired by the small "+" button
---                              in the card header (legacy slot; commonly
---                              wired to an auto-detect action).
---@field nodes      table[]     Form-DSL nodes — see arbor.UiFormNode
---@field on_load    string|nil  Pre-render hook fired with `{ host, prefix }`.
---                              Contributors populate initial values via
---                              `arbor.ui.form.set_value` with the prefixed name.
---@field on_save    string|nil  Action fired on Save with the un-prefixed slice
---                              of this section's fields. Contributor persists
---                              its own data here.
---@field priority   integer|nil Ascending — lower renders first (default 100)

---@class arbor.UiSettingsOnOpenPayload
---@field action string  Plugin action name fired BEFORE the orchestrator
---                      reads contributions. Use this to re-contribute your
---                      categories / sections with current state.

---@class arbor.UiSettings
local Settings = {}

---Register a settings panel. Idempotent — calling again with the same id
---replaces the previous registration. The gear icon in the Plugin Manager
---picks up the panel automatically (panels are stored as contributions to
---`arbor:settings:panel`).
---@param config arbor.UiSettingsPanelConfig
function Settings.panel(config) end

---Open a registered panel programmatically. Same effect as the user
---clicking the gear icon for `plugin_name`.
---@param plugin_name string
---@param panel_id    string
function Settings.open(plugin_name, panel_id) end

---Close the currently open settings panel, if any.
function Settings.close() end


-- =============================================================================
-- arbor.ui.tree — host-owned tree snapshots
--
-- A plugin that registered a sidebar with `kind = "tree"` (see
-- arbor.ui.add_sidebar) pushes the full tree shape via `arbor.ui.tree.set`.
-- Snapshots are written into the unified contribution registry under the
-- canonical point `"arbor:tree-state"` (item_id = sidebar_id); the frontend
-- reads them back through the contribution store and refreshes on the
-- coalesced `arbor://contributions-changed` event.
-- =============================================================================

---@class arbor.UiTreeSnapshot
---@field title string|nil
---@field nodes table[]   Array of TreeNode tables (id, label, kind, children, …)

---Replace the snapshot for the given sidebar/request id. Re-call to update;
---each call dual-writes into the contribution registry under
---`point="arbor:tree-state"` and triggers a coalesced
---`arbor://contributions-changed` event so consumers can react.
---@param sidebar_or_request_id string
---@param snapshot              arbor.UiTreeSnapshot
function Ui.tree.set(sidebar_or_request_id, snapshot) end

---Read the current snapshot back (returns nil when none has been set).
---@param sidebar_or_request_id string
---@return arbor.UiTreeSnapshot|nil
function Ui.tree.get(sidebar_or_request_id) end


-- =============================================================================
-- arbor.events — unified subscribe / emit
--
-- One namespace for both built-in hooks (`corvus:commit`, `garrulus:sync_done`,
-- `arbor:plugin_load`, …) and plugin-defined events. Everything on the bus is
-- `<namespace>:<event>`, so a subscriber never has to tell the two apart.
--
--   built-in hook   <product>:<event>   the product that owns the concept —
--                                       corvus, garrulus, pipeline, or arbor
--                                       for the host runtime itself
--   plugin event    <plugin>:<event>    the plugin that published it
--
-- The event half never repeats the namespace: `garrulus:note_saved`, not
-- `garrulus:vault_note_saved`.
--
-- Both sides auto-prefix an unqualified name, and the prefix each one supplies
-- is the only one it could mean:
--
--   -- plugin "compile-action"
--   arbor.events.emit("build-done", { status = "ok", job = "build-42" })
--
--   -- any other plugin (or the same one) subscribes to:
--   arbor.events.on("compile-action:build-done", function(ctx)
--     arbor.log.info("build finished: " .. ctx.status)
--   end)
--
-- `emit` auto-prefixes with this PLUGIN's name when no ':' is present.
-- Publishing under another plugin's namespace (e.g.
-- `arbor.events.emit("other-plugin:event", ...)`) raises a runtime error.
-- `on` auto-prefixes with the host PRODUCT's id, so inside a Garrulus plugin
-- `arbor.events.on("note_saved", fn)` is `garrulus:note_saved`. When the
-- product-qualified form is not a real hook but `arbor:` has one by that
-- event, it falls back to the host namespace: `on("plugin_load", fn)` is
-- `arbor:plugin_load` under every product, so one source line means one hook
-- no matter which host loads it. Write the qualified form to listen across
-- products — a name that already carries a ':' is never rewritten.
-- Delivery is asynchronous — `emit` returns immediately, subscribers run on a
-- background thread.
-- =============================================================================

---@class arbor.Events
local Events = {}

---Subscribe to a built-in hook (e.g. "corvus:commit") OR to a plugin event
---(e.g. "compile-action:build-done"). The event name may be the exact string
---or a glob pattern containing one or more "*" wildcards. Each "*" matches
---any sequence of characters (including empty strings and ":" separators).
---
---The `<product>:` prefix is OPTIONAL on a built-in hook: a name with no ":"
---is resolved against the product hosting this plugin, so "commit" inside a
---Corvus plugin is "corvus:commit". Hooks the host runtime owns (plugin
---lifecycle, views, theme, which project is open) fall back to "arbor:" when
---the product-qualified form is not a real hook, so "plugin_load" is
---"arbor:plugin_load" under every product. Spell the prefix out to listen to
---another product — a name that already carries a ":" is never rewritten.
---
---Examples:
---  "corvus:commit"              -- built-in hook, written out
---  "commit"                     -- same hook, resolved against the host product
---  "plugin_load"                -- "arbor:plugin_load" — host fallback
---  "garrulus:*"                 -- every Garrulus hook, including future ones
---  "compile-action:build-done"  -- exact match for a plugin event
---  "compile-action:*"           -- any event from compile-action
---  "*:note_saved"               -- git notes and vault notes alike
---  "*"                          -- every event fired (debug)
---
---The RESOLVED name is checked against the hook catalog: a name that matches
---no hook and no plugin event is reported in the plugin log rather than
---silently never firing. Patterns containing "*" are matched, not validated.
---
---A plugin with at least one wildcard subscription also receives built-in
---hooks without needing to declare them in the manifest.
---@param event string  Hook / event name or glob pattern with "*" wildcards
---@param fn    fun(ctx: any)
function Events.on(event, fn) end

---Emit a plugin event. The event name is auto-prefixed with this plugin's
---name when it contains no ':' (e.g. "build-done" -> "<plugin>:build-done").
---If the caller explicitly includes a colon, the prefix MUST equal this
---plugin's name — otherwise a runtime error is raised.
---
---Delivery is asynchronous: `emit` returns immediately and subscribers run
---on a background thread. Don't assume subscribers have executed by the time
---this function returns.
---@param event   string
---@param payload any|nil  Serialised to JSON once and delivered as a table
function Events.emit(event, payload) end


-- =============================================================================
-- arbor.service  — inter-plugin RPC (cross-VM dispatch)
-- =============================================================================
--- Providers expose named functions via arbor.service.export; other plugins
--- call them asynchronously with arbor.service.call. Args and returns travel
--- as JSON. The call returns a Promise that resolves with the provider's
--- return value, or rejects with a typed table { kind, message } where kind
--- is one of:
---
---   "not_found"        -- target plugin or method isn't registered
---   "plugin_disabled"  -- target plugin exists but is disabled
---   "handler_error"    -- provider threw while executing
---
--- Permissions: arbor.service.export / unexport / list_own require
--- `service_export = true`; arbor.service.call / list require
--- `service_call = true`. When neither is set, `arbor.service` is nil.
---
--- Example:
---   -- Provider "greeter":
---   arbor.service.export("greet", function(args)
---     return "hello " .. (args.name or "world")
---   end)
---
---   -- Consumer:
---   arbor.service.call("greeter.greet", { name = "Arbor" })
---     :ok(function(r) arbor.log.info(r) end)             -- "hello Arbor"
---     :err(function(e) arbor.log.warn(e.kind .. ": " .. e.message) end)
---
--- Or inside a coroutine:
---   arbor.async.run(function()
---     local r, err = arbor.async.await(
---       arbor.service.call("greeter.greet", { name = "Arbor" })
---     )
---     if err then ... end
---   end)
-- =============================================================================

---@class arbor.ServiceError
---@field kind    "not_found"|"plugin_disabled"|"handler_error"
---@field message string

---@class arbor.Service
local Service = {}

---Register a service method exported by this plugin. Other plugins invoke it
---via `arbor.service.call("<thisPlugin>.<method>", ...)`. Requires
---`service_export = true` in the manifest.
---@param method string
---@param fn     fun(args: any): any
function Service.export(method, fn) end

---Remove a previously-exported service method. Requires `service_export = true`.
---@param method string
function Service.unexport(method) end

---Return the list of method names this plugin currently exports.
---Requires `service_export = true`.
---@return string[]
function Service.list_own() end

---Asynchronously invoke a service exported by another plugin. Returns a
---Promise that resolves with the provider's return value or rejects with an
---`arbor.ServiceError`. The optional `cb` parameter is zucchero — it still
---receives `(ok, result_or_error)` alongside the promise so older code keeps
---working. Requires `service_call = true`.
---
---@param qualified string                                Full "plugin.method" name
---@param args      any|nil                               Payload (serialised to JSON)
---@param cb        fun(ok: boolean, result: any|arbor.ServiceError)|nil  Optional zucchero
---@return arbor.Promise
function Service.call(qualified, args, cb) end

---List every "<plugin>.<method>" currently exported across all enabled
---plugins — useful for debugging / discovery. Sorted by plugin, then method.
---A disabled plugin's exports are omitted: `arbor.service.call` would refuse
---them. Requires `service_call = true`.
---@return string[]
function Service.list() end


-- =============================================================================
-- arbor.command — command palette registration + invocation
-- =============================================================================

---Permission tier a command requires its *caller* to already hold. Supply a
---single domain/level pair; the first recognised pair wins. Omit for "no tier
---required" (any caller holding `command_invoke` may fire).
---@class arbor.CommandRequiredPerm
---@field git       "read"|"write"|"history_rewrite"|nil
---@field fs        "read"|"write"|nil
---@field issues    "read"|"write"|nil
---@field provider  "read"|"write"|nil
---@field toolchain "read"|"write"|nil
---@field terminal  "commands"|"any"|nil

---@class arbor.CommandConfig
---@field id          string                          Unique id within this plugin (e.g. "run-tests")
---@field title       string                          Display title shown in the Command Palette
---@field description string|nil                       Secondary line in the palette
---@field icon        string|nil                       Lucide icon name
---@field group       string|nil                       Section label used to bucket palette results
---@field invocable   boolean|nil                      When true, other plugins may fire this via `arbor.command.fire("<thisPlugin>::<id>")`. Default false (palette-only).
---@field required    arbor.CommandRequiredPerm|nil    Permission tier the caller must hold to fire this command. Only meaningful with `invocable = true`.

---@class arbor.Command
local Command = {}

---Register a Command Palette entry. Selecting it fires `command:<id>` on this
---plugin (handle it with `arbor.events.on("command:<id>", fn)`). Pass
---`invocable = true` to also let other plugins fire it via `arbor.command.fire`.
---@param config arbor.CommandConfig
function Command.register(config) end

---Remove a previously-registered command from the palette.
---@param id string
function Command.unregister(id) end

---Invoke a registered command. Two kinds are invocable:
---  * another plugin's command marked `invocable = true`, addressed as
---    `"<owner>::<id>"` — its `command:<id>` handler receives `ctx` (with any
---    declared `args` merged under the `args` key);
---  * a HOST BUILT-IN, addressed as `"arbor:area.verb"` — run by Arbor itself
---    (commit, push, refresh the UI, …). See `arbor.HostCommands` below.
---Fire-and-forget. Requires `command_invoke = true`, and the caller must hold
---whatever permission tier the target declares as `required` (host built-ins
---declare it too — e.g. the `arbor:git.*` commands require `git = "write"`).
---@param id  string    "<owner>::<id>" (plugin) or "arbor:area.verb" (host)
---@param ctx any|nil    Context table delivered to the command handler
function Command.fire(id, ctx) end

---Host built-in commands a plugin may invoke via `arbor.command.fire(id, ctx)`
---or a node `dispatch = { kind = "command", id = "..." }`. Closed by default —
---only the ids below are exposed; destructive / history-rewriting verbs are not.
---Git commands target the repo from `ctx.tab_id` (or the static `args.tab_id`),
---falling back to the active tab.
---
---   id                       required        ctx params
---   arbor:git.commit         git=write       message (req), amend?
---   arbor:git.push           git=write       refspec (req), remote? (=origin), force?
---   arbor:git.fetch          git=write       remote? (=origin)
---   arbor:git.pull           git=write       remote? (=origin)
---   arbor:git.branch_create  git=write       name (req), from_oid? (=HEAD)
---   arbor:git.checkout       git=write       name (req)
---   arbor:git.branch_delete  git=write       name (req)
---   arbor:git.stage_all      git=write       —
---   arbor:git.unstage_all    git=write       —
---   arbor:repo.refresh       (none)          — (re-loads the active repo view)
---   arbor:app.open_settings  (none)          — (opens the Settings panel)
---@class arbor.HostCommands


-- =============================================================================
-- arbor.keybinding
-- =============================================================================

---@class arbor.KeybindingConfig
---@field key         string      Single key character, e.g. "r", "F5"
---@field action      string      Plugin action name fired when the shortcut is pressed
---@field description string|nil  Human-readable label shown in Settings → Keybindings
---@field ctrl        boolean|nil
---@field shift       boolean|nil
---@field alt         boolean|nil

---@class arbor.Keybinding
local Keybinding = {}

---Register a global keyboard shortcut for this plugin.
---The action is fired via firePluginAction when the user presses the key combination.
---@param config arbor.KeybindingConfig
function Keybinding.register(config) end


-- =============================================================================
-- arbor.contribution — read-only introspection of the contribution registry.
--
-- Use cases:
--   • Detect that another plugin has overridden one of your contributions.
--   • Default conditionally based on what's currently registered.
--   • A coordinator plugin discovering items contributed by others.
--
-- There is no `subscribe` — listen to the existing `arbor://contributions-changed`
-- Tauri event from a plugin hook if you need to react to live changes.
-- =============================================================================

---@class arbor.ContributionRecord
---@field plugin_name string
---@field item_id     string
---@field point       string
---@field payload     any
---@field priority    integer
---@field when        arbor.WhenClause|nil
---@field disabled    boolean|nil
---@field group       string|nil

---@class arbor.ContributionPoint
---@field plugin_name string
---@field name        string
---@field description string|nil
---@field schema      any|nil

---@class arbor.Contribution
local Contribution = {}

---List all contributions registered against `point`, sorted by priority.
---@param point string
---@return arbor.ContributionRecord[]
function Contribution.list(point) end

---List every contribution point declared via `arbor.ui.contribution_point`.
---@return arbor.ContributionPoint[]
function Contribution.list_points() end


-- =============================================================================
-- arbor (global)
-- =============================================================================

---@class arbor.NotifyAction
---Tagged union — set `kind` to one of:
---  · "open-link-manager"   { kind, label, link_id }
---  · "open-tab-by-repo-id" { kind, label, repo_id }
---  · "open-url"            { kind, label, url }              -- web URLs only; file:// is ignored
---  · "open-path"           { kind, label, path, reveal? }    -- file → default editor; folder → Explorer; reveal=true opens parent dir
---  · "plugin-action"       { kind, label, plugin, action, ctx? }
---@field kind   string
---@field label  string
---@field link_id? string
---@field repo_id? string
---@field url?     string
---@field path?    string
---@field reveal?  boolean
---@field plugin?  string
---@field action?  string
---@field ctx?     table

---@class arbor.NotifyConfig
---@field message string                   Required, non-empty
---@field title?  string                   Optional, defaults to ""
---@field level?  arbor.NotifyLevel        Default: "info"
---@field toast?   boolean                 Default true; false = bell-only
---@field persist? boolean                 Default true; false = toast-only
---@field action? arbor.NotifyAction       Optional click-action

-- =============================================================================
-- arbor.mr — read-only git-provider MR / PR access (credential-blind)
-- =============================================================================
--
-- Plugins never see the OAuth token; the host resolves it internally.
-- Permission gate: `provider = "read"` in plugin.toml.

---@class arbor.MrUser
---@field login        string
---@field display_name string
---@field avatar_url?  string

---@class arbor.MrInfo
---@field number         integer  PR # on GitHub, MR iid on GitLab
---@field title          string
---@field description    string
---@field state          string   "open"|"closed"|"merged"
---@field isDraft        boolean
---@field author         arbor.MrUser
---@field sourceBranch   string
---@field targetBranch   string
---@field webUrl         string
---@field createdAt      string   ISO 8601
---@field updatedAt      string   ISO 8601
---@field provider       string   "github"|"gitlab"
---@field checksStatus   string   "pending"|"success"|"failed"|"none"
---@field commentsCount  integer

---@class arbor.MrListOptions
---@field repo_id? string         Workspace registry id; default: active repo
---@field state?   string         "open"|"closed"|"merged"|"all" (default "open")
---@field author?  string         Login filter, or the literal "current_user" sentinel
---@field labels?  string[]
---@field query?   string         Free-text query forwarded to the provider

---@class arbor.Mr
local Mr = {}

---List merge requests / pull requests for a repo registered in the workspace.
---Returns `(mrs, nil)` on success and `(nil, err)` on recoverable failure.
---When `author = "current_user"` the host resolves the authenticated user
---via the provider — plugins never have to know the login themselves.
---@param  opts? arbor.MrListOptions
---@return arbor.MrInfo[]|nil mrs
---@return string|nil         err
function Mr.list(opts) end

---@class arbor.MrCurrentUserOptions
---@field repo_id? string  Workspace registry id; default: active repo

---@class arbor.MrUserDetail
---@field id          string
---@field login       string
---@field name?       string
---@field email?      string
---@field avatar_url? string
---@field web_url?    string

---Return the authenticated user's identity on the provider attached to `repo_id`.
---Useful when the plugin wants to display "(you)" without ever touching the token.
---@param  opts? arbor.MrCurrentUserOptions
---@return arbor.MrUserDetail|nil user
---@return string|nil             err
function Mr.current_user(opts) end


-- =============================================================================
-- arbor.ci — read-only git-provider CI access
-- =============================================================================

---@class arbor.CiRun
---@field id            string
---@field name          string
---@field status        string   "pending"|"running"|"success"|"failed"|"cancelled"|"timed_out"
---@field branch        string
---@field commit_sha    string
---@field web_url       string
---@field created_at    string
---@field provider      string
---@field duration_secs? number

---@class arbor.CiRunsOptions
---@field repo_id?   string   Workspace registry id; default: active repo
---@field branch?    string   Filter to a specific branch
---@field status?    string   Filter to a specific status
---@field mr_number? integer  GitLab: scope to a specific MR
---@field per_page?  integer  Default: 20

---@class arbor.Ci
local Ci = {}

---List CI runs for a repo. Permission gate: `provider = "read"`.
---@param  opts? arbor.CiRunsOptions
---@return arbor.CiRun[]|nil runs
---@return string|nil        err
function Ci.runs(opts) end


---@class Arbor
---@field log          arbor.Log
---@field json         arbor.Json
---@field json_studio  arbor.JsonStudio
---@field fs           arbor.Fs
---@field repo         arbor.Repo
---@field mr           arbor.Mr
---@field ci           arbor.Ci
---@field issues       arbor.Issues
---@field meta         arbor.Meta
---@field settings     arbor.Settings
---@field credentials arbor.Credentials   Your own secrets, in the OS keychain
---@field terminal     arbor.Terminal
---@field job          arbor.Job
---@field timer        arbor.Timer
---@field scheduler    arbor.Scheduler
---@field ui           arbor.Ui
---@field keybinding   arbor.Keybinding
---@field command      arbor.Command
---@field contribution arbor.Contribution
---@field events       arbor.Events
---@field service      arbor.Service
---@field pipeline     arbor.Pipeline
local Arbor = {}

-- =============================================================================
-- arbor.issues — Linear / Jira issue tracker access (auto-routes per repo)
-- =============================================================================

---@class arbor.Issues
local Issues = {}

---Search issues. **Linear-only**: there is no `identifier` filter — pass
---an id-shaped string in `query` (e.g. `"ENG-42"`) for ID lookups, or use
---`lookup` for cross-tracker exact-id resolution.
---@param  filters? table
---@return table[]|nil issues
---@return string|nil  err
function Issues.search(filters) end

---Fetch a single issue by **Linear UUID** (NOT the human identifier).
---For "ENG-42"-style keys use `lookup`.
---@param  id string
---@return table|nil issue
---@return string|nil err
function Issues.get(id) end

---Resolve a single issue by its human identifier (e.g. `"ENG-42"`,
---`"PROJ-123"`), routing to the tracker bound to the active repo via
---`repo_config.issue_tracker`. Returns:
---  · table → matched issue
---  · nil   → no tracker configured, or no match
---  · (nil, err) → auth / network failure
---Each Arbor-registered project can have its own tracker, so the same
---plugin code works across mixed Linear / Jira workspaces.
---@param  identifier string
---@return table|nil issue
---@return string|nil err
function Issues.lookup(identifier) end

---Move an issue to a new workflow state. Linear-only.
---@param  id        string
---@param  status_id string
---@return table|nil issue
---@return string|nil err
function Issues.transition(id, status_id) end

---Add a comment to an issue. Linear-only.
---@param  issue_id string
---@param  body     string
---@return table|nil comment
---@return string|nil err
function Issues.comment(issue_id, body) end

---Pure compute: derive a git-branch slug from an issue table.
---@param  issue table
---@return string
function Issues.branch_name(issue) end

---Add a persistent notification to the in-app notification center. The
---boundary validates the config table: `message` must be a non-empty string,
---`level` (when supplied) must be one of "info"|"success"|"warning"|"error",
---and `action` (when supplied) must be a table.
---
---  arbor.notify{
---    title   = "Build done",
---    message = "exit 0 in 12s",
---    level   = "success",
---  }
---
---@param cfg arbor.NotifyConfig
function Arbor.notify(cfg) end

---The global arbor API instance — available in every plugin without require().
---@type Arbor
arbor = {}


-- =============================================================================
-- Built-in modules  (available via require)
-- =============================================================================

-- -----------------------------------------------------------------------------
-- require("arbor.schema")
-- -----------------------------------------------------------------------------

---@class arbor.Schema
local Schema = {}

---Validate data against a set of rules.
---Returns (ok, errors) where errors is a map of field → error message.
---
---  local ok, errs = schema.validate(ctx, {
---    name = { required = true, max_len = 64 },
---    url  = { required = true, pattern = "^https?://" },
---  })
---
---@param  data  table<string, any>
---@param  rules table<string, arbor.SchemaRule>
---@return boolean ok
---@return table<string, string> errors
function Schema.validate(data, rules) end

---Validate data and show the first error as a toast. Returns true if all rules pass.
---@param  data  table<string, any>
---@param  rules table<string, arbor.SchemaRule>
---@return boolean
function Schema.check(data, rules) end

-- -----------------------------------------------------------------------------
-- require("arbor.async")
-- -----------------------------------------------------------------------------

---@class arbor.Promise
---Async result handle. Producers (`arbor.service.call`, `arbor.job.spawn`,
---`arbor.ui.confirm`) return a Promise; consumers attach `:ok` / `:err` listeners
---or yield through `arbor.async.await` inside an `arbor.async.run` coroutine.
local Promise = {}

---Attach a success listener. Fires immediately if the promise is already
---fulfilled. Returns the same promise so calls chain.
---@param  fn fun(value: any)
---@return arbor.Promise
function Promise:ok(fn) end

---Attach a failure listener. Fires immediately if the promise is already
---rejected. Returns the same promise so calls chain.
---@param  fn fun(err: any)
---@return arbor.Promise
function Promise:err(fn) end

---Flat-map: returns a new Promise that adopts the value (or another promise)
---returned by the handler. Throwing inside a handler rejects the new promise.
---@param  on_ok  fun(value: any): any|arbor.Promise
---@param  on_err fun(err:   any): any|arbor.Promise|nil
---@return arbor.Promise
function Promise:and_then(on_ok, on_err) end

---@return "pending"|"fulfilled"|"rejected"
function Promise:state() end

---@return boolean
function Promise:is_pending() end

---@return boolean
function Promise:is_settled() end

---@class arbor.Async
local Async = {}

---@type arbor.Promise
Async.Promise = nil

---Run `fn` inside a coroutine that understands `arbor.async.await`. Errors
---raised inside the coroutine are logged via `arbor.log.error`.
---@param  fn fun(...): any
---@return thread coroutine
function Async.run(fn, ...) end

---Yield the current coroutine until `promise` settles. Must be called inside
---a coroutine started by `arbor.async.run`. Returns `(value, nil)` on resolve
---and `(nil, err)` on reject; non-promise values pass through as `(value, nil)`.
---@param  promise arbor.Promise|any
---@return any|nil value
---@return any|nil err
function Async.await(promise) end

---Return a debounced wrapper of fn.
---fn fires only after no further calls arrive for delay_ms milliseconds.
---@generic F: fun(...): any
---@param  fn       F
---@param  delay_ms integer
---@return F
function Async.debounce(fn, delay_ms) end

---Return a throttled wrapper of fn.
---At most one call per interval_ms is executed; intermediate calls are dropped.
---@generic F: fun(...): any
---@param  fn          F
---@param  interval_ms integer
---@return F
function Async.throttle(fn, interval_ms) end

-- -----------------------------------------------------------------------------
-- require("arbor.event")
-- -----------------------------------------------------------------------------

---@class arbor.Event
local Event = {}

---Subscribe to an internal plugin event.
---@param event string
---@param fn    fun(payload: any)
function Event.on(event, fn) end

---Unsubscribe from an internal plugin event.
---Pass fn to remove a specific handler; omit fn to remove all handlers for the event.
---@param event string
---@param fn    (fun(payload: any))|nil
function Event.off(event, fn) end

---Publish an internal plugin event to all registered handlers.
---Errors inside individual handlers are logged but do not stop other handlers.
---@param event   string
---@param payload any
function Event.emit(event, payload) end


-- =============================================================================
-- Form node types (for arbor.ui.form)
--
-- A form `config` table passed to `arbor.ui.form` contains a `nodes` array of
-- form nodes. Nodes are loosely typed — these classes exist only to enable
-- autocomplete for plugin authors. Fields not documented on a given node are
-- ignored at runtime.
-- =============================================================================

---@class arbor.SelectOption
---@field value       string
---@field label       string
---@field disabled    boolean|nil
---@field description string|nil  (radio only)

---Either a bare-string shortcut (expanded to { value = s, label = Capitalised s })
---or a full option table.
---@alias arbor.FormOptionInput arbor.SelectOption|string

---@class arbor.FormNodeBase
---@field id      string|nil
---@field show_if table|nil
---@field style   string|nil
---@field class   string|nil

---@class arbor.FormNodeSwitch : arbor.FormNodeBase
---@field type    "switch"
---@field field   string                   Name of the field whose value drives the branch.
---@field cases   table<string, table[]>   Map of possible values to arrays of child nodes.
---@field default table[]|nil              Fallback children when no case matches.

---@class arbor.FormNodeDate : arbor.FormNodeBase
---@field type     "date"
---@field name     string
---@field label    string|nil
---@field default  string|nil  ISO date, e.g. "2026-04-20"
---@field min      string|nil
---@field max      string|nil
---@field required boolean|nil
---@field readonly boolean|nil

---@class arbor.FormNodeDateTime : arbor.FormNodeBase
---@field type     "datetime"
---@field name     string
---@field label    string|nil
---@field default  string|nil  Local datetime, e.g. "2026-04-20T14:30"
---@field min      string|nil
---@field max      string|nil
---@field required boolean|nil
---@field readonly boolean|nil

---@class arbor.FormNodeTime : arbor.FormNodeBase
---@field type     "time"
---@field name     string
---@field label    string|nil
---@field default  string|nil  Time of day, e.g. "14:30"
---@field min      string|nil
---@field max      string|nil
---@field required boolean|nil
---@field readonly boolean|nil

---@class arbor.FormTab
---@field id       string
---@field label    string
---@field icon     string|nil   Lucide icon name (limited set; see docs)
---@field badge    string|number|nil  Small badge after the label (counts / warnings).
---@field disabled boolean|nil  Dim + non-selectable.
---@field tooltip  string|nil   Native tooltip on the tab.
---@field children table[]

---@class arbor.FormNodeTabs : arbor.FormNodeBase
---@field type         "tabs"
---@field tabs         arbor.FormTab[]
---@field default_tab  string|nil   Initial active tab id (defaults to first tab)
---@field lazy         boolean|nil  Render only the active tab's panel (inactive panels render nothing until selected, re-mounted on switch). Use for heavy panels — a big highlighted code dump, hundreds of cards — to keep the DOM small and interaction snappy. Field values are still collected from every tab. Default false.
---@field persist_key  string|nil   When set, the active tab id is mirrored to `localStorage[persist_key]` so the user's selection survives reopening the modal. Restoration is guarded against stale ids (an id that no longer exists in the current `tabs` falls back to `default_tab`, then to the first tab). Doubles as the cross-renderer sync key: two `tabs` widgets in the same modal that share a `persist_key` (typically one `strip_only` in `header.centre` and one `panels_only` in `nodes`) read and write the same in-memory slot, so clicking on one updates the other in lock-step.
---@field strip_only   boolean|nil  When true, render only the tab strip — skip the per-tab panel divs entirely. Designed for the "view-mode switcher in `header.centre`" pattern: the strip lives in the header for the Studio-shaped chrome look, while a second `tabs` widget in the body (same `persist_key`, `panels_only = true`) renders the panel content. Default: false.
---@field panels_only  boolean|nil  When true, render only the per-tab panel divs — skip the tab strip. Mirror of `strip_only`: typically paired with a `strip_only` tabs in `header.centre` so the body shows panels without a duplicate strip. Default: false.

-- =============================================================================
-- Studio-shaped modal chrome (`arbor.ui.form{...}` top-level subkeys)
-- =============================================================================

---Icon shown next to the title in a Studio-shaped modal header. Exactly one
---variant must be present. Raw SVG markup is intentionally not accepted —
---use `image` (URL: `file://`, `data:`, `https://`) for custom pictograms.
---@class arbor.FormHeaderIcon
---@field lucide string|nil   Lucide icon name (mutually exclusive with brand / image)
---@field brand  string|nil   Provider brand id — one of "github" / "gitlab" / "bitbucket" / "linear" / "jira"
---@field image  string|nil   URL pointing at a PNG / SVG file (file:// / data: / https://)

---Studio-shaped modal header. When set on `arbor.ui.form{...}`, replaces the
---default `<ModalHeader>` (plugin tag + title) with a richer header that
---mirrors the host's Studio modals (icon · title · meta · left / centre /
---right zones · close).
---@class arbor.FormHeaderCfg
---@field icon         arbor.FormHeaderIcon|nil  Pictogram before the title.
---@field subtitle     string|nil                Secondary single-line caption (muted), e.g. file path.
---@field dirty        boolean|nil               Render a `●` dirty marker after the title.
---@field tooltip      string|nil                Tooltip on the title (typically the full file path).
---@field size_meta    string|nil                Right-aligned meta pill (e.g. "12.4 KB · 412 lines").
---@field left         table[]|nil               FormNodes rendered after the title, before centre.
---@field centre       table[]|nil               FormNodes rendered in the centre — typically a `tabs` for view-mode switching.
---@field right        table[]|nil               FormNodes rendered before the host-owned close button.
---@field experimental table|nil                 When set (`{ description = "…" }`), render an ExperimentalBadge next to the title.

---One item in a Studio-shaped modal's right (or left) activity bar.
---Activity-bar items are ROUTING-ONLY — clicking one opens / focuses the
---sidecar with the same `id`. Items that need to fire an action (Open file…,
---Save As…) belong in `header.left` / `header.right` as `button` FormNodes.
---Use `{ separator = true }` to insert a thin divider between groups.
---@class arbor.FormActivityBarItem
---@field id        string|nil   Stable id; must match a key in `sidecars`. Required unless `separator = true`.
---@field icon      string|nil   Lucide icon name. Required unless `separator = true`.
---@field label     string|nil   Sidecar label (shown as tooltip + aria-label).
---@field tooltip   string|nil   Override tooltip (defaults to `label`).
---@field count     number|nil   Numeric badge shown on the icon (omit / 0 = hidden).
---@field dot       boolean|nil  Accent dot for "has unread content / dirty pane".
---@field tone      string|nil   Override badge / dot tone — "info" / "success" / "warning" / "error" / "accent" / "muted".
---@field disabled  boolean|nil  Render as disabled (click no-ops).
---@field separator boolean|nil  When true, render as a thin divider line instead of a button.

---Activity-bar configuration. Items map 1:1 to sidecars — every item's `id`
---must match a key in `sidecars` or a console warning fires at mount.
---@class arbor.FormActivityBarCfg
---@field side         string|nil                       "left" / "right" (default) / "both".
---@field items        arbor.FormActivityBarItem[]|nil  When `side` is left or right.
---@field left_items   arbor.FormActivityBarItem[]|nil  When `side` is "both".
---@field right_items  arbor.FormActivityBarItem[]|nil  When `side` is "both".
---@field default      string|nil                       Item id active on first mount (when no stored history).
---@field storage_key  string|nil                       When set, the active sidecar id is mirrored to `localStorage[storage_key]`.
---@field always_open  boolean|nil                      When true, one item is always selected (cannot close to nil). Default: false.

---Sidecar pane keyed by activity-bar item id. The pane is a FormNode subtree;
---value-bearing nodes participate in the modal's shared submit payload.
---Sidecars are always mounted (children survive close + reopen); the
---collapse / expand happens via CSS width transition.
---@class arbor.FormSidecarCfg
---@field width    number|nil  Pixel width of the pane. Default: 320.
---@field title    string|nil  Optional header line above the pane contents.
---@field side     string|nil  Edge the pane slides in from: "right" (default) or "left". Use "left" to sit beside a left-side activity bar (e.g. an entity navigator).
---@field children table[]     Pane contents as FormNodes.

---Modal footer override. Each zone is a FormNode list rendered horizontally;
---unset zones fall through to the default Submit / Cancel / wizard chrome.
---@class arbor.FormFooterCfg
---@field status table[]|nil  Left status row — typically state_block_pill + breadcrumb.
---@field center table[]|nil  Centre — typically undo / redo + Format / Convert tool buttons.
---@field right  table[]|nil  Right — replaces the default Submit / Cancel cluster. Pass `{}` to hide all right-side controls.

---Optional full-body fallback state — when any subkey is set, the body
---`nodes` are hidden and the matching block is rendered instead. Flip live
---with `arbor.ui.form.set_state_block(name, cfg?)` (`name = nil` clears).
---@class arbor.FormStateBlockCfg
---@field loading table|nil  Spinner overlay — `{ label?: string }`.
---@field error   table|nil  Error block — `{ label: string }` (required).
---@field empty   table|nil  Empty-doc state — `{ title?, body?, cta_label?, cta_action? }`.


-- =============================================================================
-- arbor.pipeline
-- =============================================================================

---@alias arbor.LogLevel "debug"|"info"|"warn"|"error"
---@alias arbor.StageMode "sequential"|"parallel"

---@class arbor.LuaOpSpec
---@field op       string               Op name registered via arbor.pipeline.register_op
---@field params   any|nil              Arbitrary serialisable payload passed to the handler
---@field plugin   string|nil           Override the target plugin; default = pipeline's plugin

---A step is either a shell step (`command` set) or a LuaOp step (`lua_op` set).
---If both are provided, `lua_op` wins. Omitting both is a define-time error.
---@class arbor.PipelineStepDef
---@field id             string
---@field name           string
---@field command        string|nil               Shell command (run via sh -c / cmd /C)
---@field lua_op         arbor.LuaOpSpec|nil      Native step: invoke a registered Lua handler
---@field cwd            string|nil               Working dir; nil = active repo root
---@field allow_failure  boolean|nil              When true, stage continues on non-zero exit

---@class arbor.PipelineStageDef
---@field id             string
---@field name           string
---@field steps          arbor.PipelineStepDef[]
---@field mode           arbor.StageMode|nil    Default "sequential"
---@field max_parallel   integer|nil            Cap when mode="parallel" (nil = unlimited)

---@class arbor.PipelineDef
---@field id             string
---@field name           string
---@field description    string|nil
---@field icon           string|nil             Emoji or icon identifier
---@field stages         arbor.PipelineStageDef[]
---@field lock_key       string|nil             Concurrency key; default "<plugin>:<id>"
---@field log_level      arbor.LogLevel|nil     Default "info"
---@field silent         boolean|nil            Default false. When true, the host suppresses its automatic start-toast and done-notification for runs of this pipeline (use when the plugin already surfaces its own lifecycle messages).

---Calling `arbor.pipeline(id)` returns a chainable PipelineBuilder that
---compiles down to `arbor.pipeline.define(table)` on `:commit()`. The
---table-config entry point keeps working unchanged.
---@operator call(string): arbor.PipelineBuilder
---@operator call(arbor.PipelineDef): arbor.PipelineBuilder
---@class arbor.Pipeline
local Pipeline = {}

---Register (or replace) a pipeline definition belonging to this plugin.
---@param  def arbor.PipelineDef
function Pipeline.define(def) end

---Start a new run of the named pipeline. Returns `(run_id, nil)` on
---success or `(nil, err)` on failure (typical Lua multi-return convention).
---
---Called with a single table — `arbor.pipeline.run{ pipeline_id = "build" }`.
---  · `opts.pipeline_id` — required, must match a previously `define`d def
---  · `opts.cwd`         — overrides the per-step working directory; when
---                         omitted steps run against the active repo root
---                         (or `"."` if no repo is open)
---  · `opts.silent`      — when true, suppresses the host's automatic
---                         start-toast and done-notification for this
---                         specific run (overrides the def's default).
---                         Pass `false` to force the toast/notify even
---                         when the def was registered with `silent = true`.
---@param  opts { pipeline_id: string, cwd: string|nil, silent: boolean|nil }
---@return string|nil run_id
---@return string|nil err
function Pipeline.run(opts) end

---Request cancellation of a running pipeline. Stops after the current step.
---@param  run_id string
function Pipeline.cancel(run_id) end

---Resume a run in state `failed` or `paused` from the step(s) that halted it.
---Already-successful steps are skipped. Errors when the pipeline's concurrency
---lock is currently held by another run. Returns `(true, nil)` on success,
---`(false, err)` on failure.
---@param  run_id string
---@return boolean ok
---@return string|nil err
function Pipeline.resume(run_id) end

---Drop a terminal run permanently — removes in-memory state and the persisted
---JSON file. Refuses to act on a run that is still `running`. Returns
---`(true, nil)` on success, `(false, err)` on failure.
---@param  run_id string
---@return boolean ok
---@return string|nil err
function Pipeline.discard(run_id) end

---Return the `run_id` that currently holds the given concurrency lock, or
---`nil` when free. Useful to pre-flight a "can I start?" check.
---@param  lock_key string
---@return string|nil
function Pipeline.is_locked(lock_key) end

---List the pipeline definitions registered by this plugin.
---@return arbor.PipelineDef[]
function Pipeline.list() end

---Look up a single pipeline definition belonging to this plugin.
---Returns the def table or `nil` when no pipeline with that id is
---registered. Useful in re-define paths to inherit settings (e.g. the
---display name) from the existing registration without re-deriving them
---from external state.
---@param  pipeline_id string
---@return arbor.PipelineDef|nil
function Pipeline.get(pipeline_id) end

---List pipeline runs, most-recent-first. Without `opts` returns runs
---belonging to this plugin only.
---  · `opts.plugin`      — filter by plugin name (defaults to this plugin)
---  · `opts.pipeline_id` — additionally filter by pipeline id
---  · `opts.all`         — when true, return runs from every plugin
---                         (ignores `opts.plugin`)
---@param  opts? { plugin?: string, pipeline_id?: string, all?: boolean }
---@return arbor.PipelineRun[]
function Pipeline.list_runs(opts) end

---Look up a single pipeline run by id (any plugin). Returns `nil` when
---the run doesn't exist (or has been discarded).
---@param  run_id string
---@return arbor.PipelineRun|nil
function Pipeline.get_run(run_id) end

---Register a Lua function invoked by the pipeline orchestrator when a step's
---`lua_op.op` matches `name`. The handler runs inside this plugin's Lua VM —
---no shell round-trip, full access to the `arbor.*` API.
---
---Signature of the handler: `function(params, ctx) -> result`
---   · `params`  table from the step's `lua_op.params` (arbitrary shape)
---   · `ctx.cwd` working directory resolved by the orchestrator (step's `cwd`
---               or the run's effective cwd when unset)
---   · `ctx.plugin` target plugin (normally this plugin's name)
---
---Accepted return shapes:
---   · `nil` / no return  → exit_code = 0
---   · `true` / `false`   → exit_code 0 / 1
---   · `<number>`         → that exit code
---   · `<string>`         → stdout, exit_code = 0
---   · `{ exit_code?, stdout?, stderr? }` → structured form
---Raising an error marks the step Failed with the error captured.
---@param name    string
---@param handler fun(params:table, ctx:table):any
function Pipeline.register_op(name, handler) end

---Unregister a previously-registered pipeline op.
---@param name string
function Pipeline.unregister_op(name) end

---Debugging helper: list every pipeline op currently registered across all
---enabled plugins as `"<plugin>.<op>"` strings.
---@return string[]
function Pipeline.list_ops() end

---@class arbor.PipelineStepRun
---@field def_id      string
---@field name        string
---@field status      "pending"|"running"|"paused"|"success"|"failed"|"cancelled"
---@field output      string[]
---@field started_at  integer|nil   Unix millis
---@field finished_at integer|nil
---@field exit_code   integer|nil

---@class arbor.PipelineStageRun
---@field def_id string
---@field name   string
---@field status string
---@field steps  arbor.PipelineStepRun[]

---@class arbor.PipelineRun
---@field id            string
---@field pipeline_id   string
---@field plugin        string
---@field name          string
---@field status        "pending"|"running"|"paused"|"success"|"failed"|"cancelled"
---@field started_at    integer|nil
---@field finished_at   integer|nil
---@field stages        arbor.PipelineStageRun[]
---@field lock_key      string
---@field log_level     arbor.LogLevel
---@field repo_path     string|nil

---List pipeline runs tracked by the runtime. Defaults to runs belonging to
---this plugin. Pass `{ all = true }` for every plugin, `{ plugin = "..." }`
---for a specific one, or `{ pipeline_id = "..." }` to scope to a single def.
---@param  opts { plugin: string|nil, pipeline_id: string|nil, all: boolean|nil }|nil
---@return arbor.PipelineRun[]
function Pipeline.list_runs(opts) end

---Fetch a single run by id. Returns `nil` when the run is not in the registry
---(e.g. already discarded or never started).
---@param  run_id string
---@return arbor.PipelineRun|nil
function Pipeline.get_run(run_id) end


-- =============================================================================
-- arbor.PipelineBuilder — chainable sugar for arbor.pipeline.define
--
-- Returned by `arbor.pipeline("id")`. Every method returns the builder so calls
-- can be chained. `:commit()` compiles to a PipelineDef and registers it.
--
--   arbor.pipeline("deploy")
--     :name("Deploy to staging")
--     :icon("Rocket")
--     :lock("staging-deploy")
--     :stage("build"):shell("npm run build")
--     :stage("upload"):mode("parallel"):max_parallel(4)
--       :run("s3.upload", { src = "dist", bucket = "my-app" })
--     :commit()
-- =============================================================================

---@class arbor.PipelineBuilder
local PipelineBuilder = {}

---@param  v string
---@return arbor.PipelineBuilder
function PipelineBuilder:name(v) end

---@param  v string
---@return arbor.PipelineBuilder
function PipelineBuilder:description(v) end

---@param  v string
---@return arbor.PipelineBuilder
function PipelineBuilder:icon(v) end

---Concurrency lock key — only one run per key may be Running. Alias `:lock_key`.
---@param  v string
---@return arbor.PipelineBuilder
function PipelineBuilder:lock(v) end

---@param  v string
---@return arbor.PipelineBuilder
function PipelineBuilder:lock_key(v) end

---@param  v arbor.LogLevel
---@return arbor.PipelineBuilder
function PipelineBuilder:log_level(v) end

---Suppress the host's automatic start-toast / done-notification for runs of
---this pipeline (default false). Per-run override available via
---`arbor.pipeline.run{ silent = ... }`.
---@param  v boolean|nil  Treats only `false` as "off"; nil → true.
---@return arbor.PipelineBuilder
function PipelineBuilder:silent(v) end

---Begin a new stage. Subsequent `:run` / `:shell` / `:step` calls add steps
---to it. Pass a string (stage name; id is slugified) or a table for full
---control over `{ id, name, mode, max_parallel }`.
---@param  name_or_cfg string|{id: string|nil, name: string, mode: arbor.StageMode|nil, max_parallel: integer|nil}
---@return arbor.PipelineBuilder
function PipelineBuilder:stage(name_or_cfg) end

---Set the mode of the current stage (sequential | parallel).
---@param  m arbor.StageMode
---@return arbor.PipelineBuilder
function PipelineBuilder:mode(m) end

---Cap concurrency when the current stage is parallel.
---@param  n integer
---@return arbor.PipelineBuilder
function PipelineBuilder:max_parallel(n) end

---Add a Lua-op step. Two shapes:
---   :run("op_name", { params })
---   :run({ op = "op_name", params = {...}, plugin = "...", id?, name?, allow_failure?, cwd? })
---@param  op_or_cfg string|{op: string, params: any|nil, plugin: string|nil, id: string|nil, name: string|nil, allow_failure: boolean|nil, cwd: string|nil}
---@param  params    any|nil
---@return arbor.PipelineBuilder
function PipelineBuilder:run(op_or_cfg, params) end

---Add a shell step. Either:
---   :shell("make build")
---   :shell({ command = "make", cwd = "...", id?, name?, allow_failure? })
---@param  cmd_or_cfg string|{command: string, cwd: string|nil, id: string|nil, name: string|nil, allow_failure: boolean|nil}
---@return arbor.PipelineBuilder
function PipelineBuilder:shell(cmd_or_cfg) end

---Escape hatch — push a raw step config table.
---@param  cfg arbor.PipelineStepDef
---@return arbor.PipelineBuilder
function PipelineBuilder:step(cfg) end

---Finalise the builder and call `arbor.pipeline.define` with the assembled
---config. Returns whatever `define` returns (`nil` on success).
function PipelineBuilder:commit() end


-- =============================================================================
-- arbor.FormBuilder — chainable sugar for arbor.ui.form
--
-- Returned by `arbor.ui.form()` or `arbor.ui.form("id")`. Every method returns
-- the builder so calls can be chained. `:open()` emits the form modal via the
-- same path as the legacy `arbor.ui.form{...}` table call.
--
--   arbor.ui.form()
--     :title("Settings")
--     :section("Identity")
--       :text("username", { label = "Your name", placeholder = "Alice" })
--     :section("Appearance")
--       :select("theme", { label = "Theme", options = {"dark","light"} })
--     :submit("Save", "settings:save")
--     :open()
-- =============================================================================

---@class arbor.FormBuilder
local FormBuilder = {}

---@param  v string
---@return arbor.FormBuilder
function FormBuilder:title(v) end

---@param  v string
---@return arbor.FormBuilder
function FormBuilder:description(v) end

---@param  v string
---@return arbor.FormBuilder
function FormBuilder:submit_label(v) end

---@param  v string
---@return arbor.FormBuilder
function FormBuilder:cancel_label(v) end

---Set the submit action (and optionally its label).
---   :submit("save:action")
---   :submit("Save", "save:action")
---@param  label_or_action string
---@param  action          string|nil
---@return arbor.FormBuilder
function FormBuilder:submit(label_or_action, action) end

---@param  action string
---@return arbor.FormBuilder
function FormBuilder:on_submit(action) end

---@param  action_or_cfg string|{label: string|nil, action: string}
---@return arbor.FormBuilder
function FormBuilder:cancel(action_or_cfg) end

---@param  action string
---@return arbor.FormBuilder
function FormBuilder:on_cancel(action) end

---Echo state forwarded back to the plugin in the submit ctx.
---@param  t table
---@return arbor.FormBuilder
function FormBuilder:state(t) end

---Open a new section. Subsequent fields attach to it. Calling :section() again
---auto-closes the previous section so flat layouts read naturally.
---@param  title_or_cfg string|table
---@return arbor.FormBuilder
function FormBuilder:section(title_or_cfg) end

---Explicitly close the current section so subsequent calls push at top level.
---@return arbor.FormBuilder
function FormBuilder:end_section() end

---Single-line text input — also reused for `password` / `email` / `url`.
---Live-edit hook: set `actions = { change = "ns:on_query" }` (string action or
---`DispatchTarget`) to dispatch the new value on each keystroke, trailing-edge
---debounced by `debounce_ms` (default 250). The scoped payload is
---`{ node_id, slot = "change", value, state? }`; the legacy whole-form payload
---fires when `change` is a bare string. Use this for filters / live search.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:text(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:password(name_or_cfg, opts) end

---Multi-line text input. Same `actions.change` + `debounce_ms` shape as `:text`.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:textarea(name_or_cfg, opts) end

---Click-to-edit single-line field. See `arbor.FormFieldInlineEdit`.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:inline_edit(name_or_cfg, opts) end

---Numeric stepper input. Same `actions.change` + `debounce_ms` live-edit
---shape as `:text`. Use for filters that scrub a numeric range.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:number(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:select(name_or_cfg, opts) end

---Multi-value variant of `select`. See `arbor.FormFieldMultiselect`.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:multiselect(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:checkbox(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:toggle(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:radio(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:kv_list(name_or_cfg, opts) end

---Value-bearing git branch picker — same chrome as the host's
---`<BranchSelect>` (monospace dropdown, search above 12 entries, sticky
---entry for a value not in the list). The plugin owns the `branches` list;
---typical call: `arbor.repo.branches()` → map `.name`. See
---`arbor.FormFieldBranchSelect`.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:branch_select(name_or_cfg, opts) end

---CodeMirror 6 editor field. See `arbor.FormFieldEditor` for the full shape
---(language, height, on_edit / on_select scoped slots, …).
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:editor(name_or_cfg, opts) end

---Read-only diff viewer. See `arbor.FormNodeDiff` for the full shape (hunks,
---mode, language, height, …). Display-only — not collected into the form
---values; update it live with `arbor.ui.form.patch` (`merge` the `hunks`).
---@param  cfg table   { hunks = {…}, path?, mode?, language?, … }
---@return arbor.FormBuilder
function FormBuilder:diff(cfg) end

---Hierarchical selector (value-bearing). Pass a cfg table with `nodes`; opt into
---the dynamic data-tree with `lazy = true` + `on_expand` (+ optional `on_select`
---/ `virtualize_threshold` / `row_height`). See `arbor.FormFieldTree`.
---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:tree(name_or_cfg, opts) end

---@return arbor.FormBuilder
function FormBuilder:divider() end

---@param  text_or_cfg string|table
---@return arbor.FormBuilder
function FormBuilder:label(text_or_cfg) end

---@param  text string
---@return arbor.FormBuilder
function FormBuilder:paragraph(text) end

---@param  text string
---@return arbor.FormBuilder
function FormBuilder:heading(text) end

---@param  cfg arbor.FormNodeButton
---@return arbor.FormBuilder
function FormBuilder:button(cfg) end

---Display-only horizontal trail. See `arbor.FormNodeBreadcrumb`.
---@param  cfg arbor.FormNodeBreadcrumb
---@return arbor.FormBuilder
function FormBuilder:breadcrumb(cfg) end

---Monospace URL/identifier block. See `arbor.FormNodeUrlBlock`.
---@param  cfg arbor.FormNodeUrlBlock
---@return arbor.FormBuilder
function FormBuilder:url_block(cfg) end

---1-2 letter monogram tile. See `arbor.FormNodeMonogram`.
---@param  cfg arbor.FormNodeMonogram
---@return arbor.FormBuilder
function FormBuilder:monogram(cfg) end

---Centered status message (loading / error / empty / success / info).
---See `arbor.FormNodeStateBlock`.
---@param  cfg arbor.FormNodeStateBlock
---@return arbor.FormBuilder
function FormBuilder:state_block(cfg) end

---Wizard-style step indicator (visual-only). See `arbor.FormNodeStepIndicator`.
---@param  cfg arbor.FormNodeStepIndicator
---@return arbor.FormBuilder
function FormBuilder:step_indicator(cfg) end

---"Preview before bulk action" panel with severity-coded chips.
---See `arbor.FormNodeStatusList`.
---@param  cfg arbor.FormNodeStatusList
---@return arbor.FormBuilder
function FormBuilder:status_list(cfg) end

---App-message banner (default) OR in-document callout (`style = "inline"`).
---See `arbor.FormNodeAlert`.
---@param  cfg arbor.FormNodeAlert
---@return arbor.FormBuilder
function FormBuilder:alert(cfg) end

---Hero header card (avatar + title + meta + actions).
---See `arbor.FormNodeInfoCard`.
---@param  cfg arbor.FormNodeInfoCard
---@return arbor.FormBuilder
function FormBuilder:info_card(cfg) end

---Horizontal pill selector (filter / category chips). See `arbor.FormNodeChipBar`.
---@param  cfg arbor.FormNodeChipBar
---@return arbor.FormBuilder
function FormBuilder:chip_bar(cfg) end

---Standalone click-to-copy button with chrome.
---See `arbor.FormNodeCopyButton`.
---@param  cfg arbor.FormNodeCopyButton
---@return arbor.FormBuilder
function FormBuilder:copy_button(cfg) end

---Small "Experimental" pill. See `arbor.FormNodeExperimentalBadge`.
---@param  cfg arbor.FormNodeExperimentalBadge
---@return arbor.FormBuilder
function FormBuilder:experimental_badge(cfg) end

---Standalone section title bar (no children body).
---See `arbor.FormNodeSectionHeader`.
---@param  cfg arbor.FormNodeSectionHeader
---@return arbor.FormBuilder
function FormBuilder:section_header(cfg) end

---Action-only chip-style filter button. See `arbor.FormNodeFilterButton`.
---@param  cfg arbor.FormNodeFilterButton
---@return arbor.FormBuilder
function FormBuilder:filter_button(cfg) end

---Panel chrome wrapper (icon + title + count + actions + toolbar + body +
---footer) — same look as the host's `<PanelShell>` widget. See
---`arbor.FormNodePanelShell`.
---@param  cfg arbor.FormNodePanelShell
---@return arbor.FormBuilder
function FormBuilder:panel_shell(cfg) end

---Bottom-panel title bar (header only — pair with sibling layout nodes
---for the body). See `arbor.FormNodeBottomPanelHeader`.
---@param  cfg arbor.FormNodeBottomPanelHeader
---@return arbor.FormBuilder
function FormBuilder:bottom_panel_header(cfg) end

---Wraps one or more child nodes with the host's singleton tooltip — smart
---placement, viewport-aware flipping, keyboard focus, optional shortcut
---hint, optional Markdown body. `display` defaults to `"inline"` (`<span>`,
---fits buttons / badges / icons); pass `display = "block"` when wrapping a
---block-level subtree. See `arbor.FormNodeTooltip`.
---@param  cfg arbor.FormNodeTooltip
---@return arbor.FormBuilder
function FormBuilder:tooltip(cfg) end

---Display-only colour swatch — chip-only or labelled card row. Distinct
---from the value-bearing `color` field (HTML5 input). See
---`arbor.FormNodeColorSwatch`.
---@param  cfg arbor.FormNodeColorSwatch
---@return arbor.FormBuilder
function FormBuilder:color_swatch(cfg) end

---Display-only keybinding badge — boxed `<kbd>` chrome or plain inline
---monospace text (IntelliJ-menu style). Resolves the visible label from
---`action` (live lookup against the user's keybindings) → `binding` →
---`keys` → `label`. See `arbor.FormNodeKbd`.
---@param  cfg arbor.FormNodeKbd
---@return arbor.FormBuilder
function FormBuilder:kbd(cfg) end

---Display-only uppercase type pill — one-word type hint with curated
---palette (`kind`) or explicit semantic tone (`tone`). See
---`arbor.FormNodeTypePill`.
---@param  cfg arbor.FormNodeTypePill
---@return arbor.FormBuilder
function FormBuilder:type_pill(cfg) end

---Display-only charset indicator — small monospace pill, warning-tinted
---when overridden. Presentational only; pair with a sibling `select` to
---let the user pick. See `arbor.FormNodeEncodingPill`.
---@param  cfg arbor.FormNodeEncodingPill
---@return arbor.FormBuilder
function FormBuilder:encoding_pill(cfg) end

---Display-only round avatar — initials from `name`, stable hue from
---`email` or `name`. For people; for entities (workspaces, plugins) use
---`monogram` instead. See `arbor.FormNodeAvatar`.
---@param  cfg arbor.FormNodeAvatar
---@return arbor.FormBuilder
function FormBuilder:avatar(cfg) end

---Display-only monochrome provider brand glyph — same chrome as the host's
---`<BrandIcon>`. Renders the canonical `simple-icons` mark in
---`currentColor`, so it inherits the surrounding text colour. Use for
---activity bar / inline glyphs; for branded swatches with the brand
---background colour use `brand_tile` instead. See `arbor.FormNodeBrandIcon`.
---@param  cfg arbor.FormNodeBrandIcon
---@return arbor.FormBuilder
function FormBuilder:brand_icon(cfg) end

---Display-only branded square tile — same chrome as the host's
---`<BrandTile>`. Composes the brand glyph on the brand's hard-coded
---background colour (GitHub dark, GitLab orange, …) with a fixed bright
---foreground (brand contrast does not borrow theme tokens). Use for auth
---tiles / provider cards / welcome screens. See `arbor.FormNodeBrandTile`.
---@param  cfg arbor.FormNodeBrandTile
---@return arbor.FormBuilder
function FormBuilder:brand_tile(cfg) end

---Display-only provider user identity row — avatar + name + optional
---secondary line (email / @handle / domain), with click-to-copy on each
---line. Same chrome as the host's `<ProviderUserBadge>` used in provider
---settings cards. See `arbor.FormNodeProviderUserBadge`.
---@param  cfg arbor.FormNodeProviderUserBadge
---@return arbor.FormBuilder
function FormBuilder:provider_user_badge(cfg) end

---Escape hatch — push an arbitrary node table (any `type`, any extra fields).
---@param  node table
---@return arbor.FormBuilder
function FormBuilder:field(node) end

---Finalise the builder and emit the form via the legacy opener.
function FormBuilder:open() end


-- =============================================================================
-- arbor.core.* — opt-in pipeline op catalog (require only what you need)
--
-- Every module exposes a set of ready-to-use LuaOp handlers with the standard
-- contract `function(params, ctx) -> { exit_code, stdout, stderr? }`. Call
-- `.register()` to expose them under their bare names so pipeline StepDefs can
-- refer to them as `lua_op = { op = "<name>", params = ... }` without a
-- `plugin` field.
--
-- Picking one module only (e.g. `require("arbor.core.assert")`) avoids loading
-- the handlers you don't need — no wasted closures per plugin VM.
-- =============================================================================

---Shared op-result shape used by every arbor.core.* handler.
---@class arbor.CoreOpResult
---@field exit_code integer           0 = success, non-zero = step Failed
---@field stdout    string|nil        Captured log lines (joined with "\n")
---@field stderr    string|nil        Optional short error message

---Op ctx passed to every handler by the pipeline orchestrator.
---@class arbor.CoreOpCtx
---@field cwd    string               Resolved working directory for this step
---@field plugin string                Plugin that owns the pipeline

-- arbor.core.file and arbor.core.content used to live here. They were trivial
-- wrappers over arbor.fs / arbor.text so the SDK now stops at those low-level
-- APIs. Plugins that need the same op set keep a plugin-local copy (see
-- plugins/source-export/pipeline_ops/ for the canonical reference).

---@class arbor.CoreEdit
local CoreEdit = {}
---Set `value` at the JSONPath `jpath` (dotted path accepted too).
---`value` is auto-promoted: `"42"` becomes `42`, `"[1,2]"` becomes an array.
---@param params { path:string, jpath:string, value:any }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreEdit.json_edit(params, ctx) end
---@param params { path:string, ypath:string, value:any }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreEdit.yaml_edit(params, ctx) end
---@param params { path:string, tpath:string, value:any }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreEdit.toml_edit(params, ctx) end
---`value` is always stringified (XML text/attributes are opaque strings).
---@param params { path:string, xpath:string, value:any }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreEdit.xml_edit(params, ctx) end
function CoreEdit.register() end

---@class arbor.CoreAssert
local CoreAssert = {}
---Pass when the path exists (or when `negate=true` and it does NOT exist).
---@param params { path:string, negate:boolean|nil }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreAssert.assert_file_exists(params, ctx) end
---Default: fail when the regex `pattern` is found. `negate=true` flips
---semantics to "fail when NOT found".
---@param params { path:string, pattern:string, negate:boolean|nil }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreAssert.assert_file_not_contains(params, ctx) end
---Recursive glob over ctx.cwd (basename match). Validates the hit count
---against `[min, max]` — both optional; defaults min=1.
---@param params { glob:string, min:integer|nil, max:integer|nil }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreAssert.assert_glob_matches(params, ctx) end
---Parse the current version out of pom.xml / package.json / Cargo.toml and
---compare (semver-style, pre-release tags ignored) with `new_version`.
---@param params { file:string, new_version:string }
---@param ctx    arbor.CoreOpCtx
---@return arbor.CoreOpResult
function CoreAssert.assert_version_bump(params, ctx) end
function CoreAssert.register() end


---@class arbor.FormWizardStep
---@field id          string
---@field label       string
---@field description string|nil
---@field icon        string|nil    Lucide icon name for the step badge
---@field children    table[]

---@class arbor.FormNodeWizard : arbor.FormNodeBase
---@field type        "wizard"
---@field steps       arbor.FormWizardStep[]
---@field start_step  string|nil    Initial step id
---@field next_label  string|nil    Default: "Next"
---@field back_label  string|nil    Default: "Back"

---Single-line text input — backs `text` / `password` / `email` / `url`. Wraps
---the shared `<Input>` widget, so `icon` / `icon_end` / `prefix` / `suffix`
---/ `size` / `clearable` mirror its affordances. Live-edit dispatch via
---`actions.change` (debounced by `debounce_ms`, default 250 ms).
---@class arbor.FormFieldText : arbor.FormNodeBase
---@field type         "text"|"password"|"email"|"url"
---@field name         string
---@field label        string|nil
---@field default      string|nil
---@field placeholder  string|nil
---@field pattern      string|nil    Regex / Lua pattern for inline validation
---@field pattern_hint string|nil
---@field size         "sm"|"md"|"lg"|nil   Padding tier. Default "md".
---@field icon         string|nil    Leading Lucide icon name (rendered inside the input chrome)
---@field icon_end     string|nil    Trailing Lucide icon name. The clear-× takes precedence when `clearable` is set and the field has a value.
---@field prefix       string|nil    Leading text affix (e.g. "$", "https://", "@") — muted, non-editable
---@field suffix       string|nil    Trailing text affix (e.g. "kg", "%", ".com") — muted, non-editable
---@field clearable    boolean|nil   Show a × button while the field has a value. Default false.
---@field actions      { change: string|arbor.DispatchTarget|nil }|nil
---@field debounce_ms  integer|nil   Trailing-edge debounce for `actions.change`. Default 250 ms; 0 fires on every keystroke.
---@field required     boolean|nil
---@field readonly     boolean|nil
---@field hint         string|nil

---Numeric input with a built-in stepper column — wraps the shared
---`<NumberStepper>` widget. Same `icon` / `icon_end` / `prefix` / `suffix`
---/ `size` surface as `text`. Live-edit dispatch via `actions.change`.
---@class arbor.FormFieldNumber : arbor.FormNodeBase
---@field type         "number"
---@field name         string
---@field label        string|nil
---@field default      number|nil
---@field placeholder  string|nil
---@field min          number|nil
---@field max          number|nil
---@field step         number|nil
---@field size         "sm"|"md"|"lg"|nil   Padding tier. Default "md".
---@field icon         string|nil    Leading Lucide icon name
---@field icon_end     string|nil    Trailing Lucide icon name (between the digits and the stepper column)
---@field prefix       string|nil    Leading text affix (e.g. "$")
---@field suffix       string|nil    Trailing text affix (e.g. "kg", "%", "ms")
---@field actions      { change: string|arbor.DispatchTarget|nil }|nil
---@field debounce_ms  integer|nil   Default 250 ms.
---@field required     boolean|nil
---@field readonly     boolean|nil
---@field hint         string|nil

---Single-value picker. Renders as a dropdown trigger with a chevron; opens
---a menu of `options` (auto-searchable past 12 entries). Each option is a
---bare string, an `arbor.SelectOption`, or one of the richer shapes
---(group / separator / item with icon+description+meta) accepted by the
---host. Live-edit dispatch via `actions.change` — fires on every pick
---(scoped `DispatchTarget` ships only `{ node_id, slot:'change', value,
---state? }`; bare-string action keeps the legacy whole-form payload).
---@class arbor.FormFieldSelect : arbor.FormNodeBase
---@field type          "select"
---@field name          string
---@field options       arbor.FormOptionInput[]
---@field label         string|nil
---@field default       string|nil
---@field placeholder   string|nil    Trigger placeholder when nothing is selected. Default "— select —".
---@field searchable    boolean|nil   Force the filter input on/off. Default: auto-on past 12 options.
---@field empty_message string|nil    Shown when filter yields no matches. Default "No options".
---@field clearable     boolean|nil   Show a × button inside the trigger when a value is selected; clicking it resets the field to "" and fires `actions.change`. Default false.
---@field actions       { change: string|arbor.DispatchTarget|nil }|nil
---@field required      boolean|nil
---@field readonly      boolean|nil
---@field hint          string|nil

---Multi-value variant of `select`. Stored as `string[]`. Each option in the
---menu carries a checkbox; the trigger shows the count once more than one
---entry is picked. Same `searchable` / `empty_message` defaults as `select`.
---@class arbor.FormFieldMultiselect : arbor.FormNodeBase
---@field type          "multiselect"
---@field name          string
---@field options       arbor.FormOptionInput[]
---@field label         string|nil
---@field default       string[]|nil
---@field placeholder   string|nil    Trigger placeholder when the list is empty. Default "— select —".
---@field searchable    boolean|nil   Force the filter input on/off. Default: auto-on past 12 options.
---@field empty_message string|nil    Shown when filter yields no matches.
---@field min           integer|nil   Min selected count (validation).
---@field max           integer|nil   Max selected count (validation).
---@field clearable     boolean|nil   Show a × button inside the trigger when at least one option is selected; clicking it resets the field to []. Default false.
---@field required      boolean|nil
---@field readonly      boolean|nil
---@field hint          string|nil

---Click-to-edit single-line field. Renders the current value as a clickable
---label; activating it (click / Enter / Space) swaps in the host's
---<InlineEdit> widget — Enter commits, Esc reverts, the explicit ✓ / ✕
---buttons mirror those keys. No blur-commit semantics; dismissing focus
---reverts the in-progress draft. Use this for header titles, row names, or
---anywhere a text input would be too noisy.
---@class arbor.FormFieldInlineEdit : arbor.FormNodeBase
---@field type                "inline_edit"
---@field name                string
---@field label               string|nil
---@field default             string|nil
---@field placeholder         string|nil    Placeholder inside the editing input
---@field display_placeholder string|nil    Text shown when value is empty in display mode (default: "—")
---@field size                "sm"|"md"|nil Default: "sm"
---@field maxlength           integer|nil
---@field require_value       boolean|nil   Block commit when empty (default true)
---@field readonly            boolean|nil
---@field required            boolean|nil
---@field hint                string|nil

---Value-bearing git branch picker — same chrome as the host's
---`<BranchSelect>` widget (monospace dropdown trigger, search input above
---the menu past `search_threshold`, sticky entry for a value not in the
---list). Submitted as the picked branch name. The plugin owns the
---branches list; pass it explicitly via `branches`. Typical use: call
---`arbor.repo.branches()` (requires `git = "read"`) on form open and map
---`.name`. The host does not auto-load or watch the active repo's branch
---list; push it back with `arbor.ui.form.patch` when it changes.
---@class arbor.FormFieldBranchSelect : arbor.FormNodeBase
---@field type              "branch_select"
---@field name              string
---@field branches          string[]      Available branches.
---@field label             string|nil
---@field default           string|nil
---@field placeholder       string|nil    Trigger placeholder when nothing is picked. Default "— pick a branch —".
---@field loading           boolean|nil   Render the trigger as a loading shell (label "Loading…", disabled).
---@field search_threshold  integer|nil   Show a search input above the menu once `#branches` exceeds this. Default 12.
---@field required          boolean|nil
---@field readonly          boolean|nil
---@field hint              string|nil

---@class arbor.FormFieldFile : arbor.FormNodeBase
---@field type        "file"
---@field name        string
---@field label       string|nil
---@field pick_mode   "file"|"folder"|"save"|nil   Default: "file"
---@field extensions  string[]|nil                 File extension filter (no dot), e.g. { "json", "yaml" }
---@field placeholder string|nil
---@field default     string|nil
---@field required    boolean|nil
---@field readonly    boolean|nil

---@class arbor.FormFieldAutocomplete : arbor.FormNodeBase
---@field type          "autocomplete"
---@field name          string
---@field id            string                         REQUIRED — dispatch id for set_autocomplete_options
---@field label         string|nil
---@field placeholder   string|nil
---@field default       string|nil
---@field options       arbor.FormOptionInput[]|nil    Static fallback when no source_action
---@field source_action string|nil                     Plugin action fired with { id, query, state }
---@field free_form     boolean|nil                    Allow values not in the options list (default: true)
---@field debounce_ms   integer|nil                    Debounce for source_action (default: 150)

---@class arbor.FormFieldTags : arbor.FormNodeBase
---@field type        "tags"
---@field name        string
---@field label       string|nil
---@field placeholder string|nil
---@field default     string[]|nil
---@field suggestions string[]|nil      When set, acts as an allowlist (multi-select)
---@field max         integer|nil

---One diagnostic / lint marker for an `editor` FormNode. Address a range
---either with document offsets (`from`/`to`, UTF-16 code units, CodeMirror
---native) or with a 1-based `line` for a whole-line marker. Out-of-range
---positions are clamped to the document; entries with no addressable range
---are silently dropped.
---@class arbor.FormEditorDiagnostic
---@field from     integer|nil   Document offset of the marker start
---@field to       integer|nil   Document offset of the marker end (defaults to `from`)
---@field line     integer|nil   1-based line — used when `from`/`to` are absent
---@field severity "error"|"warning"|"info"|"hint"
---@field message  string
---@field source   string|nil    Short identifier of the producer (shown in the tooltip)

---One static completion item supplied by the plugin.
---@class arbor.FormEditorCompletion
---@field label   string
---@field detail  string|nil    Short detail shown next to the label (e.g. "keyword")
---@field info    string|nil    Longer description shown in a side panel
---@field type    string|nil    CodeMirror type → icon ("keyword"|"variable"|"function"|"class"|"constant"|"property"|"method"|"enum"|"interface"|"text"|"type")
---@field apply   string|nil    Text to insert when picked (defaults to `label`)
---@field boost   number|nil    Score boost (positive = higher in the list)

---One snippet template — uses CodeMirror's `${1:placeholder}` syntax for
---tab stops. Picking the snippet expands into the editor with the cursor
---at the first tab stop.
---@class arbor.FormEditorSnippet
---@field label    string
---@field template string         Snippet body, e.g. "for (${1:i} = 0; ${1:i} < ${2:n}; ${1:i}++) ${3}"
---@field detail   string|nil
---@field info     string|nil
---@field type     string|nil
---@field boost    number|nil

---@class arbor.FormFieldEditor : arbor.FormNodeBase
---Multi-line code/text editor (CodeMirror 6). Value-bearing: the document is
---submitted as the field value and can be pushed from the host with
---`arbor.ui.form.set_value(name, text)`. On top of the whole-form model it can
---emit SCOPED events on the high-frequency channel — `on_edit` (debounced,
---value = full text) and `on_select` (value = `{ from, to, text }`). Both
---slots accept a bare action string or an `arbor.DispatchTarget` (so an edit /
---selection can drive a command); `scope_state` rides a slice of form state.
---
---Plugins can drive the editor's diagnostics, completions and snippets:
---  · `diagnostics`  — gutter markers + squiggles + hover tooltip. Patch the
---                     array live with `arbor.ui.form.patch{…}` to re-render.
---  · `completions`  — static items merged into the autocomplete popup
---                     (Ctrl-Space, or auto-fires while typing identifier chars).
---  · `snippets`     — static snippet templates (CodeMirror `${1:name}` style
---                     placeholders) merged into the autocomplete popup.
---@field type         "editor"
---@field name         string
---@field label        string|nil
---@field default      string|nil                       Initial document
---@field language      string|nil                       "json"|"toml"|"yaml"|"ron"|"properties"|"plain" (unknown → plain)
---@field height        integer|string|nil               Editor box height (px number or CSS length). Default 240
---@field line_numbers  boolean|nil                      Show the gutter (default true)
---@field active_line   boolean|nil                      Highlight the active line (default true)
---@field readonly      boolean|nil
---@field diagnostics   arbor.FormEditorDiagnostic[]|nil Lint markers driven by the plugin (patchable live)
---@field lint_gutter   boolean|nil                      Force the lint gutter on/off (default: on when `diagnostics` is non-empty)
---@field completions   arbor.FormEditorCompletion[]|nil Static completion items merged into the autocomplete popup
---@field snippets      arbor.FormEditorSnippet[]|nil    Static snippet templates (CodeMirror `${1:name}` placeholders)
---@field on_edit       string|arbor.DispatchTarget|nil  Debounced scoped slot, slot "edit", value = full text
---@field debounce_ms   integer|nil                      Debounce for on_edit (default 300)
---@field on_select     string|arbor.DispatchTarget|nil  Scoped slot, slot "select", value = { from, to, text }
---@field scope_state   string[]|nil                     liveState keys to include in the scoped payload

---@class arbor.FormDiffLine
---@field kind       "context"|"added"|"removed"
---@field content    string
---@field old_lineno integer|nil   Explicit old-side line number (auto-counted from the hunk start when omitted)
---@field new_lineno integer|nil   Explicit new-side line number (auto-counted from the hunk start when omitted)

---@class arbor.FormDiffHunk
---@field header    string|nil               Synthesised "@@ … @@" when omitted
---@field old_start integer|nil              First old-side line number (default 1)
---@field new_start integer|nil              First new-side line number (default 1)
---@field lines     arbor.FormDiffLine[]

---@class arbor.FormNodeDiff : arbor.FormNodeBase
---Read-only diff viewer. DISPLAY-ONLY (not value-bearing): the node carries
---pre-diffed hunks supplied by the plugin and reuses the app's own diff
---renderer — unified + split layouts, Prism syntax highlight, and
---virtualization for large diffs. Give it a stable `id` to swap `hunks` live
---via `arbor.ui.form.patch{ { id = "...", merge = { hunks = {…} } } }`.
---@field type             "diff"
---@field hunks            arbor.FormDiffHunk[]
---@field label            string|nil
---@field hint             string|nil
---@field path             string|nil   Filename used to pick the highlight grammar; shown in the header
---@field old_path         string|nil   Previous path when renamed (shown as "old → new")
---@field language         string|nil   Override the highlight grammar ("rust"|"json"|…); wins over `path`
---@field mode             "unified"|"split"|nil   Initial layout (default "unified")
---@field hide_mode_toggle boolean|nil  Hide the local unified/split toggle (default false)
---@field word_wrap        boolean|nil  Wrap long lines, unified only (default false)
---@field height           integer|string|nil      Viewer height — px number or CSS length (default "320px")
---@field empty_text       string|nil   Shown when there are no hunks (default "No changes")
---@field virtualize_threshold integer|nil  Total-line count above which the virtualized renderer kicks in (default 600)

---One item in a tree row's right-click context menu. Render order matches
---the array order; mix interactive items with `separator` / `header` rows.
---Each item carries its own `action` / `dispatch`; falls back to the
---tree-level `on_context_menu` slot when neither is set.
---@class arbor.FormTreeMenuItem
---@field id        string|nil                              Stable id (surfaced as `item_id` in the payload)
---@field label     string|nil                              Omit (with `separator = true`) to render a divider
---@field icon      string|nil                              Lucide icon name (curated subset)
---@field action    string|nil                              Sugar for `dispatch = { kind = "action", name = action }`
---@field dispatch  arbor.DispatchTarget|nil                Explicit dispatch target — wins over `action`
---@field danger    boolean|nil                             Render in destructive (red) styling
---@field disabled  boolean|nil                             Render disabled (no hover, no click)
---@field separator boolean|nil                             Render as a divider line
---@field header    boolean|nil                             Non-clickable bold section header

---@class arbor.FormTreeNode
---@field value       string
---@field label       string
---@field children    arbor.FormTreeNode[]|nil
---@field group       boolean|nil   Non-selectable header (still expandable). Click toggles expansion.
---@field icon        string|nil    Lucide icon name shown before the label
---@field icon_color  string|nil    Explicit CSS colour for the row icon — tint group headers per-category so a deep tree isn't monochrome
---@field tag         string|nil    Small pill badge after the label (e.g. "Tomcat")
---@field tag_variant "neutral"|"ok"|"warn"|"error"|"accent"|"dev"|"prod"|"test"|nil
---@field description string|nil    Dim caption under the label
---@field id          string|nil    Stable id — required to patch this row (lazy children, inline updates)
---@field has_children boolean|nil  Advertise (lazy) children before they load: shows an expander, fires on_expand on first open
---@field loading     boolean|nil   Show a spinner on this row (clear it with the patch that fills the children)
---@field draggable   boolean|nil   Per-row override; defaults to true when tree's `reorderable` is on and the row isn't a group
---@field drop_target boolean|nil   Per-row override; defaults to true when tree's `reorderable` is on
---@field menu_items  arbor.FormTreeMenuItem[]|nil   Per-row context menu — wins over the tree-level `menu_items` when set
---@field value_display string|nil  Right-aligned leaf display value (dim monospace, before the pill) — the "key: value" source-tree look. Distinct from `value` (the selection key).
---@field value_tone  string|nil    Colour tone for `value_display`: number / string / enum / bool / entity / handle / accent / warn / muted
---@field pill        string|nil    Type pill rendered after the value, kind-coloured via the shared `TypePill` (richer than the flat `tag`)
---@field pill_kind   string|nil    Colour bucket for `pill` (defaults to `pill`)
---@field pill_tone   "accent"|"info"|"success"|"warning"|"error"|"muted"|nil  Explicit semantic tone for `pill` (wins over `pill_kind`) — use for provenance/state badges that aren't a value-kind
---@field edit_node   arbor.FormNode|nil  Inline editor for a leaf. Activating the row swaps its value cell for this field node (rendered through the normal dispatcher — text / number / select / vec_field / color all work). The editor's own `actions.change` / dispatch fires the mutation; the tree just toggles read ⇄ edit.

---@class arbor.FormFieldTree : arbor.FormNodeBase
---@field type       "tree"
---@field name       string
---@field label      string|nil
---@field nodes      arbor.FormTreeNode[]
---@field multi      boolean|nil     Stored as string[] when true, else string (default: false)
---@field default    string|string[]|nil
---@field expanded   boolean|nil     Expand every node on open (default: false)
---@field bordered   boolean|nil     Legacy bordered look with inner padding + scroll cap (default: false — flush)
---@field max_height string|nil      When bordered, cap via CSS max-height (default: "300px")
---@field change_action string|nil   Plugin action fired on selection change (non-group nodes only). Legacy whole-form payload — prefer the scoped `on_select`. Ctx contains the full form state plus `value` (the newly selected node's value).
---Dynamic "data tree" opt-ins (additive — absent = static tree):
---@field lazy        boolean|nil   Fetch children on expand: a row with has_children but no children fires on_expand + shows a spinner
---@field on_expand   string|arbor.DispatchTarget|nil  Scoped slot fired on (lazy) expand — ships { id, value, path }; respond with a patch that merges the children
---@field on_select   string|arbor.DispatchTarget|nil  Scoped slot fired on selection change — ships the new value (wins over change_action when both set)
---@field on_scroll_range string|arbor.DispatchTarget|nil  Scoped slot fired as the (virtualized) viewport scrolls — ships { start, end, total }
---@field virtualize_threshold integer|nil  Window the rows above this many visible rows (default 400)
---@field row_height  integer|nil   Fixed row height (px) for the virtualized window (default 24)
---@field height      string|integer|nil  Fixed viewport height (px or CSS length); falls back to max_height
---@field fill        boolean|nil   Grow to fill the parent flex column and own the only scroll region (no max_height / fixed height). Use in a flush modal body to avoid a double scrollbar (default: false)
---@field searchable  boolean|nil   Inline filter input at the top of the tree (local UI state; matches label + description, auto-expands ancestors of matches, highlights the substring)
---@field search_placeholder string|nil  Placeholder for the filter input (default "Filter…")
---@field path_query  boolean|nil   Enable JSONPath-style navigation in the search box: a query starting with `$` (e.g. `$.category.crate.Component`) prefix-matches its segments as an ordered subsequence of each node's ancestor labels instead of substring-filtering. Matches are navigable with F3 / Shift+F3 (+ ↑/↓ / Enter from the input), a hit counter shows in the search row, and a results rail opens beside the tree. Requires `searchable`.
---@field reorderable boolean|nil   Enable HTML5 drag-drop reorder among rows. Non-group rows are draggable + drop-targets by default; override per row via `tnode.draggable` / `tnode.drop_target`
---@field on_reorder  string|arbor.DispatchTarget|nil  Scoped slot fired on drop — payload `{ source = { id?, value, path }, target = { id?, value, path }, position = "before"|"inside"|"after" }`
---@field menu_items  arbor.FormTreeMenuItem[]|nil     Default right-click menu items (per-row `tnode.menu_items` wins)
---@field on_context_menu string|arbor.DispatchTarget|nil  Fallback scoped slot fired when a menu item without its own action is picked — payload `{ item_id, value, path }`

---@class arbor.FormTableColumn
---@field key         string
---@field label       string
---@field type        "text"|"number"|"checkbox"|"select"|nil  Default: "text"
---@field options     arbor.FormOptionInput[]|nil              Required for type="select"
---@field placeholder string|nil
---@field width       string|nil                                CSS width (e.g. "120px", "2fr")
---@field readonly    boolean|nil   Render this column as display-only (plain text / checked glyph / select label) even when the rest of the table is editable
---@field align       "left"|"center"|"right"|nil  Cell alignment. Default: "left" (text/select), "center" (checkbox), "right" (number)

---@class arbor.FormTableRowAction
---@field id        string|nil   Stable id surfaced as `action_id` in the dispatched payload (default: positional `__action_<index>`)
---@field icon      string|nil   Lucide icon name (curated subset — see PLUGIN_ICONS)
---@field label     string|nil   Tooltip / aria-label
---@field danger    boolean|nil  Style as destructive (red on hover)
---@field action    string|nil   Legacy slot — sugar for dispatch = { kind = "action", name = action }. Payload: { row_index, row, action_id }
---@field dispatch  arbor.DispatchTarget|nil  Explicit dispatch target (wins over action)
---@field disabled  boolean|nil

---@class arbor.FormFieldTable : arbor.FormNodeBase
---@field type          "table"
---@field name          string
---@field label         string|nil
---@field columns       arbor.FormTableColumn[]
---@field default       table[]|nil     Array of row objects (keys match column.key)
---@field min_rows      integer|nil
---@field max_rows      integer|nil
---@field add_label     string|nil      Default: "Add row"
---@field row_actions   arbor.FormTableRowAction[]|nil  Per-row action buttons rendered in the trailing column, before the built-in trash. Payload: { row_index, row, action_id }
---@field hide_delete   boolean|nil     Drop the built-in row-delete (trash) button (e.g. when a `row_actions` entry takes over the destructive role)
---@field hide_add      boolean|nil     Drop the built-in "+ Add row" button (e.g. when rows are derived from an external source, or row creation goes through a separate plugin action)
---@field sticky_header boolean|nil     Make the header stick to the top of the rows region — keeps column labels visible while scrolling. Pairs naturally with `max_height`
---@field max_height    string|nil      CSS max-height for the rows region (e.g. "260px", "40vh"). The Add button stays anchored below the scroll area

---Section container. With `card = true` renders with dark title bar, border
---and an optional `+` button / counter pill in the title. Use as grouping
---chrome inside `tree_layout` content or sidebar forms.
---@class arbor.FormNodeSection : arbor.FormNodeBase
---@field type        "section"
---@field title       string|nil
---@field description string|nil
---@field children    table[]
---@field collapsible boolean|nil
---@field collapsed   boolean|nil
---@field card        boolean|nil       Dark card chrome
---@field count       integer|nil       Counter pill shown in card title
---@field add_action  string|nil        Plugin action fired when the + button is clicked
---@field variant     string|nil        `"quiet"` (non-card): no border or fill, uppercase muted caption. For a panel that is a stack of several groups in a narrow column, where a box per group reads as competing panes instead of one list.
---@field note        string|nil        Right-aligned muted caption beside the title — what the group is ABOUT (an item count, the struct being edited).

---Two-column label + controls row — use inside a card `section`. The label
---(and optional description) go on the left; `children` (inputs, buttons) on
---the right.
---@class arbor.FormNodeCardRow : arbor.FormNodeBase
---@field type        "card_row"
---@field label       string|nil
---@field description string|nil
---@field children    table[]

---Responsive card grid container — lays out `children` in an auto-fit grid
---that wraps to multiple rows when the available width is too narrow. Each
---column is at least `min_card` wide (default `"280px"`) and expands to
---fill the row. Children are typically `section variant="component"` cards
---or `info_card`s. Unlike `card_row` (a single horizontal flex row),
---`card_grid` wraps to multiple rows — use it to render dashboard-style
---layouts.
---@class arbor.FormNodeCardGrid : arbor.FormNodeBase
---@field type     "card_grid"
---@field min_card string|nil   Minimum card width before wrapping (e.g. `"280px"`, `"22ch"`). Default `"280px"`.
---@field gap      string|nil   Gap between cards (e.g. `"8px"`). Default `"8px"`.
---@field children table[]

---One row in a `property_grid`. A row is either a leaf (carries `value`) or a
---group (carries `children`, for nested structs / arrays rendered indented).
---@class arbor.PropertyRow
---@field id          string|nil   Stable id — required when editable so the grid can track the open row.
---@field label       string       Field name (left column).
---@field value       string|nil   Pre-formatted display string (e.g. `"[ 4.50, 0.00, -2.25 ]"`, `"82"`, `"Job::Legionary"`). The plugin owns formatting; the grid never interprets the raw value.
---@field value_tone  "number"|"string"|"enum"|"bool"|"entity"|"handle"|"muted"|"warn"|"accent"|nil  Syntax-highlight colour for the value text (code-editor style). Omit for the default primary colour.
---@field pill        string|nil   Type pill rendered right-aligned (e.g. `"u32"`, `"Vec3"`, `"enum"`).
---@field pill_kind   string|nil   Pill colour bucket — defaults to `pill`.
---@field pill_tooltip string|nil
---@field tooltip     string|nil   Tooltip on the value cell (typical use: the full / untruncated value).
---@field muted       boolean|nil  Dim the value (e.g. `None` / null / default).
---@field copyable    boolean|nil  Click-to-copy the value (client-side); shows a copy glyph on row hover.
---@field locked      boolean|nil  Immutable — shows a lock glyph and suppresses editing even with `edit_node`.
---@field children    arbor.PropertyRow[]|nil  Nested rows, rendered indented under this row.
---@field collapsible boolean|nil  Group rows only: render a chevron that folds the children.
---@field open        boolean|nil  Group rows only: initial open state when `collapsible` (default open).
---@field edit_node   table|nil    When present (and not `locked`), the row gains a hover pencil; clicking swaps the value cell for this node rendered inline (a `field` / `vec_field` / `select` / … — all existing editors work unchanged). The node's own `action` / `dispatch` fires the mutation on commit.

---Read-only-first property / reflection grid. Renders a dense
---IntelliJ-inspector-style list of `label → value` rows with right-aligned
---type pills, nested-struct indentation, lock glyphs for immutable fields,
---and optional per-row click-to-edit via `edit_node`. Generic — any plugin
---inspecting structured data (config dumps, JSON, ECS reflection, API
---responses) can use it. The plugin formats the values and supplies the
---editor nodes; the grid owns only the layout and the read-only ⇄ edit
---toggle.
---@class arbor.FormNodePropertyGrid : arbor.FormNodeBase
---@field type  "property_grid"
---@field rows  arbor.PropertyRow[]
---@field empty string|nil   Empty-state text when `rows` is empty. Default `"(no fields)"`.

---@class arbor.CfgListItemTag
---@field text    string
---@field variant "neutral"|"ok"|"warn"|"error"|"accent"|"dev"|"prod"|"test"|nil

---@class arbor.CfgListItem
---@field id            string
---@field label         string
---@field active        boolean|nil               Renders an accent dot
---@field tags          arbor.CfgListItemTag[]|nil
---@field edit_action   string|nil                Fired with `{ id = item.id }` when edit clicked
---@field delete_action string|nil                Fired with `{ id = item.id }` when delete clicked

---Config list — rows with active dot + tags + hover edit/delete buttons.
---@class arbor.FormNodeCfgList : arbor.FormNodeBase
---@field type  "cfg_list"
---@field items arbor.CfgListItem[]

---@class arbor.SuggestItem
---@field name   string
---@field cmd    string|nil
---@field tag    string|nil
---@field action string|nil   Fired with `{ name, cmd }` when "Add configuration" clicked

---2-column grid of suggestion cards with an "Add configuration" link each.
---@class arbor.FormNodeSuggestGrid : arbor.FormNodeBase
---@field type  "suggest_grid"
---@field items arbor.SuggestItem[]

---Dispatch target for an actionable slot. Either a callback to this plugin
---(`{ kind = "action", name = "..." }`) or a registered command
---(`{ kind = "command", id = "...", args? = ... }`). The command id is either
---another plugin's `"<owner>::<id>"` or a host built-in `"arbor:area.verb"`
---(see `arbor.HostCommands`). A bare `action` string on a node is sugar for the
---action form.
---@class arbor.DispatchTarget
---@field kind "action"|"command"
---@field name string|nil    Action name (when kind = "action")
---@field id   string|nil    Command id — "<owner>::<id>" or "arbor:area.verb" (when kind = "command")
---@field args any|nil        Static args passed to the command (when kind = "command")

---Inline action button — fires without submitting the form. With
---`icon_only = true` renders as a compact 26×26 square (useful in toolbars).
---`extra` is merged into the action payload alongside all form values — handy
---for item-specific actions in `cfg_list` / `card_row`. Set `dispatch` to fire
---a registered command instead of this plugin's own handler.
---@class arbor.FormNodeButton : arbor.FormNodeBase
---@field type        "button"
---@field label       string|nil
---@field action      string                    Action name (sugar for dispatch = { kind = "action", name = action })
---@field dispatch    arbor.DispatchTarget|nil  Explicit target — takes precedence over `action`
---@field variant     "default"|"primary"|"danger"|"ghost"|nil
---@field close_after boolean|nil
---@field disabled    boolean|nil
---@field icon        string|nil                Lucide icon name (leading)
---@field icon_end    string|nil                Lucide icon name (trailing — chevron, external-link, …). Suppressed when icon_only
---@field icon_only   boolean|nil               Hide label, render only icon
---@field size        "xs"|"sm"|"md"|"lg"|nil   Visual size. Default "sm" (matches legacy baseline)
---@field block       boolean|nil               Stretch to full container width with centred label
---@field color       string|nil                CSS colour override (hex, var(--…), color-mix(...)). Filled bg for primary; text for ghost/danger
---@field tooltip     string|nil                Hover tooltip (esp. useful when icon_only)
---@field extra       table|nil                 Merged into the action payload

---@class arbor.PipelineEditorStep
---@field id             string
---@field name           string
---@field kind           string            Operation kind (palette entry key)
---@field allow_failure  boolean|nil

---@class arbor.PipelineEditorStage
---@field id             string
---@field name           string
---@field mode           arbor.StageMode|nil
---@field max_parallel   integer|nil
---@field steps          arbor.PipelineEditorStep[]

---@class arbor.PipelineEditorOp
---@field kind    string
---@field label   string
---@field icon    string|nil
---@field summary string|nil

---@class arbor.PipelineEditorCategory
---@field id    string
---@field label string
---@field ops   arbor.PipelineEditorOp[]

---@class arbor.FormNodePipelineEditor : arbor.FormNodeBase
---@field type              "pipeline_editor"
---@field stages            arbor.PipelineEditorStage[]
---@field operations        arbor.PipelineEditorCategory[]
---@field search_query      string|nil
---@field selected_step_id  string|nil
---@field selected_stage_id string|nil
---@field step_detail_form  table[]|nil    Form nodes rendered in the detail pane for the selected step
---@field empty_label       string|nil
---@field actions           table<string,string>  Plugin action names. Recognized keys: add_stage, add_step, select_step, remove_step, duplicate_step, move_step_up, move_step_down, remove_stage, move_stage_up, move_stage_down, edit_stage, search_changed.
---
--- Dedicated 3-column workflow editor (palette · sequence · detail).
--- Use this in a tab when you need a real pipeline-style editor: the built-in
--- component handles selection, hover actions, client-side palette search and
--- the detail form of the selected step. Every structural mutation emits a
--- plugin action via the `actions` map. The `step_detail_form` is rendered
--- through the same form-node pipeline used by the rest of the modal, so
--- text/number/checkbox/kv_list fields inside are collected normally at submit.

---@class arbor.FormNodeTreeLayout : arbor.FormNodeBase
---@field type                  "tree_layout"
---@field nav_children          table[]        Left-panel nodes
---@field content_children      table[]        Right-panel nodes
---@field nav_width             string|number|nil  Width of the nav rail. Without `nav_resizable`: any CSS length ("240px", "20rem", "30%"…). With `nav_resizable`: parsed as pixels ("NNNpx" or a raw number) and used as the initial width when no stored preference exists. Default "240px".
---@field nav_collapsible       boolean|nil    Render a round toggle in the top-right corner to hide the sidebar. Preference persists under `arbor:tree-layout-collapsed:<id>` when the node has an `id`. When collapsed, a 34 px rail with a round reopen button is shown in place of the sidebar. Default false.
---@field nav_collapsed_default boolean|nil    Initial state on first open (overridden by stored preference). Default false.
---@field nav_resizable         boolean|nil    Render a drag handle on the right edge of the nav so the user can resize the sidebar (clamped to `nav_min_width` / `nav_max_width`). Arrow keys nudge by 8 px (Shift = 32 px). Width persists under `arbor:tree-layout-nav-w:<id>` when the node has an `id`. Default false.
---@field nav_min_width         string|number|nil  Minimum width when `nav_resizable` is on. Pixels ("NNNpx" or a raw number). Default 160.
---@field nav_max_width         string|number|nil  Maximum width when `nav_resizable` is on. Pixels ("NNNpx" or a raw number). Default 480.

-- ─── Reusable CSS utility classes exposed by the form renderer ──────────────
-- Apply via the `class` field on any node (most useful on `container`) to get
-- a look consistent with the rest of Arbor without hardcoding `style`.
--
--   pf-panel         Rounded card with border + bg-elevated + 12/14 px padding.
--   pf-panel-sm      Same card, slimmer 8/10 px padding (dense lists).
--   pf-panel-flush   Card frame, no internal padding (caller provides its own).
--   pf-panel-quiet   Card with the canvas (`bg-base`) background for secondary panels.
--   pf-panel-scroll  Caps height and enables vertical scrolling inside the panel.
--   pf-panel-stretch Makes the panel fill the parent flex track (min-height: 0).
--   pf-cat-heading   Category caption (uppercase, tight, muted) used in palettes.
--   pf-op-tile       Left-aligned ghost button tile used in operation palettes.
--
-- Example:
--   { type = "container", class = "pf-panel pf-panel-scroll pf-panel-stretch",
--     children = { … } }

---One entry inside a `menu_button` dropdown. Omit `label` + `action` — or set
---`separator = true` — to render a horizontal rule. Set `heading = true` to
---render a bold non-clickable section label.
---@class arbor.FormMenuOption
---@field label     string|nil
---@field icon      string|nil         Lucide icon name
---@field action    string|nil         Plugin action fired when selected
---@field extra     table|nil          Merged into the action payload
---@field variant   "default"|"danger"|nil
---@field disabled  boolean|nil
---@field heading   boolean|nil
---@field separator boolean|nil

---Button that opens a dropdown menu on click. With `icon_only = true` the
---chevron is hidden by default (cleaner toolbar look); set `show_chevron = true`
---to force it.
---@class arbor.FormNodeMenuButton : arbor.FormNodeBase
---@field type         "menu_button"
---@field label        string|nil
---@field icon         string|nil             Lucide icon name
---@field tooltip      string|nil
---@field variant      "default"|"primary"|"danger"|"ghost"|nil
---@field disabled     boolean|nil
---@field icon_only    boolean|nil            Hide label, render only icon (+ chevron)
---@field show_chevron boolean|nil            Default: true unless icon_only is true
---@field options      arbor.FormMenuOption[]


-- =============================================================================
-- Display-only widgets (no value, no submit). Patch them live via
-- `arbor.ui.form.patch{ { id = "...", merge = { ... } } }`.
-- =============================================================================

---One segment of a `breadcrumb` node. Click on an `interactive` segment fires
---the node's `action` with `{ value, index, label }` merged into the payload.
---@class arbor.FormBreadcrumbSegment
---@field label       string
---@field icon        string|nil    Lucide name, emoji, or "plugin:<plugin>:<icon_id>"
---@field badge       string|nil    Small chip rendered after the label (e.g. "current")
---@field tooltip     string|nil
---@field interactive boolean|nil   Default true. When false the segment is dim and not clickable.
---@field value       string|number|nil  Opaque value echoed back to the action.

---Horizontal trail of chip-style segments. Useful as a path indicator in
---plugin views / studio-like modals. With `editable = true` a pencil button
---and a "type a path" mode are enabled.
---@class arbor.FormNodeBreadcrumb : arbor.FormNodeBase
---@field type             "breadcrumb"
---@field segments         arbor.FormBreadcrumbSegment[]
---@field max              integer|nil           Soft cap on visible segments (default 6). Middle collapses to "…".
---@field action           string|nil            Fired on segment click; ctx merges `{ value, index, label }`.
---@field editable         boolean|nil
---@field edit_value       string|nil            Prefilled string in edit mode.
---@field edit_placeholder string|nil
---@field commit_action    string|nil            Fired when the user submits the edited path; ctx contains `{ path }`.

---Monospace readable display for a URL or any opaque identifier. Never
---truncates with ellipsis — the user is expected to read it verbatim.
---@class arbor.FormNodeUrlBlock : arbor.FormNodeBase
---@field type     "url_block"
---@field value    string
---@field label    string|nil
---@field copyable boolean|nil    When true, renders a copy-to-clipboard button on the right.

---1-2 letter monogram tile used to brand workspaces / projects / plugins.
---For person identity use the `avatar` node (separate). When `initials` is
---omitted the renderer derives them from `name`.
---@class arbor.FormNodeMonogram : arbor.FormNodeBase
---@field type     "monogram"
---@field name     string                                            Tooltip source + initials derivation
---@field initials string|nil                                        Override the auto-derived initials
---@field color    string|nil                                        Any CSS color or `var(--…)`; default `var(--accent)`
---@field size     integer|nil                                       Pixel size of the shorter edge (12-48, default 18)
---@field variant  "square"|"circle"|"outline"|"dot"|nil             Default "square"
---@field disabled boolean|nil                                       Greyed-out look
---@field fg       string|nil                                        Foreground override (square/circle/outline)
---@field tooltip  string|nil                                        Tooltip override; falls back to `name`

---Centered block-level status message for a content pane. Tones drive the
---default icon: error → AlertCircle, success → CheckCircle2, info → Info,
---loading → built-in Spinner. Override with `icon` (Lucide name).
---@class arbor.FormNodeStateBlock : arbor.FormNodeBase
---@field type    "state_block"
---@field tone    "loading"|"error"|"success"|"info"|"neutral"|nil   Default "neutral"
---@field label   string|nil
---@field spinner boolean|nil    Only honoured when tone == "loading". Default true.
---@field icon    string|nil    Override the tone icon (Lucide name).
---@field fill    boolean|nil    Stretch to fill the parent (default true).

---@class arbor.FormStepIndicatorStep
---@field id    string
---@field label string
---@field icon  string|nil   Lucide icon name shown in pending/active state

---Wizard-style step navigation breadcrumb. PURE VISUAL — distinct from the
---`wizard` container node which routes between child trees. Use this when
---you own the step navigation yourself and just want the indicator.
---@class arbor.FormNodeStepIndicator : arbor.FormNodeBase
---@field type            "step_indicator"
---@field steps           arbor.FormStepIndicatorStep[]
---@field current         string                              Id of the active step
---@field layout          "horizontal"|"vertical"|nil         Default "horizontal"
---@field size            "sm"|"md"|nil                       Default "md"
---@field variant         "flat"|"pill"|nil                   Default "flat"
---@field separator       boolean|nil                         Default: true for flat, false for pill
---@field collapse_labels boolean|nil                         Collapse to badge-only below 768px viewport
---@field action          string|nil                          Fired with `{ id, index }` on step click (done + active only)

---@class arbor.FormStatusListChip
---@field severity "block"|"warn"|"info"|"success"
---@field text     string
---@field icon     string|nil   Lucide icon name shown before the text

---@class arbor.FormStatusListItem
---@field id    string
---@field label string
---@field chips arbor.FormStatusListChip[]

---Itemised "preview before bulk action" panel — header with summary pills,
---scrollable body of rows, optional footnote. Display-only; recompute
---`items` and patch the node when state changes.
---@class arbor.FormNodeStatusList : arbor.FormNodeBase
---@field type            "status_list"
---@field items           arbor.FormStatusListItem[]
---@field total_count     integer|nil                         "N of M" header denominator
---@field scanning        boolean|nil                         Show a "scanning…" header with a spinner
---@field scanning_label  string|nil                          Default "Scanning…"
---@field clean_label     string|nil                          Override the all-clean header message
---@field noun            { singular: string, plural: string }|nil  Default { singular = "item", plural = "items" }
---@field footnote        string|nil
---@field max_list_height integer|nil                         Pixel cap on the scrolling body (default 160)

---App-message banner OR in-document callout — picked by `style`. `banner`
---(default) renders the full-width tinted block (`Alert.svelte`) you use for
---transient app messages; `inline` renders the in-document callout
---(`Callout.svelte`) you embed in body copy / docs. When style is `inline`,
---`variant = "error"` maps to the danger styling, `variant = "success"` maps
---to the tip styling.
---
---`title` renders bold above `text` and survives `collapsible` collapse so
---the user always has something to click on. `dismissable = true` adds an ×
---button (local-only — the alert is removed from the DOM, no plugin
---round-trip; patch the node back in to restore). `collapsible = true` adds
---a chevron toggle that hides the body text; pair with `collapsed = true`
---to start collapsed.
---@class arbor.FormNodeAlert : arbor.FormNodeBase
---@field type        "alert"
---@field title       string|nil
---@field text        string
---@field variant     "info"|"warning"|"error"|"success"|nil   Default "info"
---@field style       "banner"|"inline"|nil                    Default "banner"
---@field dismissable boolean|nil   Default false. Adds an × button; local-only hide.
---@field collapsible boolean|nil   Default false. Adds a chevron that hides `text`.
---@field collapsed   boolean|nil   Default false. Initial collapse state.

---Vertical labeled wrapper around `children`. Same chrome the host uses on
---built-in form fields: label on top, content below, optional description /
---hint / error / leading icon / right-aligned actions on the label row.
---Useful around non-field content (button, copy_link), to enrich a single
---field with affordances the type doesn't expose (icon, action button next
---to the label), or to surface a computed error/hint that doesn't come from
---per-field validation.
---@class arbor.FormNodeFormField : arbor.FormNodeBase
---@field type          "form_field"
---@field label         string|nil    Omit (together with `icon`/`actions`) to render without the label row.
---@field optional_text string|nil    Small muted text after the label (e.g. "(optional)").
---@field required      boolean|nil   Show a red asterisk after the label.
---@field description   string|nil    Description shown between label and content.
---@field hint          string|nil    Hint shown below the content, muted.
---@field error         string|nil    Error shown below the content (replaces hint when set).
---@field icon          string|nil    Lucide icon name shown before the label text.
---@field actions       table[]|nil   Right-aligned action nodes on the same row as the label (typically `button` nodes).
---@field children      table[]
---@field for           string|nil    htmlFor target on the underlying <label>.

---@alias arbor.InfoCardBadgeKind "info"|"success"|"warning"|"error"|"accent"|"muted"

---@class arbor.InfoCardBadge
---@field text string
---@field kind arbor.InfoCardBadgeKind|nil

---@class arbor.InfoCardMeta
---@field label   string|nil   ALL-CAPS dim label shown before the value.
---@field value   string       Mono-styled value.
---@field tooltip string|nil   Tooltip — typical use is showing the full type path when value is shortened.

---@class arbor.InfoCardAction
---@field icon    string         Lucide icon name.
---@field label   string|nil
---@field tooltip string|nil
---@field variant "default"|"primary"|"danger"|nil
---@field disabled boolean|nil
---@field action  string         Plugin action fired on click.
---@field extra   table|nil      Extra data merged into the action payload.

---Hero header card. Use as the FIRST node of a tab body, panel section or
---modal to anchor "what am I looking at" context — title, status pill,
---type badges, key:value meta pills, and a row of action icons.
---@class arbor.FormNodeInfoCard : arbor.FormNodeBase
---@field type      "info_card"
---@field title     string
---@field subtitle  string|nil
---@field icon      string|nil     Lucide icon name (mutually exclusive with `monogram`).
---@field monogram  string|nil     1-2 letter tile (mutually exclusive with `icon`).
---@field accent    string|nil     Avatar accent override; defaults to `var(--accent)`.
---@field status    { text: string, kind: arbor.InfoCardBadgeKind|nil }|nil   Right-aligned status pill next to the title.
---@field badges    arbor.InfoCardBadge[]|nil
---@field meta      arbor.InfoCardMeta[]|nil
---@field actions   arbor.InfoCardAction[]|nil
---@field variant   ("elevated"|"flat"|"subtle")|nil   Card chrome tone. Default `"elevated"`; use `"flat"` when nesting inside another elevated surface.
---@field bordered  boolean|nil   Show the 1px border. Default `true`.

---@alias arbor.ChipTone "accent"|"info"|"success"|"warning"|"error"|"muted"|"neutral"

---@class arbor.ChipItem
---@field id       string
---@field label    string
---@field count    integer|nil
---@field tone     arbor.ChipTone|nil
---@field icon     string|nil   Lucide icon name.
---@field tooltip  string|nil
---@field disabled boolean|nil

---Horizontal pill selector. The current selection is exposed as a regular
---form value (so it can be read in submit and echoed back through
---`liveState`). In multi mode the value is `string[]`, otherwise a single
---`string`. Typical use: filter row above a list of `section` cards gated
---with `show_if = { field, value }` so flipping a chip narrows the visible
---cards without a round-trip.
---@class arbor.FormNodeChipBar : arbor.FormNodeBase
---@field type     "chip_bar"
---@field name     string                       Selection stored at `values[name]`.
---@field default  string|string[]|nil          Default-selected id(s).
---@field multi    boolean|nil
---@field size     "sm"|"md"|nil                Default "md".
---@field tint_inactive boolean|nil             Tint inactive chips by their `tone` too (coloured text + border), so the bar reads like a legend before selection. Default false (neutral until selected).
---@field action   string|nil                   Fired with `{ name, value }` on selection change (useful when no parent uses `show_if`).
---@field items    arbor.ChipItem[]

---Standalone click-to-copy button with chrome (border, hover). Distinct
---from `copy_link` — `copy_link` is a subtle inline pseudo-link with a
---glyph; `copy_button` is a real action button (icon square or icon +
---label). Copy happens client-side via the browser clipboard API.
---@class arbor.FormNodeCopyButton : arbor.FormNodeBase
---@field type             "copy_button"
---@field value            string                The exact string copied on click.
---@field variant          "icon"|"inline"|nil   Default "icon" (square 22×22). "inline" renders icon + label.
---@field label            string|nil            Inline label (default "Copy"). Ignored when variant is "icon".
---@field copied_label     string|nil            Inline success label (default "Copied").
---@field tooltip          string|nil            Hover tooltip + aria-label (default "Copy to clipboard").
---@field toast_success    string|nil            Toast text on successful copy; omit to suppress.
---@field show_error_toast boolean|nil           Show a generic error toast on copy failure. Default true.

---Small "Experimental" pill — flag features still being shaped. Amber→coral
---gradient with a flask icon. `md` (default) fits modal headers; `sm` fits
---list rows.
---@class arbor.FormNodeExperimentalBadge : arbor.FormNodeBase
---@field type        "experimental_badge"
---@field title       string|nil   Tooltip title. Default "Experimental".
---@field description string|nil   Longer description under the tooltip title.
---@field size        "sm"|"md"|nil   Default "md".
---@field label       string|nil   Override the visible label (default "Experimental").

---Standalone section title bar — headline + optional secondary description,
---without wrapping any children. Distinct from the `section` container which
---owns a body. Use this to anchor a region whose body is laid out by sibling
---nodes (settings page with a free-form layout below the heading).
---@class arbor.FormNodeSectionHeader : arbor.FormNodeBase
---@field type        "section_header"
---@field title       string
---@field description string|nil

---Action-only chip-style filter button. Same pill chrome as the host's
---`<FilterButton>` widget (rounded outline, accent when active, optional
---count badge). Clicking fires `action` — NOT value-bearing, no submit.
---The active look is driven by the `active` flag (or `count > 0`), which
---the plugin flips at runtime via
---`arbor.ui.form.patch({ id = "…", merge = { active = … } })`.
---@class arbor.FormNodeFilterButton : arbor.FormNodeBase
---@field type    "filter_button"
---@field label   string
---@field icon    string|nil   Lucide icon name shown before the label.
---@field count   integer|nil  Badge after the label; > 0 forces the active look unless `active` is set.
---@field active  boolean|nil  Active-state override. Falls back to `count > 0`.
---@field action  string       Plugin action fired on click.
---@field extra   table|nil    Extra data merged into the action payload.

---Bottom-panel title bar — same look as the host's `<BottomPanelHeader>`.
---Standalone header (no body, no footer): icon + uppercase title + count
---badge + inline `children` + right-aligned actions + a mac-style close
---button when `close_action` is set. Distinct from `panel_shell` (full
---wrapper with body); pair this with sibling layout nodes when the host
---owns the body content.
---@class arbor.FormNodeBottomPanelHeader : arbor.FormNodeBase
---@field type         "bottom_panel_header"
---@field title        string|nil
---@field icon         string|nil    Lucide icon name shown before the title.
---@field count        integer|nil   Count badge after the title (visible when > 0).
---@field children     table[]|nil   Inline content placed after the title (status / breadcrumb / tab strip).
---@field actions      table[]|nil   Right-aligned action nodes (typically `button` with `class = "ps-btn"`).
---@field close_action string|nil    When set, renders the close button on the far right; fires this action on click.

---Panel chrome wrapper — same look as the host's `<PanelShell>` widget used
---by every sidebar / main panel. Header (icon + uppercase title + count
---badge + right-aligned actions) on top, optional toolbar row, scrollable
---body, and optional fixed footer. Use inside `arbor.ui.add_view` bodies
---or plugin modals that want IntelliJ-style panel chrome. Display-only
---(child nodes still carry their own values).
---@class arbor.FormNodePanelShell : arbor.FormNodeBase
---@field type        "panel_shell"
---@field title       string
---@field icon        string|nil   Lucide icon name shown to the left of the title.
---@field count       integer|nil  Count badge after the title (visible when > 0).
---@field actions     table[]|nil  Right-aligned action nodes (typically `button` with `class = "ps-btn"`).
---@field toolbar     table[]|nil  Second-row content (search input / filter chips / tab bar / …).
---@field children    table[]      Main body — nodes laid out as in any other container.
---@field footer      table[]|nil  Fixed footer below the scrolling body.
---@field scrollable  boolean|nil  Body scrolls. Default true.
---@field hide_header boolean|nil  Skip the default header (when outer chrome owns the title bar). Default false.
---@field variant     "plain"|"plugin"|nil   Default "plain". `"plugin"` enables the floating-card chrome (elevated header / rounded outer border / `--bg-base` inset body) used by the Plugin Manager and `arbor.ui.add_view` bodies.

---Wraps one or more child nodes with the host's singleton hover/focus
---tooltip (smart placement, viewport-aware flipping, keyboard focus,
---optional shortcut hint, optional Markdown body). Display defaults to
---`"inline"` (a `<span>` with `display: inline-block`) — fits a button,
---monogram, copy_button, icon, or badge. Set `display = "block"` to render
---a `<div>` wrapper instead, required when wrapping a block-level subtree
---(section, panel_shell, info_card, …).
---@class arbor.FormNodeTooltip : arbor.FormNodeBase
---@field type        "tooltip"
---@field children    table[]                      Child node(s) the tooltip attaches to.
---@field content     string                       Primary tooltip text. Required (the wrapper is a no-op when empty).
---@field description string|nil                   Secondary line shown dimmer / smaller under `content`.
---@field shortcut    string|string[]|nil          Keyboard shortcut hint (`"Ctrl+K"` or `{ "Ctrl", "K" }`).
---@field placement   "top"|"bottom"|"left"|"right"|"auto"|nil   Preferred side; auto-flips on viewport collision. Default `"auto"`.
---@field delay       integer|nil                  Hover open-delay in ms. Default 350. Focus opens immediately.
---@field offset      integer|nil                  Distance in px between trigger and tooltip. Default 8.
---@field max_width   integer|nil                  Max width in px. Default 320.
---@field max_height  integer|nil                  Max height in px; longer content fades. Default 280.
---@field markdown    boolean|nil                  Render `content` as sanitised Markdown. Default false.
---@field display     "inline"|"block"|nil         Wrapper element. `"inline"` (default) → `<span>` (inline-block). `"block"` → `<div>`.

---Display-only colour swatch — chip-only or labelled card row. Mirrors the
---host's `<ColorSwatch>` widget used by the Marketplace palette and theme-
---preview surfaces. Distinct from the value-bearing `color` field (HTML5
---colour input) — `color_swatch` is presentational only: the plugin owns
---the `color` value (any CSS expression — hex, `rgb()`, `var(--…)`,
---`color-mix(...)`) and the chip renders accordingly. To make it editable,
---pair it with a sibling `color` field and patch the swatch's `color` from
---the field's `change` action.
---
---When `label` is set the widget renders as a labelled card row
---`[chip] Label   #caption`; when `label` is absent only the chip is
---rendered (use this inside a custom grid where the label lives elsewhere).
---Set `glyph` (a single character like `"#"`, `"n"`, `"T"`) to render a
---centred marker instead of a colour fill — useful when the swatch doubles
---as a typed-token indicator.
---@class arbor.FormNodeColorSwatch : arbor.FormNodeBase
---@field type       "color_swatch"
---@field color      string         Any CSS colour value — hex, `rgb()`, `var(--token)`, `color-mix(...)`, …
---@field label      string|nil     Display name. When set, renders as a labelled card row; when absent, only the chip is rendered.
---@field caption    string|nil     Right-hand caption in labelled mode. Defaults to the raw `color`.
---@field no_caption boolean|nil    Hide the caption in labelled mode.
---@field chip_size  integer|nil    Chip width/height in px. Defaults to 18 (labelled) / 22 (chip-only).
---@field tooltip    string|nil     Tooltip override; defaults to the colour value.
---@field glyph      string|nil     Single-character marker shown instead of the colour fill (e.g. `"#"`, `"n"`, `"T"`).

---Display-only keybinding badge — same chrome as the host's `<Kbd>` used in
---Shortcuts / Command Palette / footer hints. Resolves the visible label
---live from the user's keybindings:
---  · `action`  → built-in or plugin-registered action id (re-renders on
---                rebind in Settings → Keybindings).
---  · `binding` → explicit `{ key, modifiers, scope? }` object.
---  · `keys`    → array of chord parts (`{ "Ctrl", "K" }`).
---  · `label`   → single string `"Ctrl+K"` (split on `+`).
---When `action` / `binding` resolves to nothing the widget renders nothing —
---safe to drop next to a label without a guard.
---@class arbor.FormNodeKbd : arbor.FormNodeBase
---@field type     "kbd"
---@field action   string|nil                          Built-in or plugin-registered action id.
---@field binding  table|nil                           Explicit keybinding object `{ key, ctrl?, shift?, alt? }`. Wins over `keys`/`label`.
---@field label    string|nil                          Single label like `"Ctrl+K"`; split on `+` if `keys` isn't supplied.
---@field keys     string[]|nil                        Explicit chord parts. Wins over `label`.
---@field size     "sm"|"md"|nil                       Badge size. Default `"md"`.
---@field tone     "default"|"accent"|"muted"|nil      Visual tone. Default `"default"`.
---@field variant  "box"|"inline"|nil                  `"box"` (default) → boxed `<kbd>` badges; `"inline"` → plain monospace text (IntelliJ-menu style).

---Display-only uppercase type pill — one-word type hint, same chrome as the
---host's `<TypePill>` used in component cards and field rows. Two ways to
---drive the colour: `kind` picks from a curated palette (vector / numeric /
---bool / enum / handle / entity / option / string / array / struct /
---unknown — case-insensitive), or `tone` for an explicit semantic
---override (`accent`, `info`, `success`, `warning`, `error`, `muted`).
---@class arbor.FormNodeTypePill : arbor.FormNodeBase
---@field type    "type_pill"
---@field label   string|nil                                                   Visible text. When omitted, the resolved `kind` is shown as-is.
---@field kind    string|nil                                                   Curated kind — picks a palette. Case-insensitive; unknown values fall through to neutral / dim.
---@field tone    "accent"|"info"|"success"|"warning"|"error"|"muted"|nil      Explicit tone override. Wins over `kind`.
---@field tooltip string|nil                                                   Tooltip on hover.

---Display-only charset indicator — same chrome as the host's
---`<EncodingPill>` used in the diff toolbar / file-list rows. Small
---monospace pill; warning-tinted when `overridden` is true to surface
---that the user pinned a non-auto value. Presentational only — the plugin
---owns the label and the override flag. Pair with a sibling `select` to
---let the user pick a charset, then patch the pill via
---`arbor.ui.form.patch` to reflect the choice.
---@class arbor.FormNodeEncodingPill : arbor.FormNodeBase
---@field type       "encoding_pill"
---@field encoding   string         Encoding label currently in effect (e.g. `"UTF-8"`, `"windows-1252"`).
---@field overridden boolean|nil    True when the user has pinned a non-auto encoding — drives the warning tint.
---@field compact    boolean|nil    Compact 14px variant for cramped headers. Default false.

---Display-only round avatar — same chrome as the host's `<Avatar>` widget
---used for committer rows / reviewer chips. Initials derived from `name`
---(first letter of the first two words); stable hue derived from `email`
---(preferred) or `name`. Tooltip is `name` + optional `email`. Distinct
---from `monogram`, which is square / outline-styled and meant for entities
---(workspaces, plugins), not people.
---@class arbor.FormNodeAvatar : arbor.FormNodeBase
---@field type  "avatar"
---@field name  string|nil    Display name — also the source of the initials when no other text is supplied.
---@field email string|nil    Email address — preferred hue source; appears in the tooltip description.
---@field size  integer|nil   Avatar diameter in px. Default 24.

---Provider brand identifier shared by `brand_icon` and `brand_tile`.
---@alias arbor.ProviderBrand "github"|"gitlab"|"bitbucket"|"linear"|"jira"

---Display-only monochrome brand glyph — canonical `simple-icons` mark
---rendered in `currentColor`, so it inherits the surrounding text colour.
---Useful when a coloured tile would clash with the rest of the icon set
---(activity bar, inline glyphs, sidebar). For owned-swatch surfaces (auth
---tiles, settings cards) use `brand_tile`.
---@class arbor.FormNodeBrandIcon : arbor.FormNodeBase
---@field type  "brand_icon"
---@field brand arbor.ProviderBrand                       Provider brand to render.
---@field size  integer|nil                               Pixel size of the glyph. Default 20.
---@field title string|nil                                Override the title attribute / tooltip (defaults to the capitalised brand name).

---Display-only branded square tile — composes the canonical `simple-icons`
---mark on the brand's hard-coded background colour (GitHub dark, GitLab
---orange, Bitbucket / Jira blue, Linear indigo) with a fixed bright
---foreground. Brand contrast does NOT borrow theme tokens. Use for auth
---tiles, provider cards, welcome screens; for monochrome marks that
---inherit the surrounding colour use `brand_icon`.
---@class arbor.FormNodeBrandTile : arbor.FormNodeBase
---@field type      "brand_tile"
---@field brand     arbor.ProviderBrand     Provider brand to render.
---@field size      integer|nil             Pixel size of the inner glyph. Default 20.
---@field tile_size integer|nil             Pixel size of the outer square. Defaults to `max(size + 16, 36)`.
---@field disabled  boolean|nil             Greyed-out look — used to indicate disabled / unavailable items.
---@field title     string|nil              Override the title attribute / tooltip (defaults to the capitalised brand name).

---Display-only two-line user identity row — avatar (or initial monogram)
---+ primary name line + optional secondary line (email / @handle / domain).
---When `copyable` is true (default) both lines are click-to-copy with a
---hover affordance and a transient ✓ confirmation. Presentational only —
---the plugin owns the data (typically populated from `arbor.http.*` calls
---against the provider's user API).
---@class arbor.FormNodeProviderUserBadge : arbor.FormNodeBase
---@field type       "provider_user_badge"
---@field name       string         Primary line — typically display name or login.
---@field avatar_url string|nil     Avatar URL; falls back to a circled monogram of the first initial.
---@field secondary  string|nil     Secondary line — email, domain, @handle, …
---@field copyable   boolean|nil    When true (default), clicking the name / secondary copies it to the clipboard.


-- =============================================================================
-- arbor.hooks — built-in hook catalog with ctx schema
-- =============================================================================

---Introspection of the built-in hook catalog. Lets a plugin discover what
---hooks the host fires and what fields each ctx payload carries — without
---consulting external docs.
---
---Action hooks fired via `arbor.events.emit`, `arbor.command.register`, or
---`arbor.job.spawn{on_done=...}` are NOT in the catalog (they're plugin-defined).
---`describe()` returns nil for those.
---@class arbor.Hooks
local Hooks = {}

---List every built-in hook with its full schema. `name` on each entry is the
---fully-qualified `<product>:<event>`. The whole catalog is returned, across
---every product — filter on the namespace half of `name` to narrow it to the
---host this plugin runs under. Useful for generating docs or building runtime
---validators.
---@return arbor.HookDef[]
function Hooks.list() end

---Look up a single built-in hook by name. The `<product>:` prefix is optional
---and resolves against the host product, exactly as in `arbor.events.on`.
---Returns nil for unknown hooks (plugin-defined action hooks, or typos).
---@param  name string  "corvus:commit", or "commit" under Corvus
---@return arbor.HookDef|nil
function Hooks.describe(name) end

---@type arbor.Hooks
arbor.hooks = Hooks
