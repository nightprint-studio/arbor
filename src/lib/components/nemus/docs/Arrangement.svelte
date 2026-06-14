<h1>Arrangement &amp; sections</h1>
<p class="doc-lead">
  The arrangement view is a read-only DAW timeline of what you wrote. Each
  <code>track(…)</code> is a lane; <code>section(…)</code> blocks draw named, coloured
  bands so the form of the piece is visible at a glance.
</p>

<h2>Sections</h2>
<pre><code>track("Violoncelli", arrange(
  section("Esposizione", 16, celli_theme),
  section("Tutti", 16, celli_harmony),
))</code></pre>
<p>
  <code>arrange(section("name", bars, pattern), …)</code> lays sections end to end. Give
  every track the same sections and the bands line up across the whole arrangement, so the
  structure (intro, verse, chorus…) reads down the timeline.
</p>

<h2>Reading the timeline</h2>
<ul>
  <li>Pitched notes draw as piano-roll blocks, high notes near the top; samples and signals draw as region blocks.</li>
  <li>Hover an event for its bar:beat, duration, MIDI and (when transposed) the written note.</li>
  <li><kbd>Ctrl</kbd> + click an event to jump to the source that produced it.</li>
  <li>Select code in the editor to outline the regions it produces here.</li>
  <li>Click a track header or lane to select it — the Inspector and mixer follow.</li>
  <li><strong>Drag the bottom edge of a track header</strong> to resize that lane (double-click to reset); taller lanes spread the piano-roll so pitches separate.</li>
  <li>Right-click a header for mute / solo.</li>
</ul>

<h2>Navigating</h2>
<ul>
  <li><strong>Drag the ruler</strong> to scrub the play cursor; <strong>wheel</strong> over it (or <kbd>Shift</kbd> + wheel anywhere) scrolls the timeline.</li>
  <li><strong><kbd>Ctrl</kbd> + wheel</strong> zooms the timeline horizontally (centred on the pointer); the toolbar's <strong>−</strong> / <strong>%</strong> / <strong>+</strong> do the same (click the readout to reset to 100%).</li>
  <li>The <strong>minimap</strong> below the timeline shows the whole arrangement with a viewport box — drag it to pan. Toggle it from the view toolbar.</li>
  <li><kbd>↑</kbd> / <kbd>↓</kbd> move between lanes, <kbd>←</kbd> / <kbd>→</kbd> nudge the cursor, <kbd>Home</kbd> seeks to the start.</li>
  <li><kbd>F8</kbd> plays from the cursor (punch-in) instead of from the top.</li>
  <li>The view toolbar toggles waveform, follow-playhead, grid, event labels, the minimap and the <strong>velocity heatmap</strong> (events coloured by gain — quieter notes fade, full-gain notes stay vivid).</li>
  <li>The <strong>metronome</strong> toggle (or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>B</kbd>) adds an audible click on every beat, accented on the bar — a monitoring aid that bypasses the mix (mute / solo don't silence it).</li>
  <li>The <strong>count-in</strong> toggle (or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd>) steps a 1- or 2-bar metronome pre-roll: playback waits that many bars, clicking you in, before the song starts. It applies to the next play (from the top or from the cursor).</li>
</ul>

<h2>Tempo</h2>
<p>
  The view toolbar carries a live <strong>tempo</strong> readout in BPM: the
  <strong>−</strong> / <strong>+</strong> buttons nudge it by 1 BPM (hold <kbd>Shift</kbd>
  for ±5), and <strong>TAP</strong> sets it from your taps — tap in time, or focus it and
  press <kbd>Space</kbd> in rhythm. This is a <em>live override</em> (handy for matching or
  feeling out a tempo): it's released on the next run, when the score's own
  <code>cps(…)</code> / <code>tempo(…)</code> takes over again. Click the readout to drop the
  override and follow the score immediately.
</p>

<h2>Loop region</h2>
<p>
  <strong><kbd>Alt</kbd> + drag the ruler</strong> to mark a loop region (snapped to whole
  bars). While it's on, playback cycles within it — handy for working a section. The
  <strong>loop toggle</strong> in the view toolbar (or <kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>L</kbd>) switches it on and off without losing the region; <kbd>Esc</kbd> clears it.
  With a region set, <strong>Export loop region…</strong> (export menu / Command Palette)
  bounces just that span to its own audio file.
</p>

<h2>Markers</h2>
<p>
  Drop named jump points on the ruler: <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> adds
  one at the cursor, or <strong>right-click the ruler</strong> to add one where you click.
  Click a marker to seek to it; right-click it to <strong>rename</strong> or delete it.
  Navigate hands-free with <kbd>Ctrl</kbd> + <kbd>←</kbd> / <kbd>→</kbd> (jump to the previous /
  next marker).
</p>

<div class="callout">
  The Outline panel mirrors this: expand a track to jump straight to any of its sections.
</div>
