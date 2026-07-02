<script lang="ts">
  /** Tyto docs — Recording & screenshots. */
</script>

<h1>Recording &amp; screenshots</h1>
<p class="doc-lead">
  Tyto has two modes, switched from the titlebar or with
  <kbd>Ctrl</kbd>+<kbd>1</kbd> / <kbd>Ctrl</kbd>+<kbd>2</kbd>. The primary action
  and options adapt to the active mode.
</p>

<h2>Recording</h2>
<p>
  In <b>Recording</b> mode the titlebar shows a red <b>Record</b> pill. Start with
  it or <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. If a <b>countdown</b> is set, a big 3-2-1
  appears on screen first; then the Tyto window steps aside so it isn't in the
  capture and the recording <b>control bar</b> takes over. Options:
</p>
<ul>
  <li><b>System audio</b> — capture what you hear (game, browser, app output) via
    the Windows render loopback.</li>
  <li><b>Microphone</b> — mix in an input device. When both this and system audio
    are on they're mixed down into one track in the final MP4.</li>
  <li><b>Frame rate</b> — 30 or 60 fps.</li>
  <li><b>Quality</b> — High, Balanced or Compact (trades file size for fidelity).</li>
  <li><b>Countdown</b> — Off, 3, 5 or 10 seconds of on-screen count before capture
    starts, so you have a beat to get ready.</li>
</ul>

<h2>The recording control bar</h2>
<p>
  While recording, an always-on-top bar floats at the top of the screen (kept out
  of the capture). It comes in two layouts you switch between at any time:
</p>
<ul>
  <li><b>Compact</b> — a slim pill with the elapsed time, pause/resume and stop.</li>
  <li><b>Expanded</b> — a card that also shows what you're recording, with a large
    timer and full-width controls.</li>
</ul>
<p>
  <b>Pause/resume</b> holds the video (the elapsed clock stops with it); <b>Stop</b>
  finalizes and saves the file. When a recording or screenshot finishes, the Tyto
  window comes back with the fresh capture highlighted in the library.
</p>

<div class="callout">
  If the capture source becomes unavailable mid-recording — a monitor is unplugged,
  a captured window is closed, or the display changes resolution — Tyto stops on its
  own and saves everything recorded up to that point, rather than freezing.
</div>

<h2>Compact mode</h2>
<p>
  Shrink Tyto to a small, always-on-top <b>mini toolbar</b> (like Windows'
  <kbd>Win</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> bar) from the menu or with
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd>. The source button opens a
  <b>method menu</b> — Display, Window, Smart, Rectangle or Freehand — and every method
  opens the on-screen overlay in that mode, where you pick the target directly: windows
  and monitors highlight in blue as you hover and are picked with a click; Smart,
  Rectangle and Freehand draw over a frozen snapshot of the desktop. The bar grows just
  enough to show the menu, then snaps back. Mode and the capture button sit beside it. As
  soon as you finish a capture, Tyto returns to the full window with your new capture in
  the library; expand back any time from the bar's expand button.
</p>

<h2>Screenshots</h2>
<p>
  In <b>Screenshot</b> mode the action becomes a single <b>Screenshot</b> button.
  A quick-action for the other mode always sits next to the primary button, so
  you can grab a still without leaving recording setup (and vice-versa).
</p>
<ul>
  <li><b>Format</b> — save stills as <b>PNG</b> (lossless), <b>JPG</b> (smaller,
    lossy) or <b>WebP</b>. The choice sets the file extension and encoder; it
    applies to screenshots only — recordings use the video container. A
    <b>freehand region</b> screenshot is always saved as PNG regardless, since its
    transparency needs an alpha channel.</li>
</ul>

<div class="callout">
  Both actions are disabled until the target is ready — a region, for example,
  needs a drawn rectangle first. For a monitor or region <b>screenshot</b> Tyto
  hides itself for the grab so its window never lands in the shot.
</div>
