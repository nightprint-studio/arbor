<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Source Export Plugin</h1>
<p>
  A Lua plugin that ships with Arbor. It exports the <strong>source of your
  current repo</strong> to a customer-visible copy, applying a declarative
  sequence of transformations (rename, delete internal files, bump versions,
  patch config, …) before handing the result off to their remote.
</p>
<p>
  The plugin compiles a <strong>profile</strong> (stored per-repo) into a
  live Arbor pipeline and runs it through the standard orchestrator — so you
  get the same streaming output, logging, resume and lock semantics
  as any other pipeline.
</p>

<h2>Opening the editor</h2>
<p>
  Click the <strong>Share2</strong> icon in the RepoActions bar (the split
  button with the dropdown). The primary click runs the selected profile; the
  dropdown is split in two:
</p>
<ul>
  <li><strong>Profiles</strong> (top) — selectable list, one per configured profile for the active repo. Clicking one sets it as the active selection (the Share2 button then runs it on click).</li>
  <li><strong>Actions</strong> (below a separator, footer) — open a modal directly and <em>do not</em> change the selected profile:
    <ul>
      <li><strong>New profile…</strong> — empty or from template.</li>
      <li><strong>Edit configurations…</strong> — open the full editor modal.</li>
      <li><strong>Plugin settings…</strong> — global settings (output folder, cleanup policy, templates, <code>ju</code> binary).</li>
    </ul>
  </li>
</ul>
<p>
  <em>Note:</em> this is the same pattern used by the Workspace dropdown —
  footer items never become the combo's active value, so you can pick
  "Edit configurations…" without losing track of which profile was selected.
</p>
<p>
  <strong>Sequences</strong> (multi-export meta-runs) live in a separate
  sidebar on the RIGHT ActivityBar — see the Sequences section below. The
  toolbar combo is intentionally per-repo only.
</p>

<h2>Profile shape</h2>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>branch_src</code></td><td>Branch / tag to clone from (empty = current HEAD). Autocomplete from local branches + tags.</td></tr>
    <tr><td><code>branch_dest</code></td><td>Destination branch (optional). Placeholder for a <code>git_push</code> step.</td></tr>
    <tr><td><code>remote_url</code></td><td>Destination remote URL (optional).</td></tr>
    <tr><td><code>auto_clone</code></td><td>When true (default), prepend an auto-clone stage: <code>git clone $SOURCE_PATH $OUTPUT_PATH</code> before any user step.</td></tr>
    <tr><td><code>log_level</code></td><td><code>debug</code> / <code>info</code> / <code>warn</code> / <code>error</code>. Debug prints every resolved command before execution.</td></tr>
    <tr><td><code>variables</code></td><td>User-defined <code>$KEY</code> / <code>$&#123;KEY&#125;</code> placeholders usable inside any string field.</td></tr>
    <tr><td><code>stages</code></td><td>Ordered list of groups; each group has a <code>mode</code> (<code>sequential</code> / <code>parallel</code>) and a list of steps.</td></tr>
  </tbody>
</table>

<h2>Variable expansion syntax</h2>
<p>
  Any string field in any step parameter runs through the expander before
  execution. The same resolver covers profile variables, sequence globals,
  per-item overrides, <code>set_variable</code> rebinds and the built-ins
  below — all in one namespace (built-ins win on name collision).
</p>
<table class="shortcuts-table">
  <thead><tr><th>Form</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>$NAME</code></td><td>Greedy match on <code>[A-Za-z0-9_]</code>. Unresolved → left as-is for debuggability.</td></tr>
    <tr><td><code>$&#123;NAME&#125;</code></td><td>Explicit brace form — required when the var is followed by letters / underscore (<code>$&#123;FOO&#125;bar</code> vs <code>$FOObar</code>).</td></tr>
    <tr><td><code>$&#123;NAME:default&#125;</code></td><td>Fallback when <code>NAME</code> is <em>unset OR empty</em> (bash <code>$&#123;VAR:-default&#125;</code> semantics). The default runs verbatim to the next <code>}</code> — it can contain <code>:</code> (URLs, paths); nesting is not supported.</td></tr>
    <tr><td><code>$&#123;NAME:&#125;</code></td><td>Default is an empty string — forces empty when unset.</td></tr>
    <tr><td><code>$$</code></td><td>Literal <code>$</code> escape.</td></tr>
  </tbody>
</table>
<p>
  The expansion applies to the profile's <code>branch_src</code> too — you
  can write <code>$&#123;RELEASE_BRANCH:main&#125;</code> and have the auto-clone
  stage resolve it at run time against the sequence's variable set.
</p>

<h2>Built-in variables</h2>
<p>Always available inside any string field (override user vars on name collision):</p>
<table class="shortcuts-table">
  <thead><tr><th>Name</th><th>Value</th></tr></thead>
  <tbody>
    <tr><td><code>$SOURCE_PATH</code></td><td>Absolute path of the active repo (cloned as the source).</td></tr>
    <tr><td><code>$OUTPUT_PATH</code></td><td>Absolute path of the auto-clone destination. This is the default <code>cwd</code> of every step.</td></tr>
    <tr><td><code>$BRANCH_SRC</code></td><td>Resolved source branch / tag.</td></tr>
    <tr><td><code>$BRANCH_DEST</code></td><td>Destination branch from the profile (may be empty).</td></tr>
    <tr><td><code>$PROFILE</code></td><td>Profile name.</td></tr>
    <tr><td><code>$RUN_ID</code></td><td>Unique id for this run (stable across retry/resume).</td></tr>
    <tr><td><code>$TIMESTAMP</code></td><td>ms since epoch at run start.</td></tr>
    <tr><td><code>$COMMIT_SHA</code></td><td>HEAD sha of the source, if known.</td></tr>
    <tr><td><code>$REPO_NAME</code></td><td>Source folder's basename.</td></tr>
  </tbody>
</table>

<h2>Sequences (cross-repo meta-runs)</h2>
<p>
  A <strong>Sequence</strong> is an ordered list of profile runs — optionally
  across different repositories — that share a single output folder and a
  matrix of variable overrides. Use it when a nightly build has to export
  several projects in a specific order, or when the same profile needs to run
  with several variable combinations.
</p>
<p>
  Sequences live in the <strong>right-side ActivityBar</strong> under the
  <em>Workflow</em> icon. The sidebar is a clean list of sequences; the
  editor and the run history are full modals opened from there.
</p>

<h3>The editor (3-column Items tab)</h3>
<ul>
  <li><strong>Info</strong> — name, description, fail-fast toggle, output root override, and the sequence-level <strong>Global variables</strong> (kv_list).</li>
  <li><strong>Items</strong> — 3-column layout:
    <ul>
      <li><em>Palette</em> (left): collapsible card per known repo; click any profile to append it to the sequence.</li>
      <li><em>Sequence items</em> (middle): ordered list of picked items; click to focus one.</li>
      <li><em>Detail</em> (right): move up / down / remove, <em>Profile</em> identity card with a click-to-copy repo path, a <em>Runtime</em> card (enabled / allow-failure), and <em>Variable overrides for this item</em> — the kv_list that layers on top of the sequence globals.</li>
    </ul>
  </li>
  <li><strong>History</strong> — this sequence's runs, newest first, with colored status glyphs and the output folder inline (click to copy, trailing button opens it in the OS file manager).</li>
</ul>

<h3>Matrix variables — merge order</h3>
<ol>
  <li>Profile-defined variables (tab Info of the profile itself)</li>
  <li>Sequence global variables</li>
  <li>Per-item variable overrides</li>
</ol>
<p>
  All merged into one namespace; last writer wins. Built-ins
  (<code>$OUTPUT_PATH</code>, <code>$SOURCE_PATH</code>, …) always dominate on
  name collision. Use <code>$&#123;NAME:default&#125;</code> for optional vars —
  see Variable expansion syntax above.
</p>

<h3>Output folder</h3>
<p>
  Every item in a sequence writes its output under
  <code>&lt;output_root&gt;/NN_profile/…</code>. If <code>output_root</code> is
  empty, the runtime auto-creates
  <code>&lt;plugin.output_folder&gt;/sequence_&lt;name&gt;_&lt;ts&gt;</code>. This
  override wins over the profile's own output logic for the duration of the
  sequence run — the profile stays untouched.
</p>

<h3>Fail-fast vs continue-on-error</h3>
<p>
  Off by default. When off, every enabled item runs even if a previous one
  failed, and the final status is <code>success</code> / <code>partial</code>
  / <code>failed</code> based on the mix. When on, the first failure marks
  the run <code>failed</code> and the rest are marked <code>skipped</code>.
</p>

<h3>Deep-linking into a run</h3>
<p>
  Each item row in the History modal shows the profile name as a clickable
  ghost button with an <code>ExternalLink</code> glyph. Click opens the
  standalone <strong>Pipeline Run Detail</strong> modal (z-index above the
  history modal) with the PipelineRunGraph + per-step output log — no need
  to open the bottom Pipelines panel.
</p>

<h3>Persistence</h3>
<p>
  Sequences are <strong>global</strong> (stored in
  <code>~/.config/arbor/plugin_data/source-export/global.json</code>) — they
  can fan out across repos from any workspace. Per-profile data (profiles
  themselves) remains per-repo as before. Runs are capped at the last 50
  entries and survive restarts; orphaned "running" runs left by a crash are
  swept to <code>failed</code> at plugin load.
</p>

<h2>Operations catalog</h2>

<h3>File</h3>
<table class="shortcuts-table">
  <thead><tr><th>Op</th><th>Purpose</th></tr></thead>
  <tbody>
    <tr><td><code>create_file</code></td><td>Write a new file with literal content (multi-line safe via base64).</td></tr>
    <tr><td><code>touch_file</code></td><td>Create an empty file, or update mtime if already present.</td></tr>
    <tr><td><code>copy_file</code></td><td>Copy file or directory to a new location.</td></tr>
    <tr><td><code>move_file</code></td><td>Move / rename.</td></tr>
    <tr><td><code>delete_file</code></td><td>Delete one or more exact paths.</td></tr>
    <tr><td><code>delete_pattern</code></td><td>Delete by glob pattern. Windows limitation: patterns are reduced to basenames (<code>**/*.tmp</code> → <code>*.tmp</code>) because PS <code>-Include</code> matches basenames only. Scope via the step's <code>cwd</code> or split into multiple steps.</td></tr>
    <tr><td><code>append_file</code></td><td>Append content to an existing file (multi-line safe).</td></tr>
    <tr><td><code>prepend_file</code></td><td>Prepend content (e.g. license headers).</td></tr>
  </tbody>
</table>

<h3>Content</h3>
<table class="shortcuts-table">
  <thead><tr><th>Op</th><th>Purpose</th></tr></thead>
  <tbody>
    <tr><td><code>replace_in_file</code></td><td>Find &amp; replace inside one file. <code>plain</code> = literal, else regex. Multi-line find/replace are base64-encoded so quoting and newlines round-trip intact.</td></tr>
    <tr><td><code>replace_on_glob</code></td><td>Same, applied to every file matching a glob. Logs every file it mutates.</td></tr>
    <tr><td><code>insert_at_anchor</code></td><td>Insert a block before/after the first line matching a regex anchor.</td></tr>
    <tr><td><code>properties_edit</code></td><td>Upsert <code>key=value</code> entries in a Java <code>.properties</code> file. Existing keys are replaced in place; missing ones appended.</td></tr>
    <tr><td><code>env_merge</code></td><td>Same, for <code>.env</code> files.</td></tr>
    <tr><td><code>template_render</code></td><td>Render a <code>.tmpl</code> file by substituting <code>&#123;&#123;VAR&#125;&#125;</code> placeholders with profile + built-in variables. Writes the output to a new path.</td></tr>
    <tr><td><code>json_edit</code></td><td>Set a value at a dotted path (<code>$.database.host</code>). Parsed &amp; written via <code>serde_json</code> (native LuaOp — cross-platform, no PowerShell). Value is parsed as JSON when possible (<code>42</code>, <code>true</code>, <code>&#34;x&#34;</code>, <code>&#123;&#34;y&#34;:1&#125;</code>), otherwise stored as string.</td></tr>
    <tr><td><code>yaml_edit</code></td><td>Dotted-path set on YAML files via <code>serde_yaml</code>. Intermediate maps are auto-created, and scalars are parsed with JSON semantics so numbers / booleans / nested objects round-trip correctly.</td></tr>
    <tr><td><code>toml_edit</code></td><td>Dotted-path set on TOML files via the <code>toml</code> crate. Same semantics as json_edit / yaml_edit.</td></tr>
    <tr><td><code>xml_edit</code></td><td>Set <code>InnerText</code> on a node, or value on an attribute via a minimal XPath subset (<code>//foo/@attr</code>, <code>/root/child[@k='v']</code>). Native LuaOp powered by <code>quick-xml</code> — no PowerShell, Unix friendly.</td></tr>
  </tbody>
</table>

<h3>Git</h3>
<p>
  <code>git_init</code>, <code>git_clone</code>, <code>git_commit</code>,
  <code>git_tag</code>, <code>git_push</code>, <code>git_checkout</code>,
  <code>git_cherry_pick</code>, <code>git_merge</code>,
  <code>git_submodule_update</code>. Every op logs the resolved args
  (<code>cwd</code>, <code>branch</code>, <code>ref</code>, …) before running.
  Git operations default their <code>cwd</code> to <code>$OUTPUT_PATH</code>
  so they act on the clone, never on the source.
</p>

<h3>Build / Dep</h3>
<table class="shortcuts-table">
  <thead><tr><th>Op</th><th>Behaviour</th></tr></thead>
  <tbody>
    <tr><td><code>mvn_set_version</code></td><td><code>mvn versions:set -DnewVersion=… -DgenerateBackupPoms=false</code>. Prefers local <code>mvnw</code> wrapper when present.</td></tr>
    <tr><td><code>mvn_deploy</code></td><td><code>mvn deploy [-P&lt;profile&gt;] &lt;extra&gt;</code>. Again prefers <code>mvnw</code>.</td></tr>
    <tr><td><code>gradle_task</code></td><td><code>gradlew &lt;tasks&gt;</code> when the wrapper exists, else <code>gradle</code>.</td></tr>
    <tr><td><code>gradle_offline</code></td><td><code>gradlew dependencies --refresh-dependencies</code> then copies <code>~/.gradle/caches</code> to <code>dest</code>. Basic implementation — production-grade offline bundles usually need extra config.</td></tr>
    <tr><td><code>npm_install</code></td><td><code>npm ci</code> (strict lockfile).</td></tr>
    <tr><td><code>pnpm_install</code></td><td><code>pnpm install --frozen-lockfile</code>.</td></tr>
    <tr><td><code>npm_pack</code></td><td><code>npm pack</code>.</td></tr>
    <tr><td><code>m2_offline_ju</code></td><td>Runs the external <code>ju</code> tool (path set in plugin settings) to extract Maven dependencies into an offline m2.</td></tr>
    <tr><td><code>docker_build</code></td><td><code>docker build -t &lt;tag&gt; -f &lt;dockerfile&gt; &lt;context&gt;</code>.</td></tr>
    <tr><td><code>docker_push</code></td><td><code>docker push &lt;tag&gt;</code>.</td></tr>
  </tbody>
</table>

<h3>Validation</h3>
<table class="shortcuts-table">
  <thead><tr><th>Op</th><th>Check</th></tr></thead>
  <tbody>
    <tr><td><code>assert_file_exists</code></td><td>File must exist. <strong>NOT</strong> toggle inverts — file must NOT exist.</td></tr>
    <tr><td><code>assert_cmd_exit_zero</code></td><td>Command must exit 0. NOT toggle — must exit non-zero.</td></tr>
    <tr><td><code>assert_env_set</code></td><td>Env var must be defined. NOT — must NOT be defined.</td></tr>
    <tr><td><code>assert_branch_clean</code></td><td>Working copy must have no uncommitted changes. NOT — must be dirty.</td></tr>
    <tr><td><code>assert_file_not_contains</code></td><td>Pattern must NOT appear. NOT — pattern MUST appear.</td></tr>
    <tr><td><code>assert_glob_matches</code></td><td>Number of files matching the glob must be within <code>[min, max]</code> (max empty = unlimited).</td></tr>
    <tr><td><code>assert_version_bump</code></td><td>Current version in <code>pom.xml</code> / <code>package.json</code> / <code>Cargo.toml</code> must be <em>less than</em> <code>new_version</code> (semver-ish comparison; prerelease tags ignored).</td></tr>
  </tbody>
</table>

<h3>Execution &amp; Flow</h3>
<table class="shortcuts-table">
  <thead><tr><th>Op</th><th>Behaviour</th></tr></thead>
  <tbody>
    <tr><td><code>shell_command</code></td><td>Arbitrary shell one-liner. Variables are expanded before execution.</td></tr>
    <tr><td><code>log_message</code></td><td>Print a log line at a given level.</td></tr>
    <tr><td><code>notify_toast</code></td><td>Surface a toast via <code>echo [NOTIFY] …</code>.</td></tr>
    <tr><td><code>set_variable</code></td><td><strong>Compile-time</strong> rebind: mutates <code>ctx.vars</code> so every subsequent step's command uses the new value. Note: it can't capture another step's stdout — use static values or previously-set vars in <code>value</code>.</td></tr>
  </tbody>
</table>

<h2>Not implemented (yet)</h2>
<p>
  Listed below so you see the gap at a glance when building a profile. Adding
  any of these to a profile makes the compiler refuse to start the run with a
  clear error pointing at the step.
</p>
<ul>
  <li><strong>chmod_file</strong>, <strong>normalize_eol</strong>, <strong>strip_bom</strong>, <strong>strip_comments</strong>.</li>
  <li><strong>lua_inline</strong> — inline Lua evaluated inside the pipeline. Requires orchestrator hooks.</li>
  <li><strong>try_on_error</strong> — control-flow op. Requires orchestrator-level policy changes.</li>
  <li><strong><code>set_variable</code> capture</strong> — the op currently takes a static <code>value</code>. Capturing another step's stdout needs orchestrator support for step-result chaining.</li>
</ul>

<h2>Shell vs LuaOp</h2>
<p>
  Most ops are implemented as <strong>LuaOp handlers</strong> — pure Lua in
  the Arbor process (no shell round-trip). They run faster, avoid the whole
  class of cmd.exe / PowerShell quoting traps, and have identical semantics
  on every OS. The remaining shell-bound ops are the ones that wrap external
  tools or run arbitrary user commands — no advantage to reimplement those
  in Lua.
</p>
<p>
  The 22 generic LuaOp ops live in the <code>arbor.core.*</code> built-in
  modules (shipped inside every plugin sandbox). Source Export opts them in
  at load time with four <code>require(...).register()</code> calls; any
  other plugin can do the same and use the same catalog.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Kind</th><th>Category</th><th>Who runs it</th></tr></thead>
  <tbody>
    <tr><td>LuaOp</td><td>create_file · touch_file · copy_file · move_file · delete_file · delete_pattern · append_file · prepend_file · replace_in_file · replace_on_glob · properties_edit · env_merge · template_render · insert_at_anchor · json_edit · yaml_edit · toml_edit · xml_edit · assert_file_exists · assert_file_not_contains · assert_glob_matches · assert_version_bump</td><td>In-process via <code>arbor.pipeline.register_op</code> handlers from <code>arbor.core.&#123;file,content,edit,assert&#125;</code></td></tr>
    <tr><td>Shell</td><td>shell_command · log_message · notify_toast · git_* · mvn_* · gradle_* · npm_* · pnpm_* · docker_* · m2_offline_ju · assert_cmd_exit_zero · assert_env_set · assert_branch_clean · set_variable (log-only stub)</td><td>Spawned process via <code>cmd /C</code> / <code>sh -c</code></td></tr>
  </tbody>
</table>

<h2>Safety guarantees</h2>
<p>
  Every destructive op (<code>delete_pattern</code>, <code>replace_on_glob</code>,
  …) uses paths <em>relative</em> to <code>$OUTPUT_PATH</code> by default. The
  clone lives under the plugin's <code>output_folder</code> setting (defaults
  to <code>%TEMP%\arbor-source-export</code> / <code>/tmp/arbor-source-export</code>),
  suffixed with the profile name + a ms-precision timestamp so every run starts
  from a fresh, unique directory. The source repo is never touched.
</p>
<p>
  Only change <code>output_folder</code> to a sensitive parent dir (e.g. your
  home) if you understand the blast radius: a <code>delete_pattern</code>
  inside a run would then see any files there.
</p>

<h2>Import / export</h2>
<p>
  Profiles and individual stages can be exported as JSON. Use the Upload icon
  in the toolbar of the Configurations modal (whole profile) or in a stage
  header (single stage). Import opens a native file picker so you can re-import
  anywhere. IDs are refreshed on import so imported stages never collide with
  existing ones.
</p>
