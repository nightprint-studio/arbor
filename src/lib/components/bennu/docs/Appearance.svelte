<!-- Bennu docs — appearance and the activity bar: the settings that are the shell's, not Bennu's. -->
<h1>Appearance</h1>
<p class="doc-lead">
  <strong>Settings → Interface → Appearance</strong> (<kbd>Ctrl</kbd> + <kbd>,</kbd>). Everything
  on that page belongs to Arbor rather than to Bennu, and changing it here changes it in every
  Arbor window — the same page is in Corvus's settings, in merula's, in the explorer's and in
  Tyto's, and it is the same page, not a copy.
</p>

<h2>The pages under Interface</h2>
<ul>
  <li><strong>Appearance</strong> — the theme, the chrome and the text size (below).</li>
  <li><strong>Animations</strong> — how fast transitions run, or whether they run at all.</li>
  <li><strong>Keyboard Inputs</strong> — the overlay that draws every key and chord you press,
    with the action each one triggered. For demos, screencasts and pairing.</li>
  <li><strong>Terminals</strong> — the shells the built-in terminal offers. Bennu's terminal is
    Arbor's terminal: the same shells, configured once.</li>
</ul>

<h2>What is on the Appearance page</h2>
<ul>
  <li><strong>Colour theme</strong> — the active theme, and the editor that makes new ones. The
    title bar's gear has the same list as a submenu, for switching without opening settings.</li>
  <li><strong>Window controls</strong> — Mac-style trio on the left, or flat Windows controls on
    the right. Not shown on macOS, where the system paints the real ones.</li>
  <li><strong>Compact title bar</strong> — a shorter title bar, for a short screen.</li>
  <li><strong>Minimized dialogs cap</strong> — how many dialogs may sit parked in the status bar
    at once. Past the cap a minimize is refused with a toast; nothing parked is ever closed for
    you.</li>
  <li><strong>Font scale</strong> — scales every piece of UI text together, from 80% to 140%.
    The editor's own font size is separate, under <em>Editor</em>.</li>
</ul>

<h2>Rearranging the rails</h2>
<p>
  <strong>Customize Activity Bar…</strong> — from the Command Palette, from the title bar's gear,
  or from the button on the Appearance page — opens the dialog that decides what is on the two
  icon rails and in what order. One tab per rail, and within each, the top cluster (the tool
  windows) and the bottom one (the dock toggles).
</p>
<ul>
  <li>Drag the handle to reorder within a cluster.</li>
  <li>The eye hides a tool. It stays reachable — every one of them has a shortcut and a Command
    Palette entry — it simply stops taking a slot on the bar.</li>
  <li><strong>Project</strong> carries a lock and cannot be hidden: it is the one button that is
    the way in. With no file open and no tree, nothing on screen could open one.</li>
  <li><strong>Restore defaults</strong> puts the current tab back to its natural order with
    everything visible. It does not touch the other tab, and nothing is written until Save.</li>
</ul>

<div class="callout">
  The dialog only lists what <em>this</em> project has. A Cargo root has no Structure, Maven or
  Forms button, so they are not there to arrange — but the arrangement you gave them on a Maven
  project is remembered, and comes back with them.
</div>

<h2>Where it is kept</h2>
<p>
  In the active profile's <code>config.toml</code>, beside the rest of the interface settings —
  never in the project. Two machines on the same profile get the same bar; two projects on the
  same machine do not each get their own.
</p>

<h2>The welcome tour</h2>
<p>
  Bennu opens its first-run tour once, on the first launch. <em>Welcome tour</em> in the Command
  Palette re-opens it whenever you want, and it can be skipped from any step. Its own tour and
  its own record of having seen it: going through Arbor's does not count, and neither does the
  reverse.
</p>
