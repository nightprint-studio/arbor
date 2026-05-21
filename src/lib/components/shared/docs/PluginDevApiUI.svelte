<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Plugin Development â€” API: UI</h1>
<p>APIs for interacting with the Arbor user interface: notifications, forms, activity bar entries, keyboard shortcuts, and the command palette.</p>

<h2>arbor.ui â€” user interface</h2>
<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.notify{`{`} message, title?, level?, action? {`}`}</code></td><td>Add a persistent notification to the in-app notification center. <code>level</code>: <code>"info" | "success" | "warning" | "error"</code> (default <code>"info"</code>). See the <em>arbor.notify</em> section below.</td></tr>
    <tr><td><code>arbor.ui.form(config)</code></td><td>Display an input form modal; submitting fires <code>submit_action</code></td></tr>
    <tr><td><code>arbor.ui.confirm&#123; message, confirm_label?, confirm_variant?, state? &#125;</code></td><td>Confirmation dialog. Returns a Promise that resolves with <code>true</code> on confirm and <code>false</code> on cancel. <code>confirm_variant</code>: <code>"primary" | "danger" | "ghost"</code>.</td></tr>
    <tr><td><code>arbor.ui.pick_file(opts)</code></td><td>Native file/folder picker. Fires <code>opts.action</code> with <code>&#123; path, ...opts.extra &#125;</code> on confirm; empty <code>path</code> on cancel. <code>opts.mode</code>: <code>"file"</code> (default), <code>"folder"</code>, <code>"save"</code>. Optional: <code>title</code>, <code>extensions</code>, <code>initial_path</code>.</td></tr>
    <tr><td><code>arbor.ui.add_sidebar(opts)</code></td><td>Register a plugin panel attached to an ActivityBar icon. Accepts <code>side: "left"|"right"</code> (default "right"), <code>position: "top"|"bottom"</code> (default "top"), and <code>kind: "form"|"tree"</code> (default "form"). Form panels respond to <code>panel:open:&lt;id&gt;</code> with <code>set_panel_content</code>; tree panels push nodes via <code>tree.set</code> and accept cross-plugin contributions â€” see the <em>Tree sidebars</em> section below.</td></tr>
    <tr><td><code>arbor.ui.set_panel_content(id, body)</code></td><td>Push form-DSL content (<code>&#123;title, nodes, actions?&#125;</code>) into a registered panel. Call from the <code>panel:open:&lt;id&gt;</code> handler, or any time underlying state changes.</td></tr>
    <tr><td><code>arbor.ui.tree.set(sidebar_id, body)</code></td><td>Push a tree snapshot into a <code>kind="tree"</code> sidebar. <code>body</code> is <code>&#123;title?, breadcrumb?, nodes&#125;</code> or a bare nodes array. <code>breadcrumb</code> is an optional list of segments <code>&#123;label, icon?, action?, data?, badge?, tooltip?&#125;</code> rendered as a clickable trail above the tree â€” segments with empty <code>action</code> are non-interactive (the current location). Triggers a re-render on the frontend. <br><br><strong>Multi-selection:</strong> tree sidebars now support Ctrl/Cmd+click toggle and Shift+click range. Context-menu items can scope themselves via <code>when.multi</code>: <code>true</code> = only in multi-select, <code>false</code> = single-row only, omitted = both. Action handlers receive <code>ctx.node_ids[]</code> and <code>ctx.nodes[]</code> (single-row contexts get a 1-element array; <code>ctx.node_id</code> and <code>ctx.data</code> stay populated for backward compat).</td></tr>
    <tr><td><code>arbor.ui.tree.get(sidebar_id)</code></td><td>Read the snapshot you most recently set, or <code>nil</code>. Useful when merging incremental updates without keeping a parallel cache.</td></tr>
    <tr><td><code>arbor.ui.contribute(point, item)</code></td><td>Push an item into a contribution point owned by another plugin. <code>item = &#123;id, payload, priority?, when?, disabled?, group?&#125;</code>. Re-contributing with the same id replaces the previous payload (idempotent). <code>when</code> / <code>disabled</code> / <code>group</code> live at the top level â€” placing them inside <code>payload</code> still works but logs a deprecation warn.</td></tr>
    <tr><td><code>arbor.ui.unregister_contribution(point, item_id)</code></td><td>Remove a contribution your plugin previously pushed.</td></tr>
    <tr><td><code>arbor.ui.contribution_point(config)</code></td><td>Declare a contribution point owned by your plugin. <code>config = &#123;name, description?, schema?&#125;</code>. Informational â€” listed in <code>list_contribution_points</code>; payloads are NOT validated at runtime.</td></tr>
    <tr><td><code>arbor.ui.list_contributions(point)</code></td><td>Read the merged list of contributions for a point (sorted by <code>priority</code>). Lets a host plugin fold contributions into its own snapshot.</td></tr>
    <tr><td><code>arbor.ui.container.register(opts)</code></td><td>Declare an aggregated modal. <code>opts = &#123;id, title, kind?, layout?, width?, height?, submit_label?, cancel_label?, on_load?, on_save?&#125;</code>. <code>width</code> / <code>height</code> in <code>px</code> reference a 1920Ã—1080 window and scale linearly with the actual viewport (so <code>"960px"</code> means "half the viewport"). Body is built from <code>&lt;plugin&gt;::&lt;id&gt;:category</code> + <code>&lt;plugin&gt;::&lt;id&gt;:section</code> contributions.</td></tr>
    <tr><td><code>arbor.ui.container.open(key)</code> Â· <code>close(key)</code></td><td>Show / hide a registered container by its <code>"&lt;plugin&gt;::&lt;id&gt;"</code> key.</td></tr>
    <tr><td><code>arbor.ui.settings.panel(config)</code></td><td>Sugar over <code>container.register</code>: same shape, but forces <code>kind = "modal"</code> + <code>layout = "tree_nav"</code> and binds the sub-points to the conventional <code>&lt;plugin&gt;:settings:&#123;category,section&#125;</code> naming. The gear in Plugin Manager appears whenever a plugin owns at least one container.</td></tr>
    <tr><td><code>arbor.ui.settings.open(plugin_name, panel_id)</code></td><td>Open a registered settings panel programmatically. Same effect as the user clicking the gear icon.</td></tr>
    <tr><td><code>arbor.ui.settings.close()</code></td><td>Close the currently open settings panel.</td></tr>
    <tr><td><code>arbor.ui.icon.register(config)</code></td><td>Register a custom SVG icon, namespaced as <code>plugin:&lt;your_plugin&gt;:&lt;id&gt;</code>. Reference it from any <code>icon</code> field. Wiped on plugin reload / disable.</td></tr>
    <tr><td><code>arbor.ui.add_graph_combo(opts)</code></td><td>Register a split button (run + dropdown). <code>target</code>: "activity_bar" (default) or "repo_actions"</td></tr>
    <tr><td><code>arbor.ui.set_combo_options&#123; id, options, selected? &#125;</code></td><td>Dynamically update a combo's option list (call from <code>on_repo_open</code> to refresh per-repo). Optional <code>selected</code> adopts a pick if it appears in <code>options</code>. Thin sugar over <code>contribute_patch("arbor:activitybar", id, &#123;options=â€¦&#125;)</code>.</td></tr>
    <tr><td><code>arbor.ui.set_autocomplete_options(id, opts)</code></td><td>Reply with fresh suggestions for an autocomplete field using <code>source_action</code>. Call inside the handler registered for that action.</td></tr>
    <tr><td><code>arbor.ui.form.set_options(name, opts)</code></td><td>Swap the option list of a select / radio / autocomplete field in the currently-open form</td></tr>
    <tr><td><code>arbor.ui.form.set_disabled(name, bool)</code></td><td>Disable or re-enable a field in the currently-open form</td></tr>
    <tr><td><code>arbor.ui.form.set_value(name, v)</code></td><td>Programmatically set a field's value in the currently-open form</td></tr>
    <tr><td><code>arbor.ui.form.replace(cfg)</code></td><td>Swap the whole node tree of the open form in-place, preserving field values by <code>name</code>. See <em>Dynamic form updates</em>.</td></tr>
    <tr><td><code>arbor.ui.form.set_loading(arg)</code></td><td>Toggle the loading overlay without re-rendering the form. <code>arg</code> can be <code>true</code> / <code>false</code>, a label string (implies <code>true</code>), or <code>&#123; loading, label &#125;</code>. Cheap â€” use it for per-step progress ticks during a fan-out loop.</td></tr>
    <tr><td><code>arbor.ui.form.close()</code></td><td>Programmatically dismiss the currently-open form. Pair with <code>keep_open = true</code> on the form config when submit launches a follow-up flow (file picker, second form): the modal stays mounted while the secondary flow is up, and you call <code>close()</code> once it completes.</td></tr>
    <tr><td><code>arbor.ui.operation.start&#123;â€¦&#125;</code></td><td>Push a progress card into the operations overlay (same widget used by Pull / Fetch-all / Pull-all). Config: <code>&#123;id, title, subtitle?, steps[&#123;key,label&#125;], current?&#125;</code>. The id is plugin-scoped server-side â€” collisions across plugins are impossible.</td></tr>
    <tr><td><code>arbor.ui.operation.set_current(id, step_key, detail?)</code></td><td>Move the active-step pointer; auto-completes earlier rows and leaves later ones pending.</td></tr>
    <tr><td><code>arbor.ui.operation.update_step(id, step_key, &#123;status?, detail?&#125;)</code></td><td>Patch a single row. <code>status</code>: <code>"pending"|"completed"|"skipped"|"error"</code>. Avoid setting <code>"active"</code> here â€” use <code>set_current</code> instead (sticky active = forever spinner).</td></tr>
    <tr><td><code>arbor.ui.operation.finish(id, &#123;summary?, error?&#125;)</code></td><td>Close the card. It lingers a few seconds with the summary or error, then auto-dismisses.</td></tr>
    <tr><td><code>arbor.ui.add_separator()</code></td><td>Insert a horizontal separator in the activity bar after the last registered item</td></tr>
    <tr><td><code>arbor.ui.add_context_menu_item(opts)</code></td><td>Add item to the commit/branch/file context menu</td></tr>
    <tr><td><code>arbor.ui.add_menu_item(opts)</code></td><td>Add item to the hamburger menu</td></tr>
    <tr><td><code>arbor.ui.add_toolbar_action(opts)</code></td><td>Add an inline action button to one of Arbor's toolbars. <code>target</code>: <code>"diff"</code>, <code>"status-bar:left"</code>, <code>"status-bar:right"</code>, <code>"title-bar:left"</code>, <code>"title-bar:right"</code>, <code>"commit-detail"</code>, <code>"commit-form"</code>. Unknown targets pass through verbatim â€” usable for plugin-owned custom toolbars.</td></tr>
    <tr><td><code>arbor.ui.open_path(path)</code></td><td>Hand a file/folder to the OS default handler (Explorer on Windows, Finder on macOS, xdg-open on Linux). Used to expose "Open in file manager" affordances on artefact folders.</td></tr>
    <tr><td><code>arbor.ui.copy_to_clipboard&#123; text, toast? &#125;</code></td><td>Copy <code>text</code> to the system clipboard via the webview; optional <code>toast</code> overrides the success message ("Copied to clipboard" by default). For one-shot copies driven by the user clicking a value, prefer the <code>copy_link</code> DSL node â€” it runs entirely client-side with no plugin hop.</td></tr>
    <tr><td><code>arbor.ui.show_pipeline_run(run_id)</code></td><td>Deep-link to a pipeline run: opens a standalone detail modal (graph + output log) on top of whatever is currently visible. No-op on empty <code>run_id</code>. Use it to jump from a plugin's own UI (sidebar, history modal, â€¦) to the canonical run view without opening the bottom Pipelines panel.</td></tr>
    <tr><td><code>arbor.ui.set_branding&#123; svg? | svg_path?, window_icon_path? &#125;</code></td><td>Replace the default Arbor app mark. Pass either <code>svg</code> (inline markup) <em>or</em> <code>svg_path</code> (absolute path read off disk by the host â€” no <code>fs.read</code> perm needed) to paint the in-app surfaces (title-bar slot, welcome screen, About modal, HTML stats export). <code>window_icon_path</code> is an absolute path to a <strong>raster</strong> image (PNG / ICO) handed to the OS window-icon API â€” taskbar / Alt-Tab / window chrome on Windows &amp; Linux. At least one field is required; missing fields don't reset their counterpart. RAM-only â€” see the <em>Branding &amp; theme</em> section below.</td></tr>
    <tr><td><code>arbor.ui.clear_branding()</code></td><td>Restore both the bundled SVG mark and the bundled window icon. No-op when the current override belongs to another plugin.</td></tr>
    <tr><td><code>arbor.ui.set_theme_tokens&#123; vars &#125;</code></td><td>Layer a CSS-variable overlay on top of the active theme. <code>vars</code> is a <code>"--name" = "value"</code> table (every key must start with <code>--</code>). Overlays survive theme switches; they vanish on plugin reload or <code>clear_theme_tokens</code>.</td></tr>
    <tr><td><code>arbor.ui.clear_theme_tokens()</code></td><td>Drop this plugin's overlay; other plugins' overlays remain.</td></tr>
  </tbody>
</table>

<h2>The unified contribution model</h2>
<p>
  Every <code>add*</code> / <code>set*</code> / <code>register</code> call above is sugar
  on top of <code>arbor.ui.contribute(point, item)</code>. Each surface â€” context menu,
  command palette, keybindings, sidebars, activity bar, icons, tree state, panel content â€”
  is exposed as a <strong>well-known contribution point</strong>. Plugins may use the sugar
  API or call <code>contribute</code> directly; the result is the same.
</p>
<p>The frontend reads a single canonical store (<code>list_plugin_contributions(point)</code>) and listens to <code>arbor://contributions-changed</code> to refresh. Render-time iteration goes through one host-side primitive (<code>&lt;Contribution point=â€¦&gt;</code>) that filters out items from disabled plugins, applies <code>when</code> / <code>disabled</code> automatically, wraps each snippet in an error boundary, and exposes a <code>fire(extra?)</code> helper.</p>
<p>
  <strong>Top-level fields.</strong>
  <code>when</code>, <code>disabled</code>, <code>group</code> are typed top-level fields on the
  contribution item â€” not magic keys inside <code>payload</code>. <code>when</code> takes
  <code>&#123;kind?: string|string[], data_field?: &#123;key, value&#125;&#125;</code> and is
  matched against the renderer's context. <code>disabled = true</code> hides the item
  without unregistering it. <code>group</code> is a free-form bucket label consumers can
  use to render section headers.
</p>
<p>
  <strong>Built-in point validation.</strong>
  Payloads contributed to built-in points (<code>arbor:status-bar:*</code>,
  <code>arbor:menu</code>, <code>arbor:keybinding</code>, etc.) are checked against a
  shape at <code>contribute</code> time. A malformed payload is logged
  (<code>tracing::error</code>) with the offending plugin / point / item id and dropped
  before it reaches the registry. Plugin-defined points (anything that doesn't
  start with <code>arbor:</code>) are not validated.
</p>
<h3>Sugar APIs â†” contribution points</h3>
<table class="shortcuts-table">
  <thead><tr><th>Built-in point</th><th>Sugar API</th><th>Payload</th></tr></thead>
  <tbody>
    <tr><td><code>arbor:context-menu:&lt;target&gt;</code></td><td><code>add_context_menu_item</code></td><td><code>&#123;target, label, action, icon?&#125;</code></td></tr>
    <tr><td><code>arbor:menu</code></td><td><code>add_menu_item</code></td><td><code>&#123;label, action, icon?&#125;</code></td></tr>
    <tr><td><code>arbor:sidebar</code></td><td><code>add_sidebar</code></td><td><code>&#123;action, label, icon?, side, position, kind, â€¦&#125;</code></td></tr>
    <tr><td><code>arbor:activitybar</code></td><td><code>add_graph_combo</code> Â· <code>add_separator</code></td><td><code>&#123;kind: "combo"|"separator", target, â€¦&#125;</code></td></tr>
    <tr><td><code>arbor:diff-toolbar</code><br/><code>arbor:status-bar:&lt;side&gt;</code><br/><code>arbor:title-bar:&lt;side&gt;</code><br/><code>arbor:commit-detail:action</code><br/><code>arbor:commit-form:action</code></td><td><code>add_toolbar_action</code> (single sugar, <code>target</code> selects)</td><td><code>&#123;label?, icon?, action, tooltip?, color?&#125;</code></td></tr>
    <tr><td><code>arbor:command-palette</code></td><td><code>arbor.command.register</code></td><td><code>&#123;title, description?, icon?, group?&#125;</code></td></tr>
    <tr><td><code>arbor:keybinding</code></td><td><code>arbor.keybinding.register</code></td><td><code>&#123;key, ctrl?, shift?, alt?, action, description?&#125;</code></td></tr>
    <tr><td><code>arbor:icon</code></td><td><code>arbor.ui.icon.register</code></td><td><code>&#123;svg&#125;</code></td></tr>
    <tr><td><code>arbor:tree-state</code></td><td><code>arbor.ui.tree.set</code></td><td><code>&#123;title?, nodes[], version&#125;</code> â€” replace-by-id</td></tr>
    <tr><td><code>arbor:panel-content</code></td><td><code>arbor.ui.set_panel_content</code></td><td><code>&#123;title?, nodes, actions?&#125;</code> â€” replace-by-id</td></tr>
    <tr><td><code>&lt;plugin&gt;::&lt;id&gt;:category</code><br/><code>&lt;plugin&gt;::&lt;id&gt;:section</code></td><td><code>arbor.ui.container.register</code> + <code>arbor.ui.contribute</code></td><td>Aggregated modal (containers). See <em>Containers</em> below.</td></tr>
  </tbody>
</table>
<p>
  Context menus are split <strong>per target</strong> so consumers subscribe only to
  the slot they care about. Use <code>add_context_menu_item(&#123;target = "commit", â€¦&#125;)</code>
  â€” the dual-write derives the point name as <code>arbor:context-menu:commit</code>.
  Known targets: <code>commit</code>, <code>branch</code>, <code>tag</code>, <code>stash</code>,
  <code>file</code>, <code>remote</code>, <code>submodule</code>, <code>worktree</code>,
  <code>line</code>, <code>hunk</code>, <code>tab</code>, plus any plugin-defined string.
</p>
<p>
  Re-contributing with the same <code>(plugin, point, id)</code> replaces the previous payload,
  so the sugar APIs that update state at runtime (<code>set_combo_options</code>,
  <code>tree.set</code>, <code>set_panel_content</code>) work naturally. Use a stable
  <code>id</code> to keep updates idempotent.
</p>
<p>
  When you only want to update <em>some</em> fields of an item without re-specifying
  the whole payload, use <code>arbor.ui.contribute_patch(point, id, partial)</code> â€”
  it shallow-merges <code>partial</code> into the existing payload and writes back.
  <code>set_combo_options</code> is a thin sugar over this primitive.
</p>

<h3>Toolbar action points (covered by <code>add_toolbar_action</code>)</h3>
<p>
  Inline action buttons on Arbor's toolbars all share one sugar:
  <code>arbor.ui.add_toolbar_action(&#123;id, target, action, label?, icon?, tooltip?, color?&#125;)</code>.
  The <code>target</code> short-name selects which toolbar; the renderer ignores
  fields it doesn't care about.
</p>
<table class="shortcuts-table">
  <thead><tr><th>target</th><th>Point</th><th>Where it renders</th></tr></thead>
  <tbody>
    <tr><td><code>"status-bar:left"</code></td><td><code>arbor:status-bar:left</code></td><td>StatusBar, after the built-in indicators (branch / change pills).</td></tr>
    <tr><td><code>"status-bar:right"</code></td><td><code>arbor:status-bar:right</code></td><td>StatusBar, before jobs / notifications / version (always visible).</td></tr>
    <tr><td><code>"title-bar:left"</code></td><td><code>arbor:title-bar:left</code></td><td>TitleBar, after the workspace dropdown.</td></tr>
    <tr><td><code>"title-bar:right"</code></td><td><code>arbor:title-bar:right</code></td><td>TitleBar, before docs / theme / settings.</td></tr>
    <tr><td><code>"diff"</code></td><td><code>arbor:diff-toolbar</code></td><td>DiffViewer header â€” next to Copy / Maximize.</td></tr>
    <tr><td><code>"commit-detail"</code></td><td><code>arbor:commit-detail:action</code></td><td>CommitDetailPanel â€” action row below the body. Fired with the commit oid.</td></tr>
    <tr><td><code>"commit-form"</code></td><td><code>arbor:commit-form:action</code></td><td>CommitForm â€” between the Amend toggle and the Commit split button.</td></tr>
    <tr><td><code>"workspace-row"</code></td><td><code>arbor:workspace-row</code></td><td>WorkspaceManagementModal â€” per-workspace action toolbar (after Edit / Export / Delete). Fired with <code>&#123;workspace_id, workspace_name, repo_count&#125;</code>.</td></tr>
    <tr><td><code>&lt;custom&gt;</code></td><td>verbatim</td><td>Any other string passes through unchanged â€” use this to target your own plugin's toolbars without a separate sugar.</td></tr>
  </tbody>
</table>

<h3>Decorator points (no sugar yet â€” use <code>arbor.ui.contribute</code>)</h3>
<table class="shortcuts-table">
  <thead><tr><th>Point</th><th>Where it renders</th><th>Payload</th></tr></thead>
  <tbody>
    <tr><td><code>arbor:branch-decorator</code></td><td>BranchTree â€” badge next to a branch row. <code>branch_pattern</code> filters which branches.</td><td><code>&#123;branch_pattern?, label?, icon?, color?, tooltip?&#125;</code></td></tr>
    <tr><td><code>arbor:file-decorator</code></td><td>FileDiffList / FileTree â€” badge next to a file path.</td><td><code>&#123;path_pattern?, label?, icon?, color?, tooltip?&#125;</code></td></tr>
    <tr><td><code>arbor:welcome-action</code></td><td>WelcomeScreen â€” quick-action card.</td><td><code>&#123;title, description?, icon?, action&#125;</code></td></tr>
    <tr><td><code>arbor:pipelines:toolbar</code></td><td>PipelinesPanel â€” extra icon-only buttons in the left vertical toolbar (Local Pipelines tab), after the built-in Run / Stop / Resume / Clear cluster.</td><td><code>&#123;icon, tooltip?, label?, accent?, success?, danger?, divider_before?, disabled?&#125;</code></td></tr>
  </tbody>
</table>
<p>
  Some decorator points may not yet have a built-in consumer in your version
  of Arbor â€” they are declared up-front so plugins can start contributing
  without API churn.
</p>

<h3>Toolbar action â€” example</h3>
<pre class="language-lua">{@html highlight(`-- Status bar pill that opens the build settings on click.
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
})`, '.lua')}</pre>

<h2>Branding &amp; theme</h2>
<p>
  Plugins can swap the app mark and overlay extra CSS variables on top of the
  active theme to deliver an enterprise-branded experience. Both surfaces are
  <strong>RAM-only</strong>: nothing is persisted, so reloading Arbor restores
  the bundled identity unless the same plugin re-applies the overrides during
  its <code>on_plugin_load</code> handler.
</p>
<h3>Replace the logo</h3>
<p>
  <code>arbor.ui.set_branding</code> covers two surfaces and the plugin
  picks which to override per call:
</p>
<ul>
  <li><code>svg</code> â€” inline SVG markup (the string must start with
    <code>&lt;svg</code>). Paints every in-app surface that shows the
    Arbor identity (title bar, welcome screen, About modal) <em>and</em>
    is embedded by the HTML stats exporter so co-branded reports stay
    consistent without a second round-trip through the plugin.</li>
  <li><code>svg_path</code> â€” alternative to <code>svg</code>: absolute
    path to an <code>.svg</code> file the host reads off disk. Use this
    when you'd rather ship the artwork as a sibling asset
    (<code>assets/logo.svg</code>) than embed it as a long string in
    <code>main.lua</code>. Same trust model as <code>window_icon_path</code> â€”
    no <code>fs.read</code> permission is required since the read happens
    server-side. Mutually exclusive with <code>svg</code>.</li>
  <li><code>window_icon_path</code> â€” absolute path to a <strong>raster</strong>
    image (PNG or ICO; SVG is rejected because the OS window-icon API
    needs a rasterised buffer and Arbor doesn't bundle a renderer). Used
    for the OS-level icon: taskbar, Alt-Tab list and window chrome on
    Windows / Linux. macOS dock icons come from <code>Info.plist</code>
    and require a build-time swap, so this field is a no-op there.</li>
</ul>
<p>
  Either field can be supplied alone â€” a follow-up call that only sets
  <code>window_icon_path</code> swaps the icon without touching the SVG,
  and vice-versa. <code>arbor.ui.clear_branding()</code> drops both at
  once and restores the bundled assets.
</p>
<pre class="language-lua">{@html highlight(`-- Replace the Arbor mark + the OS window icon for this session.
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
arbor.ui.clear_branding()`, '.lua')}</pre>

<h3>Overlay extra theme tokens</h3>
<p>
  <code>arbor.ui.set_theme_tokens&#123;vars&#125;</code> writes a map of CSS
  custom properties (each key must start with <code>--</code>) onto the
  document root, layered <em>on top of</em> the active theme. Overlays
  survive theme switches: when the user picks a new theme, Arbor reapplies
  the active theme first and then re-merges every plugin overlay. Each
  plugin owns one overlay slot; calling <code>set_theme_tokens</code> twice
  replaces the previous payload, and <code>clear_theme_tokens</code>
  releases just this plugin's slot.
</p>
<pre class="language-lua">{@html highlight(`-- Tint the accent + diff colors with the corporate palette.
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
arbor.ui.clear_theme_tokens()`, '.lua')}</pre>

<h2>arbor.notify â€” persistent notifications</h2>
<p>Adds a notification to the in-app notification center (bell icon in the status bar). Notifications persist until the user explicitly dismisses them. An optional <code>action</code> table renders a click button on the notification that triggers a built-in side-effect. Boundary validation: <code>message</code> must be a non-empty string and <code>level</code> (when supplied) must be one of <code>"info"|"success"|"warning"|"error"</code> â€” invalid input raises a Lua error.</p>
<pre class="language-lua">{@html highlight(`-- arbor.notify{ message, title?, level?, action? }
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
              action = { kind = "open-tab-by-repo-id", label = "Focus tab", repo_id = "..." } }`, '.lua')}</pre>
<p><strong>Action kinds</strong>:</p>
<ul>
  <li><code>open-link-manager</code> â€” needs <code>label</code>, <code>link_id</code>; opens the Linked Worktrees manager pre-selected on that link.</li>
  <li><code>open-tab-by-repo-id</code> â€” needs <code>label</code>, <code>repo_id</code>; activates the matching open tab (no-op if not currently open).</li>
</ul>

<h2>arbor.command â€” command palette entries</h2>
<p>Register items that appear in the Command Palette (<kbd>Ctrl+K</kbd>). Each entry fires the action <code>command:&lt;id&gt;</code> on the plugin when selected.</p>
<pre class="language-lua">{@html highlight(`arbor.command.register({
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
arbor.command.unregister("my-action")`, '.lua')}</pre>

<h2>arbor.contribution â€” registry introspection</h2>
<p>
  Read-only access to the unified contribution registry. A plugin can list every
  contribution registered against a point and every point that's been declared.
  Useful when a host plugin wants to know whether someone has overridden one of
  its sections, when defaulting depends on what's currently contributed, or when
  one plugin orchestrates several others.
</p>
<table class="shortcuts-table">
  <thead><tr><th>API</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.contribution.list(point)</code></td><td>Items contributed to <code>point</code>, sorted by <code>priority</code>. Each item: <code>&#123;plugin_name, item_id, payload, priority, when?, disabled?, group?&#125;</code>. <code>payload</code> is a Lua table.</td></tr>
    <tr><td><code>arbor.contribution.list_points()</code></td><td>Every declared contribution point: <code>&#123;plugin_name, name, description?, schema?&#125;</code>.</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`-- Skip the manual entry if another plugin already
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
end`, '.lua')}</pre>
<p>
  Reads only: there is no <code>subscribe</code>. Plugins that need to react to
  contribution changes can listen to the <code>arbor://contributions-changed</code>
  Tauri event via the standard hook mechanism.
</p>

<h2>arbor.keybinding â€” plugin keyboard shortcuts</h2>
<p>Register keyboard shortcuts that fire a Lua action when triggered anywhere in the app. Plugin shortcuts are visible under the <strong>Plugins</strong> group in <strong>Settings â†’ Keybindings</strong> (read-only).</p>
<pre class="language-lua">{@html highlight(`-- Call once during on_plugin_load.
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
end)`, '.lua')}</pre>
<p><strong>Note:</strong> plugin keybindings take priority over unbound app keys when the shortcut matches. They do <em>not</em> override user-customised app keybindings.</p>
<p>
  Registered shortcuts surface automatically in <strong>Settings â†’ Keybindings</strong>
  (read-only "Plugins" section) and the <strong>Shortcuts</strong> documentation page.
  No extra UI wiring is required from the plugin side.
</p>

<h2>Combo Button</h2>
<p>
  A split widget: a primary action button (icon only) on the left and a dropdown arrow on the right.
  <code>run_icon</code> accepts any Lucide icon name â€” common choices: <code>"Play"</code> (â–¶),
  <code>"Hammer"</code> (ðŸ”¨), <code>"Wrench"</code>, <code>"Zap"</code>.
  You can register <strong>multiple combos</strong> from the same plugin; they appear in
  registration order within the target area.
</p>
<pre class="language-lua">{@html highlight(`-- Register once (e.g. in on_plugin_load).
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
end)`, '.lua')}</pre>

<h3>Action Options</h3>
<p>
  Mark an option with <code>action = true</code> to make it behave like
  <em>"New Workspace"</em> in the workspace dropdown: clicking it fires the
  combo's <code>run_action</code> directly (so the plugin can open a modal or
  settings form) and does <strong>not</strong> become the persisted selection â€”
  the previously selected config stays active in the run button. Action options
  render in a visually separated footer below a divider.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.set_combo_options{
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
end)`, '.lua')}</pre>

<h3>Rich Combo Options</h3>
<p>
  Each combo option supports the following extra fields (all optional, additive
  on top of <code>value</code> / <code>label</code>):
</p>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Type</th><th>Effect</th></tr></thead>
  <tbody>
    <tr><td><code>icon</code></td><td>string (Lucide name)</td><td>Small icon rendered before the label.</td></tr>
    <tr><td><code>subtitle</code></td><td>string</td><td>Caption shown below the label in muted text.</td></tr>
    <tr><td><code>meta</code></td><td>string</td><td>Right-aligned tabular text (counts, durations, â€¦).</td></tr>
    <tr><td><code>disabled</code></td><td>boolean</td><td>Renders the option dimmed and prevents selection.</td></tr>
    <tr><td><code>group</code></td><td>string</td><td>Group label â€” consecutive options sharing a group are bucketed under a header.</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`arbor.ui.set_combo_options{
  id = "my_plugin:run",
  options = {
    { value = "dev",  label = "dev",  group = "Project",
      icon = "Play",  subtitle = "fast feedback",  meta = "~3s" },
    { value = "prod", label = "prod", group = "Project",
      icon = "Rocket", subtitle = "release build", meta = "~45s" },
    { value = "stale", label = "legacy", group = "Project", disabled = true },
  },
}`, '.lua')}</pre>

<h2>Sidebar Panels (add_sidebar)</h2>
<p>
  Register a plugin panel attached to an ActivityBar icon. By default the
  icon appears on the <strong>right</strong> ActivityBar â€” a dedicated
  plugin-expansion rail, visually identical to the left but dedicated to
  plugins. The left bar is reserved for built-in Arbor sections, though
  plugins may also target <code>side="left"</code> when it makes sense.
</p>
<p>
  The right ActivityBar is <strong>completely hidden</strong> when no plugin
  has registered a right-side entry â€” the layout falls back to the classic
  single-bar style.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Field</th><th>Values</th><th>Default</th></tr></thead>
  <tbody>
    <tr><td><code>id</code></td><td>string (unique per plugin)</td><td>â€” required â€”</td></tr>
    <tr><td><code>side</code></td><td><code>"left"</code> | <code>"right"</code></td><td><code>"right"</code></td></tr>
    <tr><td><code>position</code></td><td><code>"top"</code> (side panel) | <code>"bottom"</code> (shared bottom slot)</td><td><code>"top"</code></td></tr>
    <tr><td><code>icon</code></td><td>Lucide icon name or single-char emoji</td><td>â€” generic icon â€”</td></tr>
    <tr><td><code>label</code> / <code>tooltip</code></td><td>string</td><td>falls back to <code>id</code></td></tr>
  </tbody>
</table>
<p>
  The <strong>bottom slot is unique</strong>: clicking a plugin-bottom icon
  overrides whichever panel was open (stage / detail / terminal / jobs /
  pipelines / another plugin) â€” only ONE bottom panel is visible at any
  time, regardless of which ActivityBar fired the click.
</p>
<p>
  Every bottom panel â€” built-in or plugin-contributed â€” wears the same
  standardized header chrome: a 34-px bar on <code>--bg-base</code> with the
  panel title on the left, optional inline content, plugin/built-in
  toolbar actions on the right, and a red dot close button at the very end
  (the same widget used by modal headers). For plugin-bottom panels the
  title comes from <code>arbor.ui.set_panel_content(id, &#123;title, â€¦&#125;)</code>;
  the close button is wired automatically and clears the active bottom
  section. You don't render this chrome yourself â€” only the body content.
</p>
<pre class="language-lua">{@html highlight(`-- Register the panels once at plugin load.
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
end)`, '.lua')}</pre>

<h3>Supported form-DSL nodes in sidebars</h3>
<p>
  The sidebar renderer is intentionally lightweight â€” it handles the shapes
  common to dashboards and launchers. Rich editing (<code>tree_layout</code>,
  <code>pipeline_editor</code>, wizards) still belongs in modals opened via
  <code>arbor.ui.form</code>. Nodes are rendered <strong>recursively</strong>
  â€” a <code>section</code> can contain <code>list</code>, <code>row</code>,
  nested <code>section</code>, etc. at arbitrary depth.
</p>
<ul>
  <li><code>heading</code> â€” <code>&#123; type="heading", text="â€¦" &#125;</code></li>
  <li><code>label</code> / <code>paragraph</code> â€” plain text (sidebar uses the <code>text</code> field, not <code>content</code>)</li>
  <li><code>divider</code> â€” horizontal rule</li>
  <li><code>button</code> â€” <code>&#123; type="button", label?, icon?, icon_only?, variant?, disabled?, tooltip?, action, id &#125;</code>. Variants: <code>default</code> / <code>ghost</code> / <code>primary</code> / <code>danger</code>. <code>icon_only = true</code> renders a square 24Ã—24 button.</li>
  <li><code>row</code> â€” <code>&#123; type="row", gap?, children[] &#125;</code>. Inline flex, wraps when narrow. Use to lay out inline icon-button toolbars.</li>
  <li><code>list</code> â€” <code>&#123; type="list", items=[&#123;id,label,icon?,detail?,action?&#125;â€¦] &#125;</code>. A per-item <code>action</code> fires when the row is clicked; the row receives <code>&#123;id, value, label&#125;</code> in the action context.</li>
  <li><code>section</code> â€” grouped container with optional <code>title</code> and nested <code>nodes</code>. Children render through the full node renderer, so every node type above is available inside.</li>
  <li><code>card_item</code> â€” MR/Reflog-style list row. Fields: <code>id</code>, <code>icon</code>, <code>icon_variant</code> (accent/success/warning/danger), <code>title</code>, <code>subtitle</code>, <code>badge</code> (small chip, top-right of title), <code>meta</code> (<code>[&#123;text, variant&#125;]</code> chips below), <code>action</code> (primary click on the whole row), <code>actions</code> (<code>[&#123;icon, tooltip, variant, action, extra&#125;]</code> hover-revealed icon buttons on the right), <code>tooltip</code>. Use for dense clickable lists that also need secondary per-row actions.</li>
</ul>
<pre class="language-lua">{@html highlight(`-- Example: a sequences-like list where primary click runs, secondary
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
})`, '.lua')}</pre>
<p>
  <code>set_panel_content</code> also accepts a top-level <code>actions = [&#123;label, action, icon?&#125;â€¦]</code>
  array that renders as full-width footer buttons below the body.
</p>

<h2>Tree-kind sidebars (contribution model)</h2>
<p>
  A <code>kind = "tree"</code> sidebar exposes a tree-of-nodes UI (header
  toolbar, scrollable body, optional footer) and lets <em>other plugins</em>
  extend it through named contribution points. The host plugin owns the tree
  data and the extension contract; consumers push items into the points and
  the same component renders both. This is the pattern used by the built-in
  <code>compile-action</code> sidebar, where <code>run-action</code>
  contributes its "Run configurations" section and per-row Tomcat actions
  without <code>compile-action</code> knowing about run.
</p>

<h3>1. Register the sidebar</h3>
<pre class="language-lua">{@html highlight(`arbor.ui.add_sidebar({
  id          = "compile",        -- panel id (namespaced as <plugin>:<id>)
  label       = "Build & Run",
  icon        = "Hammer",
  side        = "right",
  position    = "top",
  kind        = "tree",            -- â† opt into the tree renderer
})`, '.lua')}</pre>

<h3>2. Push the tree</h3>
<p>
  Call <code>arbor.ui.tree.set(sidebar_id, body)</code> on every state change
  (typically from <code>on_repo_open</code> / <code>on_tab_switch</code>).
  Each node is shaped like:
</p>
<pre class="language-lua">{@html highlight(`{
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
}`, '.lua')}</pre>

<h3>3. Declare contribution points</h3>
<p>
  Convention: name points <code>&lt;plugin&gt;:&lt;sidebar_id&gt;:&lt;slot&gt;</code>.
  The frontend reads the following slots automatically â€” declare them so
  consumers (and the docs) know they exist:
</p>
<table class="shortcuts-table">
  <thead><tr><th>Slot</th><th>Renders</th><th>Payload shape</th></tr></thead>
  <tbody>
    <tr><td><code>toolbar</code></td><td>Buttons in the panel header</td><td><code>&#123;icon, tooltip, action, accent?, success?, danger?, divider_before?, disabled?&#125;</code></td></tr>
    <tr><td><code>tree.section</code></td><td>Top-level section appended to the tree</td><td><code>&#123;section = &lt;TreeNode&gt;&#125;</code></td></tr>
    <tr><td><code>node_action</code></td><td>Hover-revealed icon button on each row</td><td><code>&#123;icon, tooltip, action, accent?|success?|danger?, when?&#125;</code></td></tr>
    <tr><td><code>node_decorator</code></td><td>Always-on badge / icon between label and actions</td><td><code>&#123;icon?, badge?, badge_kind?, tooltip?, when?&#125;</code></td></tr>
    <tr><td><code>context_menu</code></td><td>Right-click menu items</td><td><code>&#123;label, action, danger?, separator?, when?&#125;</code></td></tr>
    <tr><td><code>dependency_provider</code></td><td>Auto-injects "Show dependencies" in the right-click menu when the node matches</td><td><code>&#123;label, action, when?&#125;</code> â€” handler writes results via <code>tree.set(request_id, â€¦)</code></td></tr>
    <tr><td><code>footer</code></td><td>Items in the panel footer</td><td><code>&#123;kind="text"|"button", icon?, label?, action?, badge?&#125;</code></td></tr>
  </tbody>
</table>
<p>
  The <code>when</code> filter narrows a contribution to specific nodes:
  <code>&#123;kind = "module"&#125;</code>, <code>&#123;kind = ["module","runnable"]&#125;</code>,
  or <code>&#123;kind = "module", data_field = &#123;key = "template_id", value = "maven"&#125;&#125;</code>.
  Omit <code>when</code> to apply to every node.
</p>

<h3>4. Contribute from another plugin</h3>
<pre class="language-lua">{@html highlight(`-- maven-update-deps / main.lua
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
end)`, '.lua')}</pre>

<p>
  Re-call <code>arbor.ui.contribute</code> with the same <code>id</code> to
  replace the previous payload â€” useful when your contribution depends on the
  active repo (e.g. a tree section whose contents change per tab). Use
  <code>arbor.ui.unregister_contribution(point, id)</code> to remove it.
</p>

<h3>Custom icons</h3>
<p>
  When a Lucide name doesn't fit, register a raw SVG and reference it from
  any <code>icon</code> field as <code>"plugin:&lt;your_plugin&gt;:&lt;id&gt;"</code>.
  The SVG should use <code>currentColor</code> for stroke / fill so it picks
  up the surrounding text color.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.icon.register({
  id  = "my-logo",
  svg = '<svg viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg">'
     .. '  <path stroke="currentColor" fill="none" stroke-width="1.5" '
     ..       'd="M2 8 L8 2 L14 8 L8 14 Z"/>'
     .. '</svg>',
})

-- then in any tree node / contribution:
icon = "plugin:my-plugin:my-logo"`, '.lua')}</pre>

<h3>Dependency tree modal</h3>
<p>
  Right-clicking a tree row auto-injects a <em>Show dependencies</em> entry
  whenever a <code>dependency_provider</code> contribution matches the node
  (via its <code>when</code> filter). Selecting it opens the
  <code>DependencyTreeModal</code> and fires the provider's
  <code>action</code> with <code>&#123;request_id, node_id, data&#125;</code>.
  The provider's job is to populate <code>arbor.ui.tree.set(request_id, &#123;title, nodes&#125;)</code>
  â€” the modal subscribes to that snapshot id and renders the result.
</p>

<h3>Dependency Explorer modal (deps-explorer plugin)</h3>
<p>
  Same transport, richer UI: the <code>deps-explorer</code> plugin opens an
  IntelliJ-style two-pane modal (resolved deps on the left, usages of the
  selected artifact on the right, with scope / outdated / conflict filters)
  by pushing snapshots under the dedicated sidebar id prefix
  <code>deps:&lt;request_id&gt;</code>. The frontend store
  <code>depsExplorerStore</code> filters the unified
  <code>arbor://contributions-changed</code> event for
  <code>point="arbor:tree-state"</code>, recognises the prefix and pops the
  modal up; subsequent updates with the
  same id patch the open modal reactively (used to attach Maven Central
  latest-version data after the initial tree lands). The pattern is reusable
  for any plugin that wants a dedicated modal â€” pick a unique sidebar-id
  prefix for the plugin and add a small store + listener.
</p>
<pre class="language-lua">{@html highlight(`-- Open the modal immediately with a "loading" snapshot.
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
&#125;)`, '.lua')}</pre>

<h2>Containers (aggregated modals)</h2>
<p>
  A <strong>container</strong> is an aggregated UI surface â€” currently a modal â€”
  whose body is built from cross-plugin contributions. The host registers the
  container; anyone (the host or a third party) contributes <em>categories</em>
  (left sidebar entries) and <em>sections</em> (right pane cards). Each section
  is rendered as its own <code>FormNodeRenderer</code> and saves in parallel.
</p>
<p>
  Two layers compose every container:
</p>
<table class="shortcuts-table">
  <thead><tr><th>API</th><th>Purpose</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.ui.container.register(opts)</code></td><td>Declare the container shell. Returns immediately; the modal opens lazily on <code>open()</code>.</td></tr>
    <tr><td><code>arbor.ui.container.open(key)</code></td><td>Show the modal. <code>key</code> is the canonical <code>"&lt;plugin&gt;::&lt;id&gt;"</code> id.</td></tr>
    <tr><td><code>arbor.ui.container.close(key)</code></td><td>Dismiss it. Mismatched keys are ignored so a plugin can't close another's modal.</td></tr>
    <tr><td><code>arbor.ui.contribute("&lt;plugin&gt;::&lt;id&gt;:category", item)</code></td><td>Add a sidebar entry. Payload: <code>&#123;label, icon?, description?, priority?&#125;</code>.</td></tr>
    <tr><td><code>arbor.ui.contribute("&lt;plugin&gt;::&lt;id&gt;:section", item)</code></td><td>Add a section card. Payload: <code>&#123;category, label?, icon?, nodes, on_save?, state?, priority?&#125;</code>.</td></tr>
  </tbody>
</table>
<p>
  <code>register</code> accepts <code>&#123;id, title, kind?, layout?, width?, submit_label?, cancel_label?, on_save?, on_load?&#125;</code>.
  <code>on_load</code> fires <strong>once when the modal opens</strong>, before
  categories/sections are read â€” use it to re-contribute fresh state. The
  contribution registry is reactive, so contributions arriving from
  <code>on_load</code> appear without a second round-trip.
</p>
<p>
  Save semantics are <strong>parallel best-effort</strong>: each section's
  <code>on_save</code> fires concurrently (Promise.allSettled), failures are
  aggregated into a single toast, and the host's <code>on_save</code> (if set)
  fires last with the full namespaced state
  <code>&#123;sections = &#123;[plugin] = &#123;[field] = value&#125;&#125;&#125;</code>.
  Field-name collisions across sections of <em>different</em> plugins are
  prevented by a backend rewrite: every form-DSL field name is silently
  prefixed with <code>&lt;contributing-plugin&gt;::</code> when the section
  is contributed, and the prefix is stripped from each plugin's slice on
  save. Plugin code never sees the namespaced names â€” the rewrite is
  transparent. Collisions across sections of the <em>same</em> plugin
  still overwrite by last-writer (use unique field names within your own
  sections).
</p>

<h3>Host registers a container</h3>
<pre class="language-lua">{@html highlight(`arbor.ui.container.register({
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

arbor.ui.container.open("my-plugin::main")`, '.lua')}</pre>

<h2>Plugin settings â€” sugar over containers</h2>
<p>
  <code>arbor.ui.settings.*</code> is sugar over the container API for the
  conventional "plugin settings" surface. The wrapper:
</p>
<ul>
  <li>Registers a container with <code>kind = "modal"</code>, <code>layout = "tree_nav"</code>.</li>
  <li>Forces the category / section sub-points to the historical naming
      <code>&lt;plugin&gt;:settings:category</code> and
      <code>&lt;plugin&gt;:settings:section</code> (single colon between
      <code>plugin</code> and <code>settings</code>) â€” so plugins extending a
      host's settings panel use the natural compact name.</li>
  <li>Discovers panels via the container registry â€” the gear icon in Plugin
      Manager appears whenever a plugin owns at least one container.</li>
</ul>
<table class="shortcuts-table">
  <thead><tr><th>Point</th><th>Payload shape</th></tr></thead>
  <tbody>
    <tr><td><code>&lt;host&gt;:settings:category</code></td><td><code>&#123;label, icon?, description?, priority?&#125;</code> â€” sidebar entry.</td></tr>
    <tr><td><code>&lt;host&gt;:settings:section</code></td><td><code>&#123;category, label?, icon?, nodes, on_save?, priority?&#125;</code> â€” content card. <code>category</code> selects which sidebar entry the card belongs to.</td></tr>
  </tbody>
</table>
<p>
  Anyone can contribute to either point. External plugins can (a) add a new
  sidebar entry, (b) drop a card into an existing entry, or (c) replace an
  existing card by re-contributing with the same <code>id</code>.
</p>

<h3>1. Host: register the panel + categories</h3>
<pre class="language-lua">{@html highlight(`-- Once at PLUGIN_LOAD. All calls are idempotent.
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
end`, '.lua')}</pre>

<h3>2. Host: contribute sections (cards) into its categories</h3>
<pre class="language-lua">{@html highlight(`local function build_general_card()
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
end)`, '.lua')}</pre>

<h3>3. Host: refresh on open</h3>
<pre class="language-lua">{@html highlight(`-- on_load fires once when the modal opens. Re-contribute the cards so
-- toolchain lists, run configurations, etc. reflect what is on disk.
arbor.events.on("my_plugin:settings_refresh", function(_ctx)
  arbor.ui.contribute("my-plugin:settings:section", {
    id = "general-core",
    payload = { category = "general", label = "Core",
                nodes = build_general_card(), on_save = "my_plugin:save_general" },
  })
end)`, '.lua')}</pre>

<h3>4. External plugin: extend an existing panel</h3>
<pre class="language-lua">{@html highlight(`-- "extras-plugin" adds a brand-new sidebar entry to my-plugin's panel.
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
})`, '.lua')}</pre>

<h3>5. Open the panel programmatically</h3>
<pre class="language-lua">{@html highlight(`arbor.ui.settings.open("my-plugin", "main")
arbor.ui.settings.close()  -- close whatever is open`, '.lua')}</pre>

<h3>Cross-plugin reads</h3>
<p>
  Any plugin can read its own settings via <code>arbor.settings.global.get</code>.
  To read <em>another</em> plugin's settings, set
  <code>settings_read_others = true</code> in <code>[permissions]</code> and
  call <code>arbor.settings.read("other-plugin", "key")</code> /
  <code>arbor.settings.read_project("other-plugin", "key")</code>. Cross-plugin
  <strong>writes</strong> stay restricted: the target plugin must opt in by
  exporting a service via <code>arbor.service.export</code>, which the caller
  then invokes through <code>arbor.service.call</code>.
</p>
<pre class="language-lua">{@html highlight(`-- in extras-plugin, after settings_read_others = true:
local mode = arbor.settings.read("my-plugin", "mode") or "balanced"`, '.lua')}</pre>

<p>
  The Plugin Manager also exposes a <strong>Clear cache</strong> button
  (two-click confirmation) that wipes a plugin's <code>global.json</code>.
</p>

<h2>Form node types</h2>
<table class="shortcuts-table">
  <thead><tr><th>type</th><th>Key fields</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td><code>text</code></td><td>name, label, placeholder, default, pattern, pattern_hint, readonly</td><td>Also: password, email, url</td></tr>
    <tr><td><code>textarea</code></td><td>name, label, placeholder, default, rows</td><td></td></tr>
    <tr><td><code>number</code></td><td>name, label, default, min, max, step</td><td></td></tr>
    <tr><td><code>range</code></td><td>name, label, default, min, max, step, show_value, value_format</td><td>value_format: "&#123;v&#125;ms"</td></tr>
    <tr><td><code>checkbox</code></td><td>name, label, default</td><td></td></tr>
    <tr><td><code>toggle</code></td><td>name, label?, description?, default, size (sm/md/lg)</td><td>iOS-style switch. Use for "feature on/off"; use <code>checkbox</code> for "I agree"</td></tr>
    <tr><td><code>select</code></td><td>name, label, default, options[]</td><td>options: value+label+disabled?</td></tr>
    <tr><td><code>radio</code></td><td>name, label, default, options[], inline</td><td>options: value+label+description?</td></tr>
    <tr><td><code>color</code></td><td>name, label, default (#rrggbb)</td><td></td></tr>
    <tr><td><code>kv_list</code></td><td>name, label, key_placeholder, value_placeholder, default</td><td>Submitted as JSON object</td></tr>
    <tr><td><code>section</code></td><td>title, description, children[], collapsible, collapsed</td><td>Layout only</td></tr>
    <tr><td><code>container</code></td><td>children[], columns, gap</td><td>CSS grid</td></tr>
    <tr><td><code>row</code></td><td>children[], gap, align, wrap</td><td>Flexbox row</td></tr>
    <tr><td><code>separator</code></td><td>label?</td><td>Labelled divider line</td></tr>
    <tr><td><code>divider</code></td><td>â€”</td><td>Plain &lt;hr&gt;</td></tr>
    <tr><td><code>paragraph</code></td><td>content, variant (normal/muted/heading/caption)</td><td></td></tr>
    <tr><td><code>label</code></td><td>text, variant</td><td>Static text alias</td></tr>
    <tr><td><code>alert</code></td><td>text, variant (info/warning/error/success)</td><td></td></tr>
    <tr><td><code>code</code></td><td>text, language?, copy?, toast?</td><td>Read-only monospace block. When <code>language</code> matches a Prism grammar (<code>"json"</code>, <code>"rust"</code>, <code>"yaml"</code>, â€¦) the block is syntax-highlighted using the same Prism setup as the diff viewer. <code>copy: true</code> shows a floating Copy button; <code>toast</code> overrides the success toast.</td></tr>
    <tr><td><code>icon</code></td><td>icon (Lucide name), variant (default/muted/info/success/warning/danger), size, tooltip, class, style</td><td>Inline Lucide glyph for status dots / badges. <code>Loader2</code> auto-spins via CSS.</td></tr>
    <tr><td><code>copy_link</code></td><td>text, toast?, tooltip?, font (normal/"mono"), class, style</td><td>Click-to-copy pseudo-link with a subtle <code>Copy</code> glyph on the right. Calls <code>navigator.clipboard</code> directly â€” no plugin action hop. Ideal for paths, IDs, URLs.</td></tr>
    <tr><td><code>button</code></td><td>label?, action, variant (default/primary/danger/ghost), close_after, disabled, icon, icon_only, tooltip, extra, class</td><td>Inline action; <code>icon</code> is a Lucide name, <code>icon_only</code> renders without label, <code>extra</code> merges into the action payload. Pass <code>class = "pal-row"</code> for a tight flush-left catalog-row style.</td></tr>
    <tr><td><code>menu_button</code></td><td>label?, icon, icon_only, tooltip, show_chevron, options[]</td><td>Opens a dropdown menu. Each option: <code>&#123; label?, icon?, action?, extra?, variant?, disabled?, heading?, separator? &#125;</code></td></tr>
    <tr><td><code>date</code></td><td>name, label, default, min, max, readonly, required</td><td>Submits ISO "YYYY-MM-DD"</td></tr>
    <tr><td><code>datetime</code></td><td>name, label, default, min, max, readonly, required</td><td>Submits "YYYY-MM-DDTHH:MM" (local, no TZ)</td></tr>
    <tr><td><code>time</code></td><td>name, label, default, min, max, readonly, required</td><td>Submits "HH:MM"</td></tr>
    <tr><td><code>switch</code></td><td>field, cases, default</td><td>Renders one branch based on another field's value</td></tr>
    <tr><td><code>tabs</code></td><td>tabs[], default_tab</td><td>Tab strip; all fields inside always collected for submit</td></tr>
    <tr><td><code>wizard</code></td><td>steps[], start_step, next_label, back_label</td><td>Multi-step form with Back/Next footer</td></tr>
    <tr><td><code>file</code></td><td>name, label, pick_mode, extensions, placeholder</td><td>Opens FilePickerModal â€” submits path string</td></tr>
    <tr><td><code>autocomplete</code></td><td>name, id, options?, source_action?, debounce_ms, free_form</td><td>Static or dynamic suggestions</td></tr>
    <tr><td><code>tags</code></td><td>name, default, suggestions, max</td><td>Submits <code>string[]</code></td></tr>
    <tr><td><code>tree</code></td><td>name, nodes[], multi, expanded, bordered, max_height</td><td>Hierarchical selector. Nodes: <code>value, label, icon?, group?, tag?, tag_variant?, description?, children?</code></td></tr>
    <tr><td><code>table</code></td><td>name, columns[], min_rows, max_rows, add_label</td><td>Submits <code>Array&lt;Record&gt;</code></td></tr>
    <tr><td><code>tree_layout</code></td><td>nav_children[], content_children[], nav_width</td><td>2-col split (nav + content). Typical use: tree on the left, form cards on the right gated with <code>show_if</code></td></tr>
    <tr><td><code>section</code></td><td>title, description, children[], collapsible, collapsed, card, count, add_action, header_actions[], class</td><td><code>card = true</code> renders with dark title bar + counter pill + optional + button. <code>collapsible = true</code> toggles the body. <code>header_actions</code>: <code>&#123; icon, tooltip, action, extra, disabled, variant &#125;[]</code> â€” icon buttons in the header; <code>variant = "danger"</code> applies the red hover. <code>class = "pf-card-compact"</code> tightens body padding for dense list-mode cards.</td></tr>
    <tr><td><code>card_row</code></td><td>label, description, children[]</td><td>Two-column label + controls row inside a <code>section</code> card</td></tr>
    <tr><td><code>form_field</code></td><td>label?, optional_text?, required?, description?, hint?, error?, icon?, actions[]?, children[], for?</td><td>Vertical labeled wrapper â€” same look as the host's <code>&lt;FormField&gt;</code> widget. Wrap any nodes with the standard arbor field chrome (label on top, content below, optional hint/error/right-aligned actions). <code>icon</code> is a Lucide name; <code>actions</code> render right-aligned on the label row (typically <code>button</code> nodes).</td></tr>
    <tr><td><code>cfg_list</code></td><td>items[]</td><td>Item rows with active dot + tags + hover edit/delete. Item: <code>&#123; id, label, active?, tags?, edit_action?, delete_action? &#125;</code></td></tr>
    <tr><td><code>suggest_grid</code></td><td>items[]</td><td>2-col grid of suggestion cards. Item: <code>&#123; name, cmd?, tag?, action? &#125;</code></td></tr>
    <tr><td><code>counter_grid</code></td><td>items[], min_width?, gap?, padding?, actions.select?</td><td>Responsive KPI tile grid. Item: <code>&#123; key, label, value, hint?, color?, icon?, empty? &#125;</code>. <code>actions.select</code> fires <code>&#123; key &#125;</code> when a non-empty tile is clicked. <code>color</code> accepts any CSS expression â€” <code>"var(--severity-high)"</code>, <code>"#f97316"</code>.</td></tr>
    <tr><td><code>score_gauge</code></td><td>value, min, max, segments[], label, size, value_color</td><td>Semi-circle gauge for a bounded value. Segment: <code>&#123; from, to, color &#125;</code>. <code>size</code>: <code>"sm" | "md" | "lg"</code> (default <code>"md"</code>). Display only.</td></tr>
    <tr><td><code>time_series_chart</code></td><td>series[], x_kind, height, show_legend, y_include_zero</td><td>Multi-series line chart with hover tooltip + legend. Series: <code>&#123; id, label, color, points: [&#123; x, y &#125;] &#125;</code>. With <code>x_kind = "time"</code> (default), <code>x</code> is an ISO-8601 string; with <code>"linear"</code> it's a number.</td></tr>
    <tr><td><code>data_table</code></td><td>columns[], rows[], row_key?, height?, initial_sort?, empty?, actions.row_click?</td><td>Sortable / clickable table. Column: <code>&#123; key, label, width?, align?, kind?, color?, sortable? &#125;</code> with <code>kind âˆˆ &#123; "text", "code", "pill", "datetime", "age" &#125;</code>. Row colour override: <code>_&lt;key&gt;_color</code>. <code>actions.row_click</code> fires <code>&#123; row_id, row &#125;</code>.</td></tr>
    <tr><td><code>filter_bar</code></td><td>name?, default?, search?, filters[], padding?, actions.change?</td><td>Search input + N chip dropdowns. Filter: <code>&#123; id, label, icon?, options[&#123; value, label, color? &#125;], mode?, searchable?, wide? &#125;</code> with <code>mode âˆˆ &#123; "single", "multi" &#125;</code> (default <code>"multi"</code>). When <code>name</code> is set the value <code>&#123; search, filters: &#123; [id]: string[] &#125; &#125;</code> is collected into form values; <code>actions.change</code> fires <code>&#123; value &#125;</code> on every keystroke / chip toggle. Set <code>search = nil</code> to omit the search input.</td></tr>
  </tbody>
</table>

<p>Top-level <code>arbor.ui.form(config)</code> options: <code>title</code>, <code>description</code>, <code>submit_label</code>, <code>submit_action</code>, <code>cancel_label</code>, <code>cancel_action</code>, <code>hide_submit</code>, <code>hide_cancel</code>, <code>width</code>, <code>height</code>, <code>sidebar</code> (two-column nav layout when the root is a <code>tabs</code> node), <code>state</code>, <code>css</code>, <code>loading</code>.</p>
<p>
  <code>loading = true</code> renders a translucent overlay with a centered
  spinner above the form body â€” use it while the plugin fans out to the
  network after opening the modal (e.g. fetching per-repo data before the
  dashboard has anything to draw). Toggle it live by passing
  <code>loading</code> alongside <code>nodes</code> to
  <code>arbor.ui.form.replace</code>: <code>arbor.ui.form.replace(&#123; loading = false, nodes = ... &#125;)</code>.
</p>
<p>
  <code>hide_submit</code> / <code>hide_cancel</code> drop the matching footer
  button entirely â€” useful for read-only modals (show one single
  <em>Close</em> button) or confirmation dialogs where only Submit makes
  sense. Keyboard Escape still closes the modal regardless of which buttons
  are visible.
</p>

<h2>Builder DSL â€” chainable form construction</h2>
<p>
  As an alternative to the table-config call, <code>arbor.ui.form()</code> (no
  argument) and <code>arbor.ui.form("id")</code> return a chainable
  <code>FormBuilder</code>. Every method returns the builder itself, so you can
  pipe a form together one node at a time and finalise with <code>:open()</code>.
  Calling <code>arbor.ui.form(table)</code> with a config table still works
  exactly as before â€” the builder is purely sugar.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form()
  :title("Inspect Commit")
  :description("Add a personal note for this commit.")
  :state({ oid = ctx.oid })
  :textarea("note", { label = "Note", placeholder = "What's interesting?", rows = 3 })
  :text("tag",      { label = "Tag",  placeholder = "e.g. fix, refactor" })
  :checkbox("bookmark", { label = "Bookmark this commit" })
  :submit("Save Note", "inspect:save_note")
  :on_cancel("inspect:cancel_note")
  :open()`, '.lua')}</pre>
<p>
  Each field method takes <code>(name, opts?)</code> or a single
  <code>&#123;name = ..., ...&#125;</code> table. Sections auto-close on the next
  <code>:section()</code> call, so flat layouts read naturally; use
  <code>:end_section()</code> to drop back to the top level explicitly.
  <code>:field(node)</code> is the escape hatch â€” push any node table that
  the field helpers don't cover (<code>tabs</code>, <code>tree_layout</code>,
  <code>cfg_list</code>, etc.).
</p>
<table class="shortcuts-table">
  <thead><tr><th>Method</th><th>Effect</th></tr></thead>
  <tbody>
    <tr><td><code>:title(s)</code> Â· <code>:description(s)</code></td><td>Modal header</td></tr>
    <tr><td><code>:submit(action)</code> Â· <code>:submit(label, action)</code></td><td>Sets <code>submit_action</code> (and <code>submit_label</code> when both args supplied)</td></tr>
    <tr><td><code>:on_submit(action)</code></td><td>Sets <code>submit_action</code> only</td></tr>
    <tr><td><code>:cancel(action)</code> Â· <code>:cancel(&#123;label, action&#125;)</code></td><td>Cancel action / label</td></tr>
    <tr><td><code>:on_cancel(action)</code></td><td>Sets <code>cancel_action</code> only</td></tr>
    <tr><td><code>:state(t)</code></td><td>Echo state forwarded back in the submit ctx</td></tr>
    <tr><td><code>:section(title|cfg)</code> Â· <code>:end_section()</code></td><td>Open / close a flat section. Re-calling <code>:section()</code> auto-closes the previous one.</td></tr>
    <tr><td><code>:text</code> Â· <code>:textarea</code> Â· <code>:password</code> Â· <code>:number</code></td><td>Input fields. Args: <code>(name, opts?)</code> or <code>&#123;name=..., ...&#125;</code></td></tr>
    <tr><td><code>:select</code> Â· <code>:radio</code> Â· <code>:checkbox</code> Â· <code>:toggle</code> Â· <code>:kv_list</code></td><td>Choice / boolean / kv inputs</td></tr>
    <tr><td><code>:divider()</code> Â· <code>:label(text|cfg)</code> Â· <code>:paragraph(s)</code> Â· <code>:heading(s)</code></td><td>Static layout nodes</td></tr>
    <tr><td><code>:button(cfg)</code></td><td>Push a button node (<code>&#123;label, icon, action, variant&#125;</code>)</td></tr>
    <tr><td><code>:form_field(label|cfg, cfg?)</code></td><td>Push a <code>form_field</code> wrapper. Two call shapes: <code>:form_field(&#123;label="â€¦", required=true, children=&#123;â€¦&#125;&#125;)</code> or <code>:form_field("Label", &#123;children=&#123;â€¦&#125;, hint="â€¦"&#125;)</code>.</td></tr>
    <tr><td><code>:field(node)</code></td><td>Escape hatch â€” push any node table verbatim</td></tr>
    <tr><td><code>:open()</code></td><td>Compile to a config and emit the form modal</td></tr>
  </tbody>
</table>

<h2>File / folder picker field</h2>
<p>Opens the standard Arbor file picker as a modal on top of the plugin form. <code>pick_mode</code> controls behaviour:</p>
<ul>
  <li><code>"file"</code> â€” select an existing file (default)</li>
  <li><code>"folder"</code> â€” select an existing directory</li>
  <li><code>"save"</code> â€” pick a destination path (typing a new filename is allowed)</li>
</ul>
<pre class="language-lua">{@html highlight(`{ type = "file", name = "output",  label = "Output path",
  pick_mode = "save", extensions = { "pdf" },
  placeholder = "Choose a fileâ€¦" }

{ type = "file", name = "repo_dir", label = "Repository root",
  pick_mode = "folder" }`, '.lua')}</pre>

<h2>Autocomplete field</h2>
<p>
  Two modes. <strong>Static:</strong> plugin provides an <code>options</code> list and the form filters locally with fuzzy scoring. <strong>Dynamic:</strong> plugin sets <code>source_action</code> and replies to each keystroke with a fresh suggestion list via <code>arbor.ui.set_autocomplete_options(id, options)</code>.
</p>
<pre class="language-lua">{@html highlight(`-- Static list
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
end)`, '.lua')}</pre>

<h2>Tags / chips field</h2>
<p>Multi-value free-form input. Press <kbd>Enter</kbd> or <kbd>,</kbd> to commit a tag; <kbd>Backspace</kbd> with an empty input removes the last chip. Set <code>suggestions</code> to restrict input to an allowlist (acts like a multi-select).</p>
<pre class="language-lua">{@html highlight(`{ type = "tags", name = "labels", label = "Labels",
  default = { "bug", "priority-1" },
  suggestions = { "bug", "enhancement", "question", "priority-1", "priority-2" },
  max = 5 }`, '.lua')}</pre>

<h2>Tree selector field</h2>
<p>Hierarchical picker for one value (<code>multi = false</code>, default) or many (<code>multi = true</code> â€” submitted as <code>string[]</code>). Set <code>group = true</code> on a node to make it a non-selectable header (still expandable and clickable-to-toggle). Each node supports:</p>
<ul>
  <li><code>value</code>, <code>label</code> â€” required</li>
  <li><code>icon</code> â€” Lucide name shown before the label</li>
  <li><code>tag</code> â€” small colored pill after the label (e.g. <code>"Tomcat"</code>)</li>
  <li><code>tag_variant</code> â€” <code>neutral | ok | warn | error | accent | dev | prod | test</code></li>
  <li><code>description</code> â€” dim subtitle under the label</li>
  <li><code>children</code> â€” nested array of same shape</li>
</ul>
<p>The tree itself is <strong>flush by default</strong> (no border, no background, no max-height) so it blends into its container â€” ideal inside a <code>tree_layout</code> nav. Opt in to the legacy bordered look via <code>bordered = true</code> and optionally cap scroll with <code>max_height</code>.</p>
<pre class="language-lua">{@html highlight(`{ type = "tree", name = "sel_cfg", expanded = true, default = "cfg-1",
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
{ type = "tree", name = "tags_tree", multi = true, nodes = { --[[ ... ]] } }`, '.lua')}</pre>

<h2>FormField wrapper</h2>
<p>
  <code>form_field</code> wraps any nodes with the same chrome host modals use
  for native form fields: label on top, content below, optional description
  between, hint or error underneath, leading icon, and right-aligned actions on
  the label row. The built-in input types (<code>text</code>, <code>select</code>, â€¦)
  already render their own label â€” reach for <code>form_field</code> when you
  need to label non-field content (<code>button</code>, <code>copy_link</code>,
  a row of mixed controls), enrich a single field with affordances the type
  doesn't expose (icon, action button next to the label), or surface a
  computed error/hint that doesn't come from per-field validation.
</p>
<pre class="language-lua">{@html highlight(`-- Wrap a copy_link with a labeled chrome
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
  :open()`, '.lua')}</pre>

<h2>IntelliJ-style tree layout</h2>
<p>
  <code>tree_layout</code> is a 2-column container: navigation (typically a toolbar + a tree) on the left, content on the right. Pair it with <code>show_if</code> on per-item sections and a tree selection stored in <code>values[name]</code> to get an IntelliJ-style run/debug configurations modal.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
})`, '.lua')}</pre>
<p>
  When a <code>tree_layout</code> is the sole root of a form, the body automatically strips its padding so the split reaches the modal edges (IntelliJ look). Combine with an always-unique <code>id</code> on each node to keep Svelte's diff efficient across <code>arbor.ui.form.replace(...)</code> calls.
</p>

<h2>Dashboard widgets â€” generic, reusable</h2>
<p>
  Four leaf nodes turn the host's dashboard primitives into form-renderable
  layout. They are <strong>generic</strong> â€” no domain coupling â€” so any plugin
  can compose its own dashboard by combining counter tiles, a gauge, a time-series
  chart, and a sortable table without writing custom Svelte.
</p>

<h3>counter_grid</h3>
<p>
  Responsive grid of KPI tiles. Each tile shows a label, a primary value,
  and an optional sub-line. Tiles with <code>empty = true</code> (or a numeric
  <code>value</code> of zero) render dimmed and ignore clicks. Click a non-empty
  tile to fire <code>actions.select</code> with <code>&#123; key &#125;</code>.
</p>
<pre class="language-lua">{@html highlight(`{ type = "counter_grid",
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
}`, '.lua')}</pre>

<h3>score_gauge</h3>
<p>
  Semi-circle gauge for a single bounded value. Coloured <code>segments</code>
  define the band palette; the needle rotates to the interpolated value.
  Display only â€” no actions.
</p>
<pre class="language-lua">{@html highlight(`{ type = "score_gauge",
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
}`, '.lua')}</pre>

<h3>time_series_chart</h3>
<p>
  Multi-series line chart with hover-tooltip and an interactive legend. Each
  series is rendered as a coloured polyline; the x-axis is time-aware by default
  (parses <code>x</code> as ISO-8601). Set <code>x_kind = "linear"</code> for
  numeric x.
</p>
<pre class="language-lua">{@html highlight(`{ type = "time_series_chart",
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
}`, '.lua')}</pre>

<h3>data_table</h3>
<p>
  Sortable, optionally clickable table. Columns control rendering via
  <code>kind</code>: <code>text</code> (default), <code>code</code> (monospace),
  <code>pill</code> (coloured chip), <code>datetime</code> (locale string),
  <code>age</code> (compact d/mo/y). <code>color</code> on the column tints the
  cell â€” for <code>pill</code> kind it sets the chip background, for any other
  kind it tints the text (zeros and empty cells stay un-tinted, so a "0
  critical" reading doesn't shout in red). A per-row override
  <code>_&lt;column.key&gt;_color</code> takes precedence. Sorting is
  client-side on the column's <code>sortable</code> flag. Row click fires
  <code>actions.row_click</code> with <code>&#123; row_id, row &#125;</code>.
</p>
<pre class="language-lua">{@html highlight(`{ type = "data_table",
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
}`, '.lua')}</pre>

<h3>filter_bar</h3>
<p>
  Pairs naturally with <code>data_table</code>: a search input plus N chip
  dropdowns whose state echoes back to the plugin on every change. When
  <code>name</code> is set, the value
  (<code>&#123; search, filters: &#123; [id]: string[] &#125; &#125;</code>) is
  collected into the form's submit payload and survives <code>form.replace</code>
  rebuilds. <code>actions.change</code> fires in real time with
  <code>&#123; value &#125;</code> so the plugin can re-filter rows and call
  <code>arbor.ui.form.replace</code> with the new <code>data_table</code> rows.
</p>
<pre class="language-lua">{@html highlight(`{ type    = "filter_bar",
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
}`, '.lua')}</pre>
<p>
  Set <code>search = nil</code> to omit the search input and render a chip-only
  bar. Filters default to multi-select; pass <code>mode = "single"</code> on a
  filter to make it radio-like (selecting one option clears the others).
</p>

<p>
  All five widgets are pure leaf nodes â€” they never collect form values
  beyond the optional <code>filter_bar.name</code>, so they can drop anywhere a
  layout node fits (inside <code>tabs</code>, gated by <code>show_if</code>,
  etc.). For interactive dashboards, pair them with
  <code>arbor.ui.form.replace</code> to push fresh data without unmounting the
  modal.
</p>

<h2>Menu button (dropdown)</h2>
<p>
  <code>menu_button</code> renders a button with a dropdown menu anchored below it. Each option fires its own <code>action</code>, optionally merging <code>extra</code> into the payload. Use <code>heading = true</code> for bold non-clickable section labels and <code>separator = true</code> for horizontal rules.
</p>
<pre class="language-lua">{@html highlight(`{ type = "menu_button",
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
}`, '.lua')}</pre>
<p>With <code>icon_only = true</code> the chevron is hidden by default (cleaner toolbar look). Set <code>show_chevron = true</code> explicitly if you want it back.</p>

<h2>Table (multi-column) field</h2>
<p>Editable grid with one row per entry. Submitted as <code>Array&lt;Record&gt;</code> keyed by <code>column.key</code>. Columns support <code>text</code>, <code>number</code>, <code>checkbox</code> and <code>select</code> editors.</p>
<pre class="language-lua">{@html highlight(`{ type = "table", name = "env_rules", label = "Environment rules",
  min_rows = 1, max_rows = 10,
  columns = {
    { key = "pattern", label = "Pattern", width = "2fr", placeholder = "GIT_*" },
    { key = "action",  label = "Action",  width = "140px", type = "select",
      options = { "allow", "deny" } },
    { key = "enabled", label = "On",      width = "60px",  type = "checkbox" },
  },
  default = {
    { pattern = "GIT_*", action = "allow", enabled = true },
  } }`, '.lua')}</pre>

<h2>Wizard multi-step form</h2>
<p>Split a long form into sequential steps. Arbor replaces the Submit button with <kbd>Back</kbd> / <kbd>Next</kbd> while stepping through, and re-enables Submit on the final step. All fields across every step are collected for the final payload â€” moving between steps never loses values.</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
})`, '.lua')}</pre>
<p>Any field node supports <code>show_if</code> for conditional visibility. <code>show_if</code> supports: <code>eq</code>, <code>neq</code>, <code>gt</code>, <code>lt</code>, <code>gte</code>, <code>lte</code>, <code>in</code>/<code>in_values</code>, <code>nin</code>, and logical <code>and</code>/<code>or</code>/<code>not</code>.</p>

<h2>Shortcut options syntax (select / radio)</h2>
<p>For simple cases you can pass <code>options</code> as a plain array of strings. Arbor auto-expands each entry to <code>&#123; value = s, label = s:capitalised() &#125;</code>. This keeps short enum lists readable:</p>
<pre class="language-lua">{@html highlight(`-- Bare-string shortcut
{ type = "select", name = "mode", label = "Mode", options = { "dark", "light", "auto" } }

-- Equivalent full form
{ type = "select", name = "mode", label = "Mode", options = {
    { value = "dark",  label = "Dark"  },
    { value = "light", label = "Light" },
    { value = "auto",  label = "Auto"  },
  }}`, '.lua')}</pre>
<p>Mix-and-match is allowed: a single <code>options</code> array can hold strings and full tables together, so you can upgrade individual entries (to add <code>description</code>, for instance) without rewriting the rest.</p>

<h3>Live change action</h3>
<p>
  Set <code>actions = &#123; change = "&lt;action&gt;" &#125;</code> on a <code>select</code> to fire a plugin
  action on every selection. The handler receives <code>&#123; value &#125;</code> (the chosen option's <code>value</code>)
  alongside the rest of the form's field map. Useful for "filter" or "window picker" controls
  that should re-fetch data immediately rather than waiting for Submit.
</p>
<pre class="language-lua">{@html highlight(`{ type = "select", name = "range_days",
  label   = "Trend window",
  default = "30",
  options = { "30 days", "60 days", "90 days" },
  actions = { change = "dashboard:range_changed" },
}

arbor.events.on("dashboard:range_changed", function(ctx)
    local n = tonumber(ctx.value:match("(%d+)"))
    -- re-fetch with the new range, then arbor.ui.form.replace(...)
end)`, '.lua')}</pre>

<h3>Rich select / multiselect options</h3>
<p>
  In addition to <code>value</code> / <code>label</code>, the <code>select</code> and <code>multiselect</code>
  field types accept <strong>group headers</strong>, <strong>separators</strong>, and per-item visual extras
  (<code>icon</code>, <code>description</code>, <code>meta</code>, <code>disabled</code>). Plain strings and
  the legacy <code>&#123; value, label &#125;</code> shape continue to work â€” these entries are purely additive.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Entry shape</th><th>Effect</th></tr></thead>
  <tbody>
    <tr><td><code>"plain-string"</code></td><td>Auto-expanded to <code>&#123; value = s, label = capitalised(s) &#125;</code>.</td></tr>
    <tr><td><code>&#123; value, label, icon?, description?, meta?, disabled? &#125;</code></td><td>Selectable item. <code>icon</code> is a Lucide name, <code>description</code> renders as a small caption under the label, <code>meta</code> as muted right-aligned text.</td></tr>
    <tr><td><code>&#123; group, items &#125;</code></td><td>Group header â€” <code>items</code> is a nested option list. Optional <code>collapsible = true</code>, <code>default_collapsed = true</code>.</td></tr>
    <tr><td><code>&#123; separator = true, label? &#125;</code></td><td>Decorative separator strip. With a <code>label</code> the strip becomes an uppercase section title.</td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`{ type = "select", name = "config", label = "Run config",
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
}`, '.lua')}</pre>

<h3>multiselect (type = "multiselect")</h3>
<p>
  Same option shape as <code>select</code>, stored as <code>string[]</code>.
  Renders with a checkbox per row; the panel stays open across selections so the
  user can pick several values without re-opening it. Optional <code>min</code> /
  <code>max</code> bounds enable count validation on submit.
</p>
<pre class="language-lua">{@html highlight(`{ type = "multiselect", name = "tags", label = "Tags",
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
end)`, '.lua')}</pre>
<p>
  Both <code>select</code> and <code>multiselect</code> support full keyboard
  navigation (<kbd>â†‘</kbd> <kbd>â†“</kbd> to move, <kbd>Enter</kbd> to pick,
  <kbd>Home</kbd>/<kbd>End</kbd>, <kbd>Esc</kbd> to close) and an optional
  search input that filters by label and description.
</p>

<h2>Date / datetime / time fields</h2>
<p>Native HTML5 pickers wired into the form. Values are submitted as plain strings â€” plugins parse them as needed:</p>
<table class="shortcuts-table">
  <thead><tr><th>type</th><th>Submitted format</th><th>Example</th></tr></thead>
  <tbody>
    <tr><td><code>date</code></td><td>ISO 8601 date</td><td><code>"2026-04-20"</code></td></tr>
    <tr><td><code>datetime</code></td><td>Local datetime, no timezone suffix</td><td><code>"2026-04-20T14:30"</code></td></tr>
    <tr><td><code>time</code></td><td>24-hour time</td><td><code>"14:30"</code></td></tr>
  </tbody>
</table>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
end)`, '.lua')}</pre>
<p><code>min</code> and <code>max</code> accept strings in the same format the field submits.</p>

<h2>Switch / case form nodes</h2>
<p>
  <code>switch</code> branches the form on the current value of another field.
  Use it instead of repeating a <code>show_if</code> cascade when several mutually exclusive fields share a controlling value â€” easier to read and cheaper to maintain.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
})`, '.lua')}</pre>
<p>
  Fields inside every case are <strong>initialised at form-open time</strong>, so switching branches does not lose previously-entered values. Only the fields in the matching branch are rendered and validated.
</p>
<p>
  <strong>Equivalent using show_if (for comparison):</strong>
</p>
<pre class="language-lua">{@html highlight(`-- Verbose alternative â€” one show_if per field per branch.
{ type = "text", name = "maven_goals", show_if = { field = "build_type", eq = "maven"  } },
{ type = "text", name = "gradle_tasks", show_if = { field = "build_type", eq = "gradle" } },
-- ... and so on for every field in every branch.`, '.lua')}</pre>

<h2>Tabs form node</h2>
<p>
  Group related fields into <kbd>Tab</kbd> panels. The strip appears at the top; clicking a tab swaps the visible content. <em>All</em> fields in every tab are always collected on submit â€” inactive tabs are hidden with CSS, not removed from the DOM â€” so you can freely split a large form without worrying about losing values.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
})`, '.lua')}</pre>
<p>
  Supported <code>icon</code> names (Lucide): <code>Settings</code>, <code>Wrench</code>, <code>Cog</code>, <code>Bell</code>, <code>Folder</code>, <code>Package</code>, <code>GitBranch</code>, <code>Play</code>, <code>Code</code>, <code>FileText</code>, <code>Zap</code>, <code>Users</code>, <code>Key</code>, <code>List</code>, <code>AlertTriangle</code>, <code>Info</code>. Omit <code>icon</code> to show a text-only tab. Omit <code>default_tab</code> to open on the first tab.
</p>

<h2>Dynamic form updates</h2>
<p>
  While a form is open, the plugin can mutate individual fields from any handler (button action, bus event, timer, etc.). Calls route via the <code>plugin:form-update</code> Tauri event and are applied only if the currently-open form belongs to the caller plugin â€” cross-plugin updates are silently ignored.
</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
end)`, '.lua')}</pre>
<table class="shortcuts-table">
  <thead><tr><th>Helper</th><th>Applies to</th><th>Notes</th></tr></thead>
  <tbody>
    <tr><td><code>setOptions(name, opts)</code></td><td>select, radio, autocomplete</td><td>Accepts the same <code>options</code> format as at open time (strings or full tables)</td></tr>
    <tr><td><code>setDisabled(name, bool)</code></td><td>text, textarea, number, range, date/time, select, radio, checkbox</td><td>OR'd with the field's own <code>readonly</code> flag</td></tr>
    <tr><td><code>setValue(name, v)</code></td><td>all value-bearing fields</td><td>Also clears the field's inline validation error</td></tr>
    <tr><td><code>replace(cfg)</code></td><td>whole form</td><td>Swaps the root <code>nodes</code> tree in-place â€” no close+reopen flicker. See below.</td></tr>
  </tbody>
</table>
<div class="callout info">
  <strong>Note</strong>
  <code>arbor.ui.form</code> is both a function (open a form) and a table of helpers. The <code>__call</code> metamethod preserves the original <code>arbor.ui.form(config)</code> syntax.
</div>

<h3>arbor.ui.form.replace â€” in-place structural swap</h3>
<p>
  Rebuilds the currently-open form from a new <code>nodes</code> tree without unmounting the modal. Field values whose <code>name</code> still exists are preserved; new fields get their declared defaults; gone fields are discarded. Ideal for IntelliJ-style tree modals where <kbd>+</kbd> / <kbd>âˆ’</kbd> / duplicate must update the nav &amp; content without a flicker.
</p>
<pre class="language-lua">{@html highlight(`-- Payload shape:
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
end)`, '.lua')}</pre>
<p>State preservation rules during a replace:</p>
<ul>
  <li><strong>Values</strong>: by field <code>name</code> â€” present in both â†’ kept; new â†’ default; gone â†’ dropped</li>
  <li><strong>Collapse / tabs / wizard</strong>: by node <code>id</code> â€” present â†’ kept; new â†’ declared collapsed/default</li>
  <li><strong>Tree expansion</strong>: keyed by <code>field::value</code> â€” never cleared</li>
  <li><strong>Validation errors</strong>: referencing a gone field are dropped</li>
</ul>
<p>
  Assign <strong>stable <code>id</code></strong> values to your root container (and to sections you'll add/remove) so Svelte's <code>&#123;#each&#125;</code> diff reuses the DOM across replaces instead of remounting the subtree.
</p>

<h2>Form state â€” opaque context echo</h2>
<p>Pass a <code>state</code> table to <code>form</code> to carry server-side context that isn't rendered in the UI but is echoed back unchanged in every <code>ctx</code> payload (submit, button actions, cancel).</p>
<pre class="language-lua">{@html highlight(`arbor.ui.form({
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
end)`, '.lua')}</pre>
