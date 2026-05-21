<script lang="ts">
  import { highlight } from '$lib/utils/diff-formatter';
</script>

<h1>Plugin Development â€” Toolchains</h1>
<p>The toolchain API manages versioned runtime installations (JDKs, Node.js, Rust toolchains). Entries are stored per-kind at <code>~/.config/arbor/toolchains/&lt;kind&gt;.json</code>. One entry per kind can be marked <em>active</em> â€” it is used automatically when no more specific selection is set.</p>

<h2>Sharing settings between plugins</h2>
<p>Two complementary mechanisms cover cross-plugin settings access:</p>
<ul>
  <li><strong>Cross-plugin reads</strong> â€” declare <code>settings_read_others = true</code> in <code>[permissions]</code> and call <code>arbor.settings.read("other-plugin", "key")</code> / <code>arbor.settings.read_project(...)</code>.</li>
  <li><strong>Cross-plugin writes</strong> â€” the target plugin opts in by exposing a service via <code>arbor.service.export(&#123; name = ..., handler = ... &#125;)</code>; the caller invokes it through <code>arbor.service.call</code>. Writing without consent is not supported.</li>
  <li><strong>Shared settings UI</strong> â€” a member plugin can contribute sections to another plugin's settings panel via <code>arbor.ui.contribute("&lt;owner&gt;:settings:section", ...)</code>. Each plugin still owns its own settings store.</li>
</ul>

<h2>arbor.toolchain â€” runtime toolchains</h2>

<h3>Permissions required</h3>
<ul>
  <li><code>toolchain = "read"</code> â€” for <code>list</code>, <code>active</code>, <code>env</code>, <code>detect</code></li>
  <li><code>toolchain = "write"</code> â€” for <code>add</code>, <code>remove</code>, <code>set_active</code> (implies read)</li>
</ul>

<pre class="language-toml">{@html highlight(`# plugin.toml
[permissions]
toolchain = "write"
`, 'toml')}</pre>

<table class="shortcuts-table">
  <thead><tr><th>Function</th><th>Description</th></tr></thead>
  <tbody>
    <tr><td><code>arbor.toolchain.list(kind)</code></td><td>Returns all entries for <code>kind</code> as a Lua table. Each entry: <code>&#123; id, label, path, version?, active, env? &#125;</code></td></tr>
    <tr><td><code>arbor.toolchain.active(kind)</code></td><td>Returns the active entry for <code>kind</code>, or <code>nil</code></td></tr>
    <tr><td><code>arbor.toolchain.env&#123; kind, id? &#125;</code></td><td>Returns an env table for the given entry (e.g. <code>&#123; JAVA_HOME = "..." &#125;</code>). Uses the active entry when <code>id</code> is omitted</td></tr>
    <tr><td><code>arbor.toolchain.detect(kind)</code></td><td>Auto-detects installed toolchains of this kind and returns candidate entries</td></tr>
    <tr><td><code>arbor.toolchain.add(kind, entry)</code></td><td>Register a new entry. Entry must have at least <code>id</code>, <code>label</code>, <code>path</code></td></tr>
    <tr><td><code>arbor.toolchain.remove(kind, id)</code></td><td>Remove an entry by id</td></tr>
    <tr><td><code>arbor.toolchain.set_active(kind, id)</code></td><td>Mark an entry as the active one for its kind</td></tr>
  </tbody>
</table>

<p>Supported kind values: <code>"jdk"</code>, <code>"node"</code>, <code>"rust"</code>. Custom kinds are stored but have no built-in detection or env injection.</p>

<pre class="language-lua">{@html highlight(`-- list all registered JDKs
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
`, 'lua')}</pre>

<h2>Profile combo (variant = "profile")</h2>
<p>Register a combo with <code>variant = "profile"</code> to render it as a colored pill badge in RepoActions instead of the standard run+chevron split button. This is useful for environment selectors (dev / prod / test) that convey state rather than triggering an action.</p>

<pre class="language-lua">{@html highlight(`arbor.ui.add_graph_combo({
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
`, 'lua')}</pre>

<p>Semantic <code>color</code> values: <code>"dev"</code> â†’ green, <code>"prod"</code> â†’ red, <code>"test"</code> â†’ accent blue, <code>"none"</code> â†’ muted. Any other value falls back to the default accent style.</p>
