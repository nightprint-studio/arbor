<script lang="ts">
  /**
   * Appearance: the Interface settings, which are Arbor's rather than Bennu's — the Appearance page, rearranging the activity
   * bar, where it is kept, and the welcome tour.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Reference</span>
<h1>Appearance</h1>

<p class="doc-lead">
  <strong>Settings → Interface → Appearance</strong> (<kbd>Ctrl</kbd> + <kbd>,</kbd>). Everything on that page belongs to Arbor rather than to Bennu, and changing it here
  changes it in every Arbor window — the same page is in Corvus's settings, Merula's, the explorer's and Tyto's, and it is the same page, not a copy.
</p>

<h2>The pages under Interface</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-title">Appearance</div>
    <div class="fc-desc">The theme, the chrome and the text size — below.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Animations</div>
    <div class="fc-desc">How fast transitions run, or whether they run at all.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Keyboard Inputs</div>
    <div class="fc-desc">The overlay drawing every key and chord you press, with the action each triggered — for demos, screencasts and pairing.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Terminals</div>
    <div class="fc-desc">The shells the built-in terminal offers. Bennu's terminal is Arbor's: the same shells, configured once.</div>
  </div>
</div>

<h2>The Appearance page</h2>
<dl class="meta-grid">
  <dt>Colour theme</dt>
  <dd>The active theme, and the editor that makes new ones. The title bar's gear has the same list as a submenu, for switching without opening settings.</dd>
  <dt>Window controls</dt>
  <dd>A Mac-style trio on the left, or flat Windows controls on the right. Not shown on macOS, where the system paints the real ones.</dd>
  <dt>Compact title bar</dt>
  <dd>A shorter title bar, for a short screen.</dd>
  <dt>Minimized dialogs cap</dt>
  <dd>How many dialogs may sit parked in the status bar. Past it a minimize is refused with a toast; nothing parked is ever closed for you.</dd>
  <dt>Font scale</dt>
  <dd>Every piece of UI text together, from 80% to 140%. The editor's own font size is separate, under <em>Editor</em>.</dd>
</dl>

<h2>Rearranging the rails</h2>
<p>
  <strong>Customize Activity Bar…</strong> — from the command palette, the title bar's gear, or the button on the Appearance page — decides what is on the two icon rails
  and in what order: one tab per rail, and in each the top cluster (the tool windows) and the bottom one (the dock toggles).
</p>
<ul>
  <li>Drag the handle to reorder within a cluster.</li>
  <li>The eye hides a tool. It stays reachable — every one has a shortcut and a palette entry — it just stops taking a slot.</li>
  <li><strong>Project</strong> carries a lock: it is the way in, and with no file open and no tree nothing on screen could open one.</li>
  <li><strong>Restore defaults</strong> puts the current tab back in natural order with everything visible, without touching the other tab. Nothing is written until Save.</li>
</ul>
<Callout variant="info" title="Only what this project has">
  A Cargo root has no Structure, Maven or Forms button, so they are not there to arrange — but the arrangement you gave them on a Maven project is remembered, and comes back with them.
</Callout>

<h2>Where it is kept</h2>
<p>
  In the active profile's <code>config.toml</code>, beside the rest of the interface settings — never in the project. Two machines on one profile get the same bar; two
  projects on one machine do not each get their own.
</p>

<h2>The welcome tour</h2>
<p>
  Bennu opens its first-run tour once, on the first launch. <em>Welcome tour</em> in the palette opens it again whenever you want, and it can be skipped from any step. It is
  Bennu's own tour, with its own record of having been seen: going through Arbor's does not count, nor the reverse.
</p>
