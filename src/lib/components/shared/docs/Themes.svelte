<h1>Themes</h1>

<p class="doc-lead">
  Open the Theme Editor from <strong>Settings → Appearance → Open Theme Editor</strong>.
  Every colour, shadow and terminal palette in Arbor is driven by CSS custom properties
  exposed in the editor — change one, every panel updates live.
</p>

<h2>Built-in themes</h2>
<ul>
  <li><strong>Dark</strong> — JetBrains-inspired default.</li>
  <li><strong>Light</strong> — high-contrast daytime variant.</li>
</ul>
<p>
  Built-ins are read-only. Use the <em>clone</em> icon in the sidebar to fork one
  into your custom list, then edit freely.
</p>

<h2>Importing themes</h2>
<p>
  The <em>Import</em> button in the editor header opens the in-app file picker.
  Select one or many <code>.json</code> files (Ctrl/⌘+click to add, Shift+click for
  a range) and confirm — every file is parsed independently and each successful
  import becomes a new custom theme. Failures are surfaced via toast and the dev
  console without aborting the rest of the batch.
</p>
<p>
  Imported themes always receive a freshly-generated <code>custom-*</code> id so they
  never clash with built-ins or other customs already on disk.
</p>

<h2>Bundled presets</h2>
<p>
  Arbor ships a small library of community-favourite palettes as plain JSON files
  in the <code>themes/</code> directory at the project root — exactly the same format
  the importer accepts. Browse there with the file picker and pick whichever you
  like; multi-select is supported, so installing the whole pack is one click.
</p>
<h3>Dark</h3>
<ul>
  <li><code>themes/tokyo-night.json</code></li>
  <li><code>themes/tokyo-night-storm.json</code></li>
  <li><code>themes/caffeine.json</code></li>
  <li><code>themes/dracula.json</code></li>
  <li><code>themes/monokai.json</code></li>
  <li><code>themes/gruvbox-dark.json</code></li>
  <li><code>themes/nord.json</code></li>
  <li><code>themes/solarized-dark.json</code></li>
  <li><code>themes/catppuccin-mocha.json</code></li>
  <li><code>themes/catppuccin-macchiato.json</code></li>
  <li><code>themes/catppuccin-frappe.json</code></li>
  <li><code>themes/one-dark.json</code></li>
  <li><code>themes/ayu-dark.json</code></li>
  <li><code>themes/rose-pine.json</code></li>
  <li><code>themes/rose-pine-moon.json</code></li>
  <li><code>themes/github-dark.json</code></li>
  <li><code>themes/kanagawa.json</code></li>
</ul>
<h3>Light</h3>
<ul>
  <li><code>themes/tokyo-night-day.json</code></li>
  <li><code>themes/catppuccin-latte.json</code></li>
  <li><code>themes/one-light.json</code></li>
  <li><code>themes/solarized-light.json</code></li>
  <li><code>themes/gruvbox-light.json</code></li>
  <li><code>themes/ayu-light.json</code></li>
  <li><code>themes/rose-pine-dawn.json</code></li>
  <li><code>themes/github-light.json</code></li>
</ul>
<p>
  Once imported, presets become regular custom themes — edit, duplicate, export, or
  delete them like any other entry. Drop additional <code>.json</code> files into
  <code>themes/</code> to share your own palettes alongside the bundled ones.
</p>

<h2>Exporting themes</h2>
<p>
  Select any theme in the sidebar and hit <em>Export</em> in the header. The save
  dialog (the in-app picker, never the OS one) suggests a filename based on the
  theme id; pick a destination and you'll have a portable JSON file you can share,
  back up, or version-control.
</p>

<h2>Theme JSON schema</h2>
<p>
  A theme file is a small JSON document. Only <code>name</code> and <code>vars</code>
  are required; everything else is optional. The <code>vars</code> map keys are CSS
  custom properties that are written verbatim onto <code>:root</code>.
</p>
<pre><code>&#123;
  "id":          "preset-tokyo-night",
  "name":        "Tokyo Night",
  "description": "A clean, dark theme inspired by Tokyo at night",
  "vars": &#123;
    "--bg-base":      "#1a1b26",
    "--bg-elevated":  "#24283b",
    "--accent":       "#7aa2f7",
    "--text-primary": "#c0caf5",
    "--terminal-bg":  "#1a1b26",
    "...":            "..."
  &#125;
&#125;</code></pre>
<p>
  Any non-string value or key not starting with <code>--</code> is dropped silently.
  Unknown variable names are accepted and written to the document — Arbor's own
  styles will simply ignore them, so it's safe to ship themes with extra tokens
  for plugins that consume their own variables.
</p>

<h2>Beyond colours</h2>
<p>
  A theme can also customise a few non-colour aspects of the UI. All of these
  are optional — themes that don't declare them inherit the global defaults.
</p>
<h3>Geometry</h3>
<ul>
  <li><code>--radius-sm / --radius-md / --radius-lg</code> — corner radius scale. Sharp Solarized uses <code>2/3/5px</code>; rounded Catppuccin uses <code>5/8/12px</code>; default is <code>4/6/10px</code>.</li>
  <li><code>--scrollbar-width</code> — thumb width in pixels (defaults to <code>6px</code>; some themes prefer 7-8 px for a chunkier feel).</li>
  <li><code>--scrollbar-radius</code> — thumb radius (default <code>999px</code> = pill; Solarized uses <code>2px</code> for a square scrollbar; Monokai uses <code>0</code> for utterly square).</li>
</ul>
<h3>Selection feel</h3>
<ul>
  <li><code>--selection-strength</code> — multiplier (0.5–1.5) applied to the alpha of text-selection backgrounds. <code>1</code> is neutral, light pastel themes use <code>0.7-0.8</code>, Dracula-style vivid themes go up to <code>1.3</code>.</li>
</ul>
<h3>Typography (opt-in)</h3>
<p>
  Themes can <em>suggest</em> a UI / code font, but the override is only applied
  when the user enables <strong>Use theme fonts</strong> in the editor header.
  Set <code>--theme-font-ui</code> and / or <code>--theme-font-code</code> to a
  CSS font-family stack:
</p>
<pre><code>"--theme-font-code": "'Hack', 'JetBrains Mono', ui-monospace, monospace"</code></pre>
<p>
  The toggle is global and persisted; turning it off (default) restores the
  user's preferred font stack regardless of which theme is active. This means
  a theme can publish a canonical font without ever silently overriding what
  someone has installed.
</p>

<h2>Storage</h2>
<ul>
  <li>Custom themes live as individual files at <code>~/.config/arbor/themes/&lt;id&gt;.json</code>.</li>
  <li>The active theme id is persisted in <code>~/.config/arbor/config.toml</code> under <code>[theme]</code>.</li>
  <li>Bundled presets live in the project repo at <code>themes/*.json</code> — they are imported on demand, not auto-loaded.</li>
</ul>
