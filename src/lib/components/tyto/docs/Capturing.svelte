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
  counts down over the frozen screen first (<kbd>Esc</kbd> cancels); then the Tyto
  window steps aside so it isn't in the capture and the recording <b>control bar</b>
  takes over.
</p>
<p>
  The first option decides what the recording <i>produces</i>, and the rest of the
  options follow from it. <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>F</kbd> switches
  between the two.
</p>
<ul>
  <li><b>Video</b> — one H.264 MP4 with audio. The default, and what you want for
    anything that has sound or runs long.</li>
  <li><b>Frames</b> — a playable sequence of still images. Lossless, so text and
    thin lines stay exactly as captured, and no audio.</li>
</ul>

<h3>Video options</h3>
<ul>
  <li><b>System audio</b> — capture what you hear (game, browser, app output) via
    the Windows render loopback.</li>
  <li><b>Microphone</b> — mix in an input device. When both this and system audio
    are on they're mixed down into one track in the final MP4.</li>
  <li><b>Frame rate</b> — 30 or 60 fps.</li>
  <li><b>Quality</b> — High, Balanced or Compact (trades file size for fidelity).</li>
</ul>

<h3>Frame options</h3>
<ul>
  <li><b>Frame format</b> — <b>PNG</b> (lossless, keeps text crisp), <b>JPG</b> or
    <b>WebP</b>.</li>
  <li><b>Sample rate</b> — how often the screen is looked at, at most. Frames
    identical to the one before are never written, so a still screen costs one frame
    however long it stays still, and each frame records the moment it belongs to.
    Playback follows those moments, so the sequence runs at real speed.</li>
  <li><b>Frame width</b> — downscale every frame to a fixed width, or keep the
    captured resolution. This is the largest single lever on how much disk a
    sequence takes.</li>
</ul>

<p>
  <b>Countdown</b> — Off, 3, 5 or 10 seconds of on-screen count before capture
  starts, so you have a beat to get ready — applies to both.
</p>

<div class="callout">
  A frame sequence is saved as a <b>folder</b> named <code>&lt;name&gt;.frames</code>
  next to your other captures, holding the numbered frames and a manifest describing
  their timing. Renaming, revealing and deleting it from the library work exactly as
  they do for a single file.
</div>

<div class="callout">
  <b>macOS</b> asks for screen-recording permission when you open Tyto, and remembers
  the answer — the system only ever asks once. If it was refused, the source picker
  says so in place of the displays it can't see, with an <b>Open Settings</b> button
  that takes you straight to the right pane (<b>Privacy &amp; Security → Screen
  Recording</b>). Turn Arbor on there, then quit and reopen Arbor: a permission granted
  from System Settings reaches the next launch, not the running app.
</div>

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
  finalizes and saves the file. You can also stop from anywhere with the <b>global
  Tyto shortcut</b> — while a recording runs, the same key that starts a capture stops
  it, so you never have to hunt for the bar. When a recording or screenshot finishes,
  the Tyto window comes back with the fresh capture highlighted in the library and its
  preview open.
</p>

<div class="callout">
  If the capture source becomes unavailable mid-recording — a monitor is unplugged,
  a captured window is closed, or the display changes resolution — Tyto stops on its
  own and saves everything recorded up to that point, rather than freezing.
</div>

<h2>Snip capture</h2>
<p>
  Start a quick <b>snip</b> from the menu or with
  <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> (like Windows'
  <kbd>Win</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd> bar). The current monitor <b>freezes</b>
  and Tyto expands to cover it, showing a floating toolbar. Pick a <b>method</b> and draw
  or click directly on the frozen surface:
</p>
<ul>
  <li><b>Rectangle</b> — drag a box.</li>
  <li><b>Freehand</b> — trace a shape; a screenshot is masked to it.</li>
  <li><b>Smart</b> — hover to snap to a UI element; scroll to grow or shrink through its
    surrounding containers, then click.</li>
  <li><b>Window</b> — hover a window (it highlights in blue) and click to pick it.</li>
  <li><b>Display</b> — capture the whole current monitor.</li>
</ul>
<p>
  The toolbar also toggles <b>Record</b> / <b>Shot</b> so the capture button knows what to
  do, and a <b>monitor</b> button switches the frozen backdrop between displays (one
  screen at a time). <kbd>Enter</kbd> captures, <kbd>Esc</kbd> exits. As soon as a capture
  finishes, Tyto returns to the full window with your new capture in the library.
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
  <li><b>Copy to clipboard</b> — when on (the default), a screenshot is also placed
    on the system clipboard the moment it's saved, ready to paste anywhere.</li>
</ul>

<div class="callout">
  Both actions are disabled until the target is ready — a region, for example,
  needs a drawn rectangle first. For a monitor or region <b>screenshot</b> Tyto
  hides itself for the grab so its window never lands in the shot.
</div>
