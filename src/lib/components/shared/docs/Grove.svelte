<span class="eyebrow">Music</span>
<h1>grove — Music live-coding</h1>

<p class="doc-lead">grove is a standalone music live-coding studio that ships inside Arbor — its own window with a code editor, a read-only arrangement view, a mixer, and a live audio engine. You write music as <code>.grove</code> source; the engine evaluates it and plays it back in real time.</p>

<div class="hint">grove is <strong>code-first</strong>: the source is the instrument. The panels are feedback and fine-tuning — the arrangement, mixer and inspector reflect what the code produces, they don't replace it.</div>

<h2>Opening grove</h2>
<p>Open the grove window from the Command Palette — search for <em>Open grove (Music)</em>. It opens in its own window, independent of any repository, so you can keep it alongside your work (a game's <code>music/</code> folder can live in the same project as its code).</p>

<h2>Projects</h2>
<p>A grove <strong>project</strong> is a folder with a <code>grove.toml</code> manifest (a <code>name</code> and an <code>audience</code> — "who the music is for") plus its <code>.grove</code> files. Files listed under <code>libraries</code> are imported-only: their <code>tracks(…)</code> output is ignored, only their <code>fn</code> / <code>let</code> declarations are exported for re-use.</p>
<ol class="step-list">
  <li>Create a project (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>) or open one (<kbd>Ctrl</kbd>+<kbd>O</kbd>) — pick a folder.</li>
  <li>The entry <code>.grove</code> opens as an editor tab; the <strong>Files</strong> panel lists the rest.</li>
  <li>Switch projects from the title-bar project dropdown — it remembers your recents.</li>
</ol>

<h2>The window</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Editor</div>
    <div class="fc-desc">CodeMirror with grove syntax highlighting, inline error underlines, and a live highlight of whatever is sounding right now. Edits re-evaluate automatically.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Arrangement</div>
    <div class="fc-desc">A read-only, Logic-style timeline of the evaluated tracks, with a playhead that follows the transport. Named <code>section(…)</code> blocks show as coloured ruler chips and tinted lane bands. Click the ruler to seek; right-click a lane to mute / solo.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Mixer</div>
    <div class="fc-desc">One strip per track with live meters and gain / pan knobs (live overrides on top of the source). The room knob and the Inspector's delay knobs are <strong>code-first</strong> — they write the value straight into the <code>.grove</code> source. Commit a gain / pan override to source with the ↧ button on the strip.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Console &amp; Problems</div>
    <div class="fc-desc">The Console shows log lines gated to your threshold; Problems lists evaluation diagnostics. Click a problem to jump the editor to its source span.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Sound bank</div>
    <div class="fc-desc">The instruments the engine can resolve — built-in synth presets and the VSCO 2 orchestral samplers, each listing the articulations it exposes for <code>.art("…")</code>. Download / manage the VSCO 2 bank from here.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Outline</div>
    <div class="fc-desc">The tracks, functions, constants and imports of the active file. Click a symbol to jump to its declaration; <kbd>Ctrl</kbd>+<kbd>Click</kbd> a name in the editor to follow it (cross-file too).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Inspector</div>
    <div class="fc-desc">Detail for the selected track: voice, meters, the live mix values, pattern statistics (hap count, pitch range), and the code-first <strong>delay</strong> knobs (time / feedback / mix) that write a <code>.delay(…)</code> into the source.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Zen &amp; collapse</div>
    <div class="fc-desc">Zen mode (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd>) hides the chrome; the title-bar toggles collapse the arrangement or the editor so one fills the body.</div>
  </div>
</div>

<h2>Editing from the panels</h2>
<p>The mixer and inspector knobs are a surgical bridge back to the source — the code stays the single source of truth:</p>
<ul class="prop-list">
  <li><code>gain</code> / <code>pan</code> are <strong>live overrides</strong>: drag to hear the change instantly; each re-evaluation re-baselines them to the source. Press the ↧ button on a strip (or <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> for all strips) to <strong>commit</strong> the current value into the source as a <code>.gain(…)</code> / <code>.pan(…)</code> literal.</li>
  <li><code>room</code> (mixer) and <code>delay</code> (inspector: time / feedback / mix) are <strong>code-first</strong>: turning a knob writes the literal straight into the track's <code>.grove</code> source — adding the method to the chain if it isn't there yet — and re-evaluates.</li>
  <li>A knob whose value is <em>calculated</em> in the source (e.g. <code>.gain(rand(0,1))</code>) is shown read-only: there is no single literal to rewrite.</li>
</ul>
<div class="hint">Commits are ordinary editor edits — one <kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes them.</div>

<h2>Sections</h2>
<p>Wrap an arrangement block in <code>section("NAME", cycles, pattern)</code> — the named counterpart of <code>cycles(…)</code> inside <code>arrange(…)</code> — to label a stretch of the timeline. Named sections surface in the arrangement as coloured ruler chips and tinted lane bands, tiled across the loop, so the song's macro-structure (intro / build / drop / outro) reads at a glance.</p>

<h2>Tempo</h2>
<p><code>cps(n)</code> sets a constant clock (cycles-per-second). For tempo that changes over the song, use a <strong>tempo map</strong>: <code>tempo(cycles(8, 0.5), cycles(16, 0.6))</code> plays 8 cycles at 0.5 cps, then 16 at 0.6, then loops. The tempo changes on whole-cycle boundaries and the playhead position stays continuous; the footer shows the live tempo. (Smooth accelerando / rubato is a future addition — for now tempo steps between segments.)</p>

<h2>Playing &amp; rendering</h2>
<ol class="step-list">
  <li>Press <kbd>Ctrl</kbd>+<kbd>Space</kbd> to Run — the engine opens the audio device and starts the transport (the button turns red / shows Stop).</li>
  <li>Edit while it plays: changes re-evaluate and swap in at the next cycle boundary, with the arrangement and meters updating live.</li>
  <li>Seek by clicking the arrangement ruler, or with <kbd>←</kbd> / <kbd>→</kbd> / <kbd>Home</kbd> while it has focus.</li>
  <li>Export to WAV with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd> — an offline render that runs as a background job.</li>
</ol>

<h2>Command Palette</h2>
<p>The grove window has its own Command Palette (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>) listing every window action — transport, project operations, panel toggles, settings. Type to filter, <kbd>↑</kbd> / <kbd>↓</kbd> to move, <kbd>Enter</kbd> to run.</p>

<h2>Keyboard shortcuts</h2>
<p>The full grove cheat-sheet is in the window's gear menu under <em>Keyboard Shortcuts</em> (<kbd>F1</kbd>). The essentials:</p>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Space</kbd></td><td>Run / Stop the transport</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd></td><td>Command Palette</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>O</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd></td><td>Open / new project</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd></td><td>Open a <code>.grove</code> file</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>N</kbd></td><td>New <code>.grove</code> file (editor)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>S</kbd></td><td>Save the active file</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>G</kbd></td><td>Go to line (editor)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Click</kbd></td><td>Go to declaration (incl. cross-file)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>F</kbd></td><td>Search the Console / Problems</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd></td><td>Export / render to WAV</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd></td><td>Commit mixer overrides to source</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd></td><td>Toggle Zen mode</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>,</kbd> / <kbd>F1</kbd></td><td>Settings / keyboard shortcuts</td></tr>
  </tbody>
</table>

<h2>Settings</h2>
<p>grove's settings live in the typed <code>[grove]</code> section of the global Arbor config (open them from the gear menu or <kbd>Ctrl</kbd>+<kbd>,</kbd>). Changes apply immediately.</p>
<ul class="prop-list">
  <li><code>Default octave</code> The octave assigned to a bare note name (e.g. <code>c</code> → <code>c4</code>).</li>
  <li><code>Default tempo</code> Cycles-per-second used when a file omits <code>cps()</code>.</li>
  <li><code>Log threshold</code> The minimum level emitted. Lines below it are never produced or transmitted — no IPC flood, even at <code>trace</code>.</li>
  <li><code>Sample rate</code> / <code>Bit depth</code> Format of the offline WAV render.</li>
  <li><code>Reverb tail</code> Extra seconds rendered after the last event so reverb / delay tails aren't cut off.</li>
</ul>

<h2>VSCO 2 samples</h2>
<p>The default synth voices are always available. The VSCO 2 Community Edition sample bank adds orchestral sampler instruments (strings, brass, winds, percussion). Download it from the <strong>Sound bank</strong> panel — it installs on a background job with a progress bar you can cancel, and the new instruments appear automatically once extraction finishes.</p>
<p>A <code>.inst("…")</code> or <code>s("…")</code> name the engine can't resolve to a built-in synth or an installed instrument is underlined as an <strong>error</strong> in the editor, so a typo surfaces immediately instead of silently playing a fallback voice.</p>
