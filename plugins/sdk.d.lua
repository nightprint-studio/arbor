---@meta
-- =============================================================================
-- Arbor Plugin SDK — EmmyLua type definitions
-- Compatible with lua-language-server (sumneko/LuaLS)
--
-- This file is a pure declaration file (---@meta).
-- It provides autocomplete and type checking for the `arbor` global API
-- injected into every Arbor plugin sandbox.
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
-- Hook context tables  (passed as `ctx` to arbor.on callbacks)
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
---Payload of the `on_pre_commit` hook. Handlers may **veto** the commit
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

---@class arbor.HookField
---@field name        string   Field name in the ctx table
---@field type        string   "string" | "number" | "boolean" | "string[]" | "object"
---@field required    boolean  False if the field is optional / context-dependent
---@field description string

---@class arbor.HookDef
---@field name        string             Hook name (e.g. "on_repo_open")
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

---Read the full contents of a file as a UTF-8 string.
---@param  path string
---@return string|nil content
---@return string|nil err
function Fs.read(path) end

---Write content to a file, creating any missing parent directories.
---Requires `fs = "write"` in plugin.toml.
---@param path    string
---@param content string
function Fs.write(path, content) end

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
---`arbor.service.call` mechanism (which races against startup and can silently
---no-op on host mutex contention). Returns false on unknown names, dormant
---entries, or any lookup failure.
---@param name string  manifest name of the plugin to check
---@return boolean
function Meta.plugin_loaded(name) end

---Return "windows" | "macos" | "linux".
---@return string
function Meta.os() end


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
---sidebar form-DSL renderer.
---@class arbor.UiPanelContent
---@field title   string|nil    Header shown above the body (optional)
---@field nodes   table[]|nil   Form-DSL node tree (list of node tables)
---@field actions table[]|nil   Optional footer buttons: `{label, action}`

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
---  arbor.ui.form.set_options(name, options)   -- refresh select/radio/autocomplete options
---  arbor.ui.form.set_disabled(name, bool)     -- disable/enable a field
---  arbor.ui.form.set_value(name, value)       -- programmatically set a field value
---  arbor.ui.form.replace(cfg)                -- swap the whole node tree in-place
---  arbor.ui.form.set_loading(arg)            -- toggle the busy overlay (cheap, no re-render)
---  arbor.ui.form.close()                     -- programmatically dismiss the modal
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
---Live `actions.change` on `select`: every `select` form-node accepts an
---`actions = { change = "..." }` field. When set, the action fires on every
---selection (not just Submit) with `{ value }` in the payload — handy for
---"window picker" / live-filter controls that should re-fetch immediately.
---
---Builder mode: `arbor.ui.form()` (no arg) or `arbor.ui.form("id")` returns a
---chainable `arbor.FormBuilder`; `:open()` emits the modal via the same path.
---@overload fun(): arbor.FormBuilder
---@overload fun(id: string): arbor.FormBuilder
---@param config table
function Ui.form(config) end

---Replace the option list of a select / radio / autocomplete field in the
---currently-open form of this plugin.
---@param name    string  Field name (matches the node's `name` attribute)
---@param options arbor.FormOptionInput[]
function Ui.form.set_options(name, options) end

---Toggle the disabled state of a field in the currently-open form.
---@param name     string
---@param disabled boolean
function Ui.form.set_disabled(name, disabled) end

---Programmatically set the value of a field in the currently-open form.
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

---Open a confirmation dialog. Returns a Promise that resolves with `true`
---when the user clicks the confirm button and `false` on cancel.
---@param  config arbor.UiConfirmConfig
---@return arbor.Promise
function Ui.confirm(config) end

---Open the native-feeling FilePickerModal and round-trip the chosen path back
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

---Push form-DSL content into a panel registered via `add_sidebar`. Arbor
---re-renders the panel in place and caches the content so subsequent opens
---display immediately while the plugin recomputes. Call this from the
---`panel:open:<id>` hook — or any time the underlying state changes.
---
---Supported node types in the lightweight sidebar renderer:
---`heading`, `label`, `paragraph`, `divider`, `button`, `list`, `section`.
---
---@param id   string                           Panel id (matches `add_sidebar` config)
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
---selection state into the UI on `on_repo_open`).
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
-- the overrides during its `on_plugin_load` handler.
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
--   arbor:diff-toolbar                 arbor.ui.add_toolbar_action(target="diff")           {label?, icon, action, tooltip?}
--   arbor:status-bar:left              arbor.ui.add_toolbar_action(target="status-bar:left")    {label?, icon?, action, tooltip?, color?}
--   arbor:status-bar:right             arbor.ui.add_toolbar_action(target="status-bar:right")   ›
--   arbor:title-bar:left               arbor.ui.add_toolbar_action(target="title-bar:left")     ›
--   arbor:title-bar:right              arbor.ui.add_toolbar_action(target="title-bar:right")    ›
--   arbor:commit-detail:action         arbor.ui.add_toolbar_action(target="commit-detail")  {label, icon?, action, tooltip?}     (ctx: oid)
--   arbor:commit-form:action           arbor.ui.add_toolbar_action(target="commit-form")    {label, icon?, action, tooltip?}     (ctx: staged summary)
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
-- One namespace for both built-in lifecycle hooks (`on_repo_open`, `on_commit`,
-- …) and plugin-defined events. Inter-plugin events are namespaced with the
-- publisher's plugin name to keep cross-plugin contracts explicit:
--
--   -- plugin "compile-action"
--   arbor.events.emit("build-done", { status = "ok", job = "build-42" })
--
--   -- any other plugin (or the same one) subscribes to:
--   arbor.events.on("compile-action:build-done", function(ctx)
--     arbor.log.info("build finished: " .. ctx.status)
--   end)
--
-- `emit` auto-prefixes the event name with this plugin's name when no ':' is
-- present. Publishing under another plugin's namespace (e.g.
-- `arbor.events.emit("other-plugin:event", ...)`) raises a runtime error.
-- Delivery is asynchronous — `emit` returns immediately, subscribers run on a
-- background thread.
-- =============================================================================

---@class arbor.Events
local Events = {}

---Subscribe to a built-in hook (e.g. "on_repo_open") OR to a plugin event
---(e.g. "compile-action:build-done"). The event name may be the exact string
---or a glob pattern containing one or more "*" wildcards. Each "*" matches
---any sequence of characters (including empty strings and ":" separators).
---
---Examples:
---  "on_commit"                  -- built-in lifecycle hook
---  "compile-action:build-done"  -- exact match for a plugin event
---  "compile-action:*"           -- any event from compile-action
---  "*:build-done"               -- any plugin's build-done event
---  "*"                          -- every event fired (debug)
---
---A plugin with at least one wildcard subscription also receives built-in
---lifecycle hooks without needing to declare them in the manifest.
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
---plugins — useful for debugging / discovery. Requires `service_call = true`.
---@return string[]
function Service.list() end


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
---@field terminal     arbor.Terminal
---@field job          arbor.Job
---@field timer        arbor.Timer
---@field scheduler    arbor.Scheduler
---@field ui           arbor.Ui
---@field keybinding   arbor.Keybinding
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
---@field children table[]

---@class arbor.FormNodeTabs : arbor.FormNodeBase
---@field type         "tabs"
---@field tabs         arbor.FormTab[]
---@field default_tab  string|nil   Initial active tab id (defaults to first tab)


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

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:text(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:password(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:textarea(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:number(name_or_cfg, opts) end

---@param  name_or_cfg string|table
---@param  opts        table|nil
---@return arbor.FormBuilder
function FormBuilder:select(name_or_cfg, opts) end

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

---@class arbor.FormTreeNode
---@field value       string
---@field label       string
---@field children    arbor.FormTreeNode[]|nil
---@field group       boolean|nil   Non-selectable header (still expandable). Click toggles expansion.
---@field icon        string|nil    Lucide icon name shown before the label
---@field tag         string|nil    Small pill badge after the label (e.g. "Tomcat")
---@field tag_variant "neutral"|"ok"|"warn"|"error"|"accent"|"dev"|"prod"|"test"|nil
---@field description string|nil    Dim caption under the label

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
---@field change_action string|nil   Plugin action fired on selection change (non-group nodes only). Ctx contains the full form state plus `value` (the newly selected node's value). Ideal for master/detail layouts.

---@class arbor.FormTableColumn
---@field key         string
---@field label       string
---@field type        "text"|"number"|"checkbox"|"select"|nil  Default: "text"
---@field options     arbor.FormOptionInput[]|nil              Required for type="select"
---@field placeholder string|nil
---@field width       string|nil                                CSS width (e.g. "120px", "2fr")

---@class arbor.FormFieldTable : arbor.FormNodeBase
---@field type      "table"
---@field name      string
---@field label     string|nil
---@field columns   arbor.FormTableColumn[]
---@field default   table[]|nil     Array of row objects (keys match column.key)
---@field min_rows  integer|nil
---@field max_rows  integer|nil
---@field add_label string|nil      Default: "Add row"

---2-column container: navigation (toolbar + tree, typically) on the left,
---content (gated sections, typically) on the right. When the only root node
---of a form, the body automatically strips its padding so the split reaches
---the modal edges (IntelliJ look).
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

---Two-column label + controls row — use inside a card `section`. The label
---(and optional description) go on the left; `children` (inputs, buttons) on
---the right.
---@class arbor.FormNodeCardRow : arbor.FormNodeBase
---@field type        "card_row"
---@field label       string|nil
---@field description string|nil
---@field children    table[]

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

---Inline action button — fires `action` without submitting the form. With
---`icon_only = true` renders as a compact 26×26 square (useful in toolbars).
---`extra` is merged into the action payload alongside all form values — handy
---for item-specific actions in `cfg_list` / `card_row`.
---@class arbor.FormNodeButton : arbor.FormNodeBase
---@field type        "button"
---@field label       string|nil
---@field action      string
---@field variant     "default"|"primary"|"danger"|"ghost"|nil
---@field close_after boolean|nil
---@field disabled    boolean|nil
---@field icon        string|nil                Lucide icon name
---@field icon_only   boolean|nil               Hide label, render only icon
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
---@field nav_width             string|nil     CSS width (default: "240px")
---@field nav_collapsible       boolean|nil    Render a round toggle in the top-right corner to hide the sidebar. Preference persists under `arbor:tree-layout-collapsed:<id>` when the node has an `id`. When collapsed, a 34 px rail with a round reopen button is shown in place of the sidebar. Default false.
---@field nav_collapsed_default boolean|nil    Initial state on first open (overridden by stored preference). Default false.

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

---List every built-in hook with its full schema. Useful for generating docs
---or building runtime validators.
---@return arbor.HookDef[]
function Hooks.list() end

---Look up a single built-in hook by name. Returns nil for unknown hooks
---(plugin-defined action hooks, or typos in the name).
---@param  name string
---@return arbor.HookDef|nil
function Hooks.describe(name) end

---@type arbor.Hooks
arbor.hooks = Hooks
