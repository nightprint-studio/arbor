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
  <li>Click a track header or lane to select it — the Inspector and mixer follow.</li>
  <li>Right-click a header for mute / solo.</li>
</ul>

<h2>Navigating</h2>
<ul>
  <li><strong>Drag the ruler</strong> to scrub the play cursor; <strong>wheel</strong> over it (or <kbd>Shift</kbd> + wheel anywhere) scrolls the timeline.</li>
  <li><kbd>↑</kbd> / <kbd>↓</kbd> move between lanes, <kbd>←</kbd> / <kbd>→</kbd> nudge the cursor, <kbd>Home</kbd> seeks to the start.</li>
  <li>The view toolbar toggles waveform, follow-playhead, grid and event labels.</li>
</ul>

<div class="callout">
  The Outline panel mirrors this: expand a track to jump straight to any of its sections.
</div>
