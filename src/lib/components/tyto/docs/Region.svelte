<script lang="ts">
  /** Tyto docs — Region selection. */
</script>

<h1>Region selection</h1>
<p class="doc-lead">
  Choose the <b>Region</b> source and click <b>Select a region…</b> to draw the
  exact rectangle you want to capture.
</p>

<h2>Drawing a region</h2>
<ol class="step-list">
  <li>Tyto steps aside and a full-screen selection surface appears over a frozen
    snapshot of your desktop.</li>
  <li>Pick a mode from the toolbar: <b>Rectangle</b> (drag a box), <b>Freehand</b>
    (trace a shape), or <b>Smart</b> (hover an on-screen element to snap to it — scroll
    to widen/narrow through its container, click to capture). Smart is available when
    the app in front exposes its UI elements; it's shown greyed-out otherwise.</li>
  <li>Or pick a whole target on screen: <b>Window</b> and <b>Display</b> highlight the
    window / monitor under the cursor in blue — click to capture that whole window or
    monitor. These open over a snapshot of the entire desktop.</li>
  <li>Confirm with <kbd>Enter</kbd> (or the Capture button), or cancel with
    <kbd>Esc</kbd> / right-click.</li>
</ol>

<div class="callout accent">
  The selection is always dismissable from the keyboard — <kbd>Esc</kbd> cancels
  and brings Tyto back. You can never get stuck inside it.
</div>

<h2>Freehand shape</h2>
<p>
  A <b>freehand screenshot</b> captures the exact shape you traced: everything outside
  the outline is transparent, so an oval trace yields an oval image. Masked screenshots
  are always saved as <b>PNG</b> (transparency needs an alpha channel), whatever the
  screenshot format is set to. A freehand <b>recording</b> keeps the rectangular
  bounding box of your trace — video has no per-frame transparency.
</p>

<h2>Reusing a region</h2>
<p>
  Once drawn, the region is remembered and previewed in the source picker with
  its size and position. <b>Reselect</b> to redraw
  (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>) or <b>Clear</b> to drop it.
</p>
