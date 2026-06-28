<h1>Mixer &amp; rendering</h1>
<p class="doc-lead">
  The mixer is one channel strip per track plus a master, driven by the live engine. It is
  a window onto the source: some controls are live overrides, others edit the code directly.
</p>

<h2>The strip</h2>
<ul>
  <li><strong>Fader</strong> — level, with a dB readout. Double-click returns it to unity.</li>
  <li><strong>Meter</strong> — the track's live stereo peak.</li>
  <li><strong>Pan</strong> — stereo position (bipolar knob, centre detent).</li>
  <li><strong>Room</strong> — reverb send (code-first; reflects the <code>.room(…)</code> literal).</li>
  <li><strong>Mute / Solo</strong> — mute writes <code>.gain(0)</code> into the source; solo is live-only.</li>
</ul>
<p>
  The <strong>master</strong> strip adds a <strong>gain-reduction meter</strong> beside its
  peak meter: a bar that drops from the top — with a dB readout — whenever the master
  limiter is ducking the mix. A steady reading means the output is being clamped; pull the
  master fader (or track gains) down until it only flickers on the loudest hits.
</p>

<h2>FX chain (EQ &amp; compressor)</h2>
<p>
  The Inspector's <strong>FX</strong> section edits a track's parametric EQ and
  compressor — the <code>.eq(…)</code> / <code>.comp(…)</code> strip inserts. The EQ
  shows a live response curve; <strong>add band</strong> stacks as many bands as you
  like (peak, shelf, high-/low-pass), each with frequency / gain / Q knobs. The
  compressor exposes threshold, ratio, attack, release, make-up and knee. Like room
  and delay these are <strong>code-first</strong>: the knobs reflect the source
  literals and commit straight back, so the effect is reproducible from the code.
</p>
<p>
  At the end of the mixer the <strong>reverb return</strong> strip shows the shared
  reverb bus: each track's <code>room</code> send appears as a bar feeding in, and a
  <strong>decay</strong> knob sets the bus length. Decay is a global, session-only
  control (like the master fader) — it isn't written to the <code>.merula</code> and
  persists across re-evaluations.
</p>

<h2>Live vs. code-first</h2>
<div class="callout accent">
  <span>
    <strong>gain</strong> and <strong>pan</strong> are both: dragging is heard instantly
    (a live override) and, once the gesture settles, the value is written back into the
    <code>.merula</code>. <strong>room</strong> / <strong>delay</strong> are code-first — they
    reflect the source literal and commit straight back to it. Every re-evaluation
    re-baselines the strips to what the source says.
  </span>
</div>
<p>
  When a track's gain is a <em>calculated</em> argument (not a literal), it can't be
  rewritten surgically — there mute stays live-only and the strip flags it.
</p>

<h2>Rendering to a file</h2>
<p>
  The export button in the title bar is a split control. Clicking it exports
  straight away using the chosen format (WAV or OGG Vorbis), which is remembered
  across sessions; its chevron opens a menu to switch the format or to
  <strong>Edit export…</strong>. The estimated size always reflects the selected
  format.
</p>
<ol class="step-list">
  <li>Press <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>R</kbd> (or <strong>Edit export…</strong> from the export menu) to open the options dialog.</li>
  <li>Choose the format, the render details (sample rate · bit depth · reverb tail — seeded from your defaults, overridable for this one export), and how many times the arrangement's natural loop should repeat — with a live duration · size estimate.</li>
  <li>The render runs in the background; progress shows in the title bar and the <strong>Downloads &amp; Exports</strong> overlay, where a <strong>Stop</strong> button cancels it mid-render (the partial file is discarded).</li>
</ol>
<p>
  <strong>Settings → Render</strong> holds the defaults for sample rate, bit depth
  and the reverb tail; the export dialog starts from them and lets you override
  them for a single export.
</p>
<p>
  The same overlay lists sample-bank and model downloads and library syncs — each
  can be stopped the same way.
</p>
