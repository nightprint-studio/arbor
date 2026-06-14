<!-- Nemus docs — Getting Started. Plain semantic HTML; DocsShell/PluginDocBlock
     supplies the typography. -->
<h1>Getting started</h1>
<p class="doc-lead">
  nemus is a music live-coding studio. You write a <code>.nemus</code> file — a small,
  total language of patterns — and the engine plays it. The arrangement view shows what
  you wrote as a DAW timeline; the mixer and inspector read straight from the same source.
</p>

<h2>The loop</h2>
<ol class="step-list">
  <li>Open or create a project (a folder with a <code>nemus.toml</code> manifest and one or more <code>.nemus</code> files).</li>
  <li>Edit a file. Every track is a pattern — a sequence of notes or samples over time.</li>
  <li>Press <kbd>Shift</kbd> + <kbd>F9</kbd> to <strong>Run</strong>. The engine evaluates the source and starts the transport.</li>
  <li>Keep editing while it plays. Re-evaluation re-baselines the arrangement, mixer and inspector.</li>
  <li>Export when you're happy — audio (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>R</kbd>) or a MIDI file.</li>
</ol>

<div class="callout">
  <strong>Exporting.</strong> The export split-button (and the Command Palette) offers
  a <strong>WAV / OGG audio</strong> bounce — the full rendered sound —
  <strong>Export loop region…</strong>, which bounces just the span you set as the
  loop region on the ruler, <strong>Export stems…</strong>, which writes one audio
  file per track into a folder you pick (each track rendered in isolation, ready to
  mix elsewhere), or <strong>Export MIDI…</strong>, which writes the arrangement's
  note data to a <code>.mid</code> (one track per nemus track; pitched notes plus
  recognised drum sounds as General-MIDI percussion) to take into any other DAW.
  The MIDI export bakes the song's natural loop period, once. <strong>Edit export…</strong>
  adds an optional <strong>Normalize</strong> to a target loudness (LUFS) for the audio
  bounce, peak-limited so it never clips.
</div>

<h2>A first file</h2>
<pre><code>cps(0.5)

tracks(
  track("lead", n(c4 e4 g4 c5).inst("synth.lead")),
)</code></pre>
<p>
  <code>cps(0.5)</code> sets the clock to half a cycle per second. <code>tracks(…)</code>
  is the output: each <code>track("name", pattern)</code> becomes one strip in the
  arrangement and mixer. <code>n(…)</code> is a note island; <code>.inst("…")</code> picks
  the instrument that sounds it.
</p>

<div class="callout accent">
  One cycle is one bar of 4/4 by convention. <code>cps</code> is cycles-per-second, so
  tempo in BPM ≈ <code>cps × 60 × 4</code>. <code>cps(0.6)</code> ≈ ♩144.
</div>

<h2>Where things live</h2>
<ul>
  <li><strong>Files</strong> — the project's <code>.nemus</code> files, with a per-file summary.</li>
  <li><strong>Outline</strong> — tracks, functions and constants in the active file; tracks expand to their sections.</li>
  <li><strong>Language reference</strong> — every island, combinator, transform and signal, searchable.</li>
  <li><strong>Sound bank</strong> — the engine's resolvable voices + downloadable sample packs. Click a voice to copy its name; star it to keep it in <strong>Favourites</strong>, and voices you use surface under <strong>Recently used</strong>.</li>
  <li><strong>Mixer</strong> — a fader per track, driven by the live engine.</li>
  <li><strong>Inspector</strong> — the selected track's character and the picked event.</li>
  <li><strong>Keyboard</strong> — a piano that lights the notes sounding at the playhead, coloured per track.</li>
</ul>
<p>
  Your open tabs are remembered per project (reopened with it), and the Scratch tabs come
  back too — so reopening a project picks up where you left off.
</p>

<h2>Going full-screen</h2>
<p>
  <strong>Performance mode</strong> (<kbd>F11</kbd>) turns the window into a distraction-free
  full-screen stage for live play: the rails, footer and titlebar drop away and the editor +
  arrangement fill the screen. A floating <strong>Exit</strong> stays in the top-right (or press
  <kbd>F11</kbd> again). <strong>Zen mode</strong> (<kbd>Ctrl</kbd> + <kbd>Shift</kbd> +
  <kbd>Z</kbd>) hides the same chrome without going full-screen.
</p>
