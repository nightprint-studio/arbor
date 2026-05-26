<script lang="ts">
  import Kbd from '$lib/components/shared/internal/Kbd.svelte';
</script>

<h1>Settings — Interface &amp; Git</h1>

<p class="doc-lead">Open Settings with <Kbd action="settings" /> or via the gear icon in the title bar. Settings are organised into groups in the left sidebar — Interface, Git, Performance, Access, and Project.</p>

<h2>Interface</h2>

<h3>Appearance</h3>
<ul>
  <li><strong>Window controls</strong> — switches the close / minimize / maximize buttons in the title bar between the Mac-inspired coloured trio (default) and a flat Windows-/IntelliJ-style set that runs the full height of the title bar and reaches the right edge. The same switch also restyles the close button used in modal and panel headers.</li>
  <li><strong>Compact title bar</strong> — reduces the title-bar height and tightens its padding. Useful on laptops where vertical space is scarce.</li>
  <li><strong>Compact file tree folders</strong> — IntelliJ-style "compact middle packages". Chains of single-child directories collapse into one row across the sidebar Files panel, the Stage area tree, and the commit detail file list. For example <code>src/lib/components/foo.svelte</code> renders as <code>src/lib/components</code> when each intermediate folder only contains the next segment. Conflict file lists always compact regardless of this setting.</li>
  <li><strong>Font scale</strong> — scales all UI text from 0.8× to 1.4× in 5 % increments. Useful on HiDPI or small screens.</li>
  <li><strong>Activity bar</strong> — pin the icon rail to the left or right edge, or hide it (hover the edge to reveal it temporarily).</li>
  <li><strong>Minimized dialogs cap</strong> — maximum number of dialogs that can sit minimized in the status-bar panel at the same time (1–20, default 5). New minimize attempts past this cap are refused with a toast; no parked dialog is auto-closed.</li>
</ul>

<h3>Animations</h3>
<p>
  Controls the speed and behaviour of every transition and motion effect in the UI.
  Settings are stored in <code>~/.config/arbor/config.toml</code> and take effect immediately.
</p>
<ul>
  <li>
    <strong>Enable animations</strong> — master toggle. When off, all transitions play at
    zero duration: panels appear instantly, toasts pop in without sliding, modals open without
    scaling. Useful for accessibility (reduced-motion preference) or low-powered hardware.
  </li>
  <li>
    <strong>Speed</strong> — three presets that scale every duration proportionally:
    <ul>
      <li><em>Snappy</em> — ~55 % of default durations. Tight, fast feel.</li>
      <li><em>Normal</em> — 100 % (default). Balanced and polished.</li>
      <li><em>Relaxed</em> — ~165 % of default durations. Smoother, more fluid motion.</li>
    </ul>
  </li>
  <li>
    <strong>Preview</strong> — hit <em>Replay</em> to see an animated chip using the current speed
    setting without leaving the panel.
  </li>
</ul>
<p>
  Animations that are controlled by this setting include: sidebar slide-in, bottom/right panel
  slide-in, modal and command-palette entrance, toast slide-in/out, overlay fade, settings section
  fade, and all CSS <code>transition</code> properties on interactive elements (hover states, toggles, buttons).
</p>

<h3>Keyboard Inputs</h3>
<p>
  An on-screen overlay that displays every key, chord and (optionally) mouse click as it
  happens — paired with the human-readable name of the action the shortcut triggers.
  Designed for demos, screencasts, pair-programming sessions, and any moment someone
  needs to see what you just pressed and what it does.
</p>
<p>
  Each captured chord is shown as an IDE-style key cap card with the modifier keys in
  accent and the printable key in solid. When the chord matches a known built-in or
  plugin binding, its action label slides in underneath — so viewers learn the
  shortcut <em>and</em> understand the command in a single glance. Rapid repeats
  collapse into a single card with a <code>×N</code> counter.
</p>
<ul>
  <li>
    <strong>Show keyboard inputs</strong> — master toggle. Can be flipped from anywhere
    with the global shortcut <Kbd action="toggle_keystrokes" size="sm" />, which works
    even while a modal is open (useful when you suddenly decide to start recording).
  </li>
  <li>
    <strong>Position</strong> — pick one of five anchors via a live mini-window preview:
    top-left, top-right, bottom-left, bottom-center, bottom-right. The selected spot lights
    up in accent.
  </li>
  <li>
    <strong>Size</strong> — Small, Medium or Large. Scales every pill, key cap and counter
    proportionally.
  </li>
  <li>
    <strong>Accent tone</strong> — pick the colour applied to the side stripe, modifier
    keys and the <code>×N</code> counter. <em>Accent</em> follows your theme; <em>Neon</em>,
    <em>Aqua</em> and <em>Amber</em> are fixed bright hues good for screencasts;
    <em>Mono</em> drops the saturation entirely for a purely typographic look. Mouse-click
    badges always keep their cool-blue tone regardless of this setting.
  </li>
  <li>
    <strong>Edge offset</strong> — distance in pixels between the overlay and the anchored
    screen edge (8 – 120 px). Bump it up if the overlay is being covered by another tool's
    HUD, drop it down for a more flush look.
  </li>
  <li>
    <strong>Compact layout</strong> — collapses the chord and action label onto a single
    line separated by a thin dot, instead of stacking them vertically. Great for
    minimalist demos.
  </li>
  <li>
    <strong>Show action label</strong> — toggles whether the human-readable action name
    appears under (or next to, in compact mode) each chord. Turn off for a purely
    typographic look.
  </li>
  <li>
    <strong>Visibility</strong> — how long each keystroke stays on screen (0.5 s – 6 s).
    Rapid repeats of the same chord collapse into a single pill with a <code>×N</code>
    counter instead of stacking.
  </li>
  <li>
    <strong>Opacity</strong> — overall transparency of every pill (40 % – 100 %, default
    100 %). The card has a fully solid background by default so nothing underneath
    bleeds through. Lower the slider only if you deliberately want the overlay to fade
    into a busy screen.
  </li>
  <li>
    <strong>Only show shortcuts</strong> — hides plain printable keys so only chords with
    Ctrl, Alt, Shift or Meta show up. Great for tutorials that focus on commands rather
    than typing.
  </li>
  <li>
    <strong>Capture while typing</strong> — off by default, so the overlay stays quiet
    while you write commit messages. Modifier chords inside text fields (e.g.
    <kbd>Ctrl+S</kbd>) are always captured regardless.
  </li>
  <li>
    <strong>Show mouse clicks</strong> — adds a small badge for Left / Middle / Right
    clicks. Off by default.
  </li>
  <li>
    <strong>Group rapid repeats</strong> — when on, the same chord pressed within ~600 ms
    bumps a counter instead of pushing a new pill. Turn it off if you specifically want to
    show every individual press.
  </li>
  <li>
    <strong>Try it out</strong> — the settings panel has live preset buttons
    (<em>Ctrl + K</em>, <em>Esc</em>, <em>F5</em>, …) so you can preview the look without
    needing to actually press keys.
  </li>
</ul>
<p>
  The overlay uses <code>pointer-events: none</code> and never preventDefaults any event,
  so clicks and keystrokes pass straight through to the underlying app. The capture
  listener detaches automatically the moment you toggle the feature off — no idle cost.
  No <code>backdrop-filter</code> or runtime blur is used anywhere in the overlay: it
  paints to solid colours only, so even at 60+ entries per second on the busiest
  CommitGraph there is zero per-frame compositor work for the background.
  All settings persist to <code>~/.config/arbor/config.toml</code> and survive restarts.
</p>

<h3>Graph</h3>
<ul>
  <li><strong>Commits per load</strong> — how many commits are fetched each time the graph loads or is scrolled to the end (100 – 2000, default 500). Only applies when lazy-load pagination is on.</li>
  <li><strong>Show remote branches</strong> — toggle remote-tracking refs (e.g. <code>origin/main</code>) in the lane graph.</li>
  <li><strong>Show tags</strong> — toggle annotated and lightweight tags in the lane graph.</li>
  <li><strong>Lazy-load pagination</strong> — when <em>on</em> (default), commits are loaded in batches as you scroll; when <em>off</em>, the entire repository history is loaded at once. Disable only on small repos — loading tens of thousands of commits at startup can be slow. Persisted to <code>~/.config/arbor/config.toml</code>.</li>
</ul>

<h3>Diff &amp; Stage</h3>
<ul>
  <li><strong>Context lines</strong> — number of unchanged lines shown around each hunk (0 – 20, default 3).</li>
  <li><strong>View mode</strong> — Unified (single column) or Split (side-by-side).</li>
  <li><strong>Show full file</strong> — render the entire file with diff highlights instead of just changed hunks. Useful for reading a change in its full surrounding context. The same toggle is available as a button (file icon) in the diff viewer header. Persisted to <code>~/.config/arbor/config.toml</code> under <code>[diff]</code>.</li>
  <li><strong>Virtualization threshold</strong> — when a file's diff has more than this many lines, the renderer switches to a windowed mode that only paints visible rows (default 200). Lower values keep huge files snappier.</li>
  <li><strong>Confirm before discarding</strong> — when enabled (default), a confirmation dialog appears before discarding a single file's changes. The <em>Discard All</em> confirmation is always shown regardless of this setting.</li>
</ul>

<h3>Diff viewer controls</h3>
<p>The diff header carries a few extra controls on top of the Unified/Split toggle:</p>
<ul>
  <li><strong>Chunk navigation</strong> — <kbd>↑</kbd> / <kbd>↓</kbd> chevrons (with a <em>n/N</em> counter) jump between change blocks. <Kbd action="next_chunk" /> / <Kbd action="prev_chunk" /> do the same from the keyboard.</li>
  <li><strong>Show full file</strong> — the file icon mirrors the global setting; toggling it here rebuilds the visible diff immediately.</li>
  <li><strong>Auto-focus</strong> — opening a file (or staging a line) lands the view on the first remaining change instead of the top of the file.</li>
</ul>

<h3>Keybindings</h3>
<p>
  Click any shortcut chip to record a new key combination. Press <kbd>Escape</kbd> while recording to cancel.
  Use the reset icon to restore a single binding to its default. <strong>Reset all</strong> restores every binding at once.
</p>
<p>
  The <strong>Plugins</strong> group at the bottom of the list shows keybindings registered by plugins — these are read-only.
</p>

<h3>Activity Bar</h3>
<p>
  The Activity Bar can be customised without touching any setting panel.
  Click the <strong>gear icon → Customize Activity Bar…</strong> in the title bar to open the layout editor.
</p>
<ul>
  <li>
    <strong>Visibility</strong> — each item has an eye icon. Click it to show or hide the button.
    Items marked with a lock icon (<em>Branches</em>, <em>Stage</em>, <em>Detail</em>) are mandatory and cannot be hidden.
  </li>
  <li>
    <strong>Order</strong> — drag items by their handle to reorder them within their section.
    A blue indicator line shows the insertion point as you drag.
  </li>
  <li>
    <strong>Two sections</strong> — <em>Sidebar</em> (top half, controls which panel opens on the left)
    and <em>Panel</em> (bottom half, controls the bottom panel: stage, commit detail, terminal, jobs, pipelines).
    Items can only be reordered within their own section.
  </li>
  <li>
    <strong>Plugin items</strong> — actions, combo buttons, and separators registered by plugins also
    appear here and can be reordered or hidden like built-in items.
  </li>
</ul>
<p>
  The layout is persisted to <code>~/.config/arbor/config.toml</code> and restored on next launch.
  Hidden items are still active in the background — hiding the Stage button does not disable staging.
</p>

<h2>Git</h2>

<h3>Git Flow</h3>
<p>See the dedicated <strong>Git Flow</strong> section in the sidebar for full documentation.</p>

<h3>Experimental</h3>
<p>
  Features that depend on external data, are still maturing, or may produce unexpected results
  in edge cases. All flags default to <strong>on</strong> and are stored in
  <code>localStorage</code> — they never alter local Git state and can be toggled at any time.
</p>

<h4>Squash-merge ghost edges</h4>
<p>
  When a Pull Request or Merge Request is merged via <em>squash</em>, Git creates a single new
  commit on the target branch whose only parent is the previous tip — there is no topological link
  to the original feature commits. The commit graph therefore shows the feature branch as a dangling
  strand with no visible connection to the merge point.
</p>
<p>
  When this flag is on, Arbor queries the GitHub / GitLab API on every graph load to retrieve the
  <code>merge_commit_sha</code> of each closed PR / merged MR, then draws a dashed ghost edge
  connecting that commit to the feature branch tip.
</p>
<ul>
  <li><strong>Ghost edge style</strong> — semi-transparent dashed line (45 % opacity, <code>5 3</code> dash pattern) in the feature branch's lane colour.</li>
  <li><strong>Fallback anchor</strong> — if the merge commit hasn't been fetched locally yet, the ghost edge connects the feature tip to the target branch tip <em>before</em> the merge. Once you <code>git fetch</code>, native edges appear and the ghost is suppressed automatically.</li>
  <li><strong>No token / no remote</strong> — degrades silently; graph loads normally without ghost edges.</li>
  <li><strong>Performance</strong> — adds one API call per graph load (up to 50 closed PRs/MRs). May add latency on repos with many closed PRs.</li>
</ul>
