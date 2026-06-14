<span class="eyebrow">Music</span>
<h1>nemus — Music live-coding</h1>

<p class="doc-lead">nemus is a standalone music live-coding studio that ships inside Arbor — its own window with a code editor, a read-only arrangement view, a mixer, and a live audio engine. You write music as <code>.nemus</code> source; the engine evaluates it and plays it back in real time.</p>

<div class="hint">nemus is <strong>code-first</strong>: the source is the instrument. The panels are feedback and fine-tuning — the arrangement, mixer and inspector reflect what the code produces, they don't replace it.</div>

<h2>Opening nemus</h2>
<p>Open the nemus window from the Command Palette — search for <em>Open nemus (Music)</em>. It opens in its own window, independent of any repository, so you can keep it alongside your work (a game's <code>music/</code> folder can live in the same project as its code).</p>

<h2>Projects</h2>
<p>A nemus <strong>project</strong> is a folder with a <code>nemus.toml</code> manifest (a <code>name</code> and an <code>audience</code> — "who the music is for") plus its <code>.nemus</code> files. Files listed under <code>libraries</code> are imported-only: their <code>tracks(…)</code> output is ignored, only their <code>fn</code> / <code>let</code> declarations are exported for re-use.</p>
<ol class="step-list">
  <li>Create a project (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>) or open one (<kbd>Ctrl</kbd>+<kbd>O</kbd>) — pick a folder.</li>
  <li>The entry <code>.nemus</code> opens as an editor tab; the <strong>Files</strong> panel lists the rest.</li>
  <li>Switch projects from the title-bar project dropdown — it remembers your recents.</li>
</ol>

<h2>The window</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Editor</div>
    <div class="fc-desc">CodeMirror with nemus syntax highlighting, inline error underlines, and a live highlight of whatever is sounding right now. Edits re-evaluate automatically. Autocomplete (<kbd>Ctrl</kbd>+<kbd>Space</kbd>) suggests the language's combinators, transforms and your own declarations — and, inside <code>inst("…")</code>, every instrument the engine can resolve, and inside <code>art("…")</code>, the available articulations; hover a name for its signature and docs. The usual editing comforts are there too: comment toggling, bracket / quote autoclosing, soft wrapping, and folding of multi-line blocks. If the open file changes on disk outside nemus, it offers to reload it.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Arrangement</div>
    <div class="fc-desc">A read-only, Logic-style timeline of the evaluated tracks, with a playhead that follows the transport. Note events show as clean piano-roll blocks; the toolbar toggles let you turn on a waveform overlay (drawn only on audio / sample lanes), the grid, note labels, and playhead-follow. Named <code>section(…)</code> blocks show as coloured ruler chips and tinted lane bands. Hover an event for its note / position / length; click it to load it into the Inspector. Click the ruler to seek; right-click a lane to mute / solo.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Mixer</div>
    <div class="fc-desc">One strip per track with live meters and gain / pan knobs. Dragging a knob is heard instantly (a live override) and is <strong>written back</strong> into the <code>.nemus</code> source as a <code>.gain(…)</code> / <code>.pan(…)</code> literal shortly after the gesture rests — no commit step. The room knob and the Inspector's delay knobs are code-first the same way. Muting a track writes <code>.gain(0)</code> into the source; unmuting restores the previous gain. Each strip (and the master) has a <strong>clip light</strong> that latches red if its output reaches 0 dBFS during playback; it also surfaces as a <strong>CLIP</strong> badge in the footer. Click either to reset — and each playthrough starts clean. Two checks catch clipping <strong>without playback</strong>: as you type, an event whose <strong>authored gain is boosted well above unity</strong> (e.g. <code>.gain(3)</code>) is underlined in red; and <strong>Check levels</strong> (Command Palette) runs a silent offline render of the loop and reports the <em>real</em> overloads — the sum of simultaneous voices through the FX — lighting the clip LEDs of the tracks that clip and underlining the notes sounding at each clip (hover for how far over 0 dBFS). The analysis clears when you edit; re-run it when ready.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Preview</div>
    <div class="fc-desc">A docked audition panel for hearing an instrument before you commit to it. A pitched voice plays from a full-width on-screen piano (click, drag to glide, or focus it and play two octaves from the computer keyboard — the <kbd>Z</kbd>… row is the low octave, the <kbd>Q</kbd>… row the one above; the octave buttons move the range); a one-shot plays from a single trigger. Gain / Velocity / Reverb / Speed / Pan knobs shape the note, a <strong>Scale</strong> + root turns the keyboard into scale degrees, and a free <strong>chain</strong> field appends any DSL you like (e.g. <code>.lpf(800).crush(4)</code>) — under the hood each press is a tiny <code>.nemus</code> snippet the engine evaluates, so the whole language is available. It routes through a dedicated bus that bypasses the song mixer, so it's heard cleanly even while a song plays. Open it from the Sound bank's preview button or by <kbd>Ctrl</kbd>+<kbd>Click</kbd> on an <code>inst("…")</code> / <code>s("…")</code> name in the editor.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Console &amp; Problems</div>
    <div class="fc-desc">The Console shows log lines gated to your threshold; Problems lists evaluation diagnostics — click a row to jump the editor to its source span. Press <kbd>Alt</kbd>+<kbd>F7</kbd> on a name for a floating list of its usages (↑/↓ to move, Enter to jump).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Jobs</div>
    <div class="fc-desc">Background work — offline WAV renders and sample-bank downloads — listed with live status and elapsed time. Cancel a running job, dismiss a finished one, or open it to follow its streaming output. The footer badge mirrors the running count.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Sound bank</div>
    <div class="fc-desc">The instruments the engine can resolve — the bare oscillators (<code>sine</code>, <code>sawtooth</code>, <code>square</code>, <code>triangle</code>, <code>pulse</code> + <code>saw</code> / <code>tri</code> / <code>sqr</code> / <code>sin</code> aliases), the detuned <code>supersaw</code>, the noise colours (<code>white</code>, <code>pink</code>, <code>brown</code>, <code>crackle</code>), the <code>synth.*</code> presets, and the samplers from any installed <strong>sample bank</strong>. Click a voice to copy its name; press its preview button to audition it in the <strong>Preview</strong> panel (an on-screen piano + knobs); open its info for a description and articulations, and filter the list by name. Download / manage the banks (VSCO 2, Dirt-Samples, drum machines, General MIDI) — each card shows a description and download-size estimate — from here.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Outline</div>
    <div class="fc-desc">The tracks, functions, constants and imports of the active file. Click a symbol to jump to its declaration; <kbd>Ctrl</kbd>+<kbd>Click</kbd> a name in the editor to follow it (cross-file too).</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Refactor</div>
    <div class="fc-desc">Reshape code without retyping it. <strong>Rename</strong> (<kbd>Shift</kbd>+<kbd>F6</kbd>) renames a <code>let</code> / <code>fn</code> / <code>import</code> and every use at once. <strong>Extract</strong> (<kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>) lifts a selected pattern into a named <code>let</code> and leaves the name in its place — the phrase-factoring move, on demand. <strong>Inline</strong> (<kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd>) is the inverse. <strong>Context actions</strong> (<kbd>Alt</kbd>+<kbd>Enter</kbd>) gathers the quick-fixes that apply where the caret is: fix an unresolved <code>inst("…")</code> by picking the closest installed instrument, transpose the note (or selection) by a semitone or octave in place, snap an out-of-scale note to the nearest degree of the enclosing <code>.scale("…")</code>, change that scale (re-spelling its notes to keep their degree), or reach rename / inline / extract. <strong>File structure</strong> (<kbd>Ctrl</kbd>+<kbd>F12</kbd>) jumps to any symbol; <strong>Format</strong> (<kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd>) reflows the file to canonical style.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Libraries</div>
    <div class="fc-desc">Reuse <code>.nemus</code> modules from GitHub. Declare them in <code>nemus.toml</code> under <code>[libraries]</code> (e.g. <code>drums = "github:owner/repo@v1"</code>), then import their <code>let</code> / <code>fn</code> with <code>import &#123; … &#125; from "$lib/drums/groove.nemus"</code>. <strong>Sync libraries</strong> (Command Palette) downloads each one pinned to a commit SHA into a shared cache and records it in <code>nemus.lock</code> for reproducible builds; opening a project auto-fetches any that are missing. Public GitHub repos.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Inspector</div>
    <div class="fc-desc">Detail for the selected track: voice, meters, the live mix values, pattern statistics (hap count, pitch range, peak <strong>voices</strong>), and the code-first <strong>delay</strong> knobs (time / feedback / mix) that write a <code>.delay(…)</code> into the source. Clicking an event in the arrangement also shows that event's detail (note, position, length) here. The footer tracks the live voice count and the <strong>DSP load</strong> (the audio CPU budget — it tints amber, then red, as it runs hot); the voices tooltip names the heaviest track so you know what to thin out.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Zen &amp; collapse</div>
    <div class="fc-desc">Zen mode (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd>) hides the chrome; the title-bar toggles collapse the arrangement or the editor so one fills the body.</div>
  </div>
</div>

<h2>The language</h2>
<p>A <code>.nemus</code> file is <strong>code</strong>, not a piano roll. You build patterns with a small host language and a compact <em>mini-notation</em> for rhythms and melodies, then compose them into named tracks. The full reference — every combinator, transform, signal and operator with its signature and an example — lives in the window's <strong>Docs</strong> panel (and as live autocomplete + hover in the editor); this is the narrative tour.</p>

<h3>Host language</h3>
<p>The host language names and composes patterns. The essentials:</p>
<ul class="prop-list">
  <li><code>let name = expr</code> binds a value; <code>fn name(params) = expr</code> defines an expression-bodied function (no recursion — the language is total, so it always terminates).</li>
  <li><code>import {'{'} kick, snare {'}'} from "lib/drums.nemus"</code> pulls top-level declarations from another file (its own <code>tracks(…)</code> output is ignored).</li>
  <li>Arithmetic (<code>+ - * /</code>, unary minus) and Rust-style ranges (<code>0..8</code> exclusive, <code>0..=7</code> inclusive) are available; call a method on a range with parentheses: <code>(0..8).map(…)</code>.</li>
  <li>Every function is called with parentheses and comma-separated arguments — the one exception is the mini-notation islands.</li>
</ul>
<pre class="code-block">let bass = n(c2 g1).inst("synth.bass")
fn bassline(root) = n($root ~ $root g1).lpf(800)</pre>

<h3>Islands &amp; mini-notation</h3>
<p>The two islands carry mini-notation — a space-separated mini-language for one cycle. <code>s(…)</code> (alias <code>sound</code>) holds <strong>sample names</strong> (<code>bd</code>, <code>sd</code>, <code>hh</code>); <code>n(…)</code> (alias <code>note</code>) holds <strong>pitches</strong> (<code>c4</code>), <strong>scale degrees</strong> (<code>0 2 4</code>) or <strong>chords</strong> (<code>c4'min7</code>), played by an instrument. The structural operators are identical between them:</p>
<table class="shortcuts-table">
  <thead><tr><th>Operator</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>~</code></td><td>a silent slot (rest)</td></tr>
    <tr><td><code>_</code></td><td>extend the previous term by a slot</td></tr>
    <tr><td><code>[ ]</code></td><td>group events into one slot (nestable)</td></tr>
    <tr><td><code>&lt; &gt;</code></td><td>alternation — one element per cycle</td></tr>
    <tr><td><code>&amp;</code></td><td>parallel (stack), the loosest-precedence operator</td></tr>
    <tr><td><code>*n</code> / <code>/n</code></td><td>fast / slow inside the slot</td></tr>
    <tr><td><code>!n</code> / <code>@n</code></td><td>replicate as separate slots / weight (more duration; <code>n</code> may be fractional, e.g. <code>bd@1.5</code>)</td></tr>
    <tr><td><code>(n,k)</code></td><td>euclidean — distribute n hits over k steps</td></tr>
    <tr><td><code>:n</code> / <code>'chord</code></td><td>sample variant (s only) / chord (n only)</td></tr>
    <tr><td><code>$ident</code></td><td>splice a named variable in as a leaf</td></tr>
  </tbody>
</table>
<pre class="code-block">s(bd [hh hh] sd ~)        // a kick, two fast hats, a snare, a rest
s(bd(3,8))                // the euclidean tresillo
n(&lt;c4'min7 af3'maj7&gt;)     // one chord per cycle</pre>

<h3>Transforms</h3>
<p>A <strong>transform</strong> turns a pattern into another pattern. It has two forms: as a <em>method</em> it applies (<code>pat.gain(0.4)</code>); standalone it is a reusable <em>transform value</em> (<code>gain(0.4)</code>), which you pass to the higher-order transforms (<code>every</code>, <code>off</code>, <code>sometimes</code>, <code>jux</code>) without a lambda. A nullary transform like <code>rev</code> is a value as a bare name.</p>
<pre class="code-block">arp.every(4, rev)              // reverse every 4th cycle
lead.off(0.125, gain(0.4))     // an echo, an eighth later, quieter
hats.degrade().gain(0.8)       // drop ~half the hits (seeded, stable per loop)</pre>
<p>The vocabulary covers <strong>time &amp; structure</strong> (<code>fast</code>, <code>slow</code>, <code>rev</code>, <code>iter</code>, <code>chunk</code>, <code>palindrome</code>, <code>swingBy</code>, <code>humanize</code>), <strong>probability</strong> (<code>degrade</code>, <code>degradeBy</code>, <code>sometimes</code>, <code>sometimesBy</code>, <code>jux</code>), and <strong>voice &amp; mix</strong> (<code>gain</code>, <code>pan</code>, <code>room</code>, <code>lpf</code>, <code>hpf</code>, <code>delay</code>, <code>crush</code>, <code>shape</code>, <code>shift</code>, <code>speed</code>, <code>vel</code>, <code>inst</code>, <code>art</code>, <code>scale</code>, <code>add</code>, <code>addDeg</code>). Voice/mix parameters accept a constant <em>or</em> a pattern/signal, so they can vary per event: <code>.lpf(sine.range(400, 2000))</code> sweeps the cutoff with an LFO; <code>.pan(rand(0, 1))</code> randomises stereo.</p>
<p><code>.humanize(t, v)</code> makes a quantised line breathe: it nudges each onset by up to <code>t</code> cycles and wobbles its gain by up to <code>±v</code> (both seeded per onset, so the feel is identical every loop). With no arguments it applies gentle defaults — <code>hats.humanize()</code>. The <em>Humanize pattern</em> quick-fix (Alt+Enter on a selection) wraps the pattern for you.</p>
<p><code>.art("legato")</code> plays a part <strong>monophonically and connected</strong>, so a melodic line flows note-to-note instead of detaching at every step: a synth re-pitches one held voice with no re-attack, while a sampler crossfades briefly from each note into the next (masking the sample's recorded onset). A rest breaks the line (the next note starts fresh), and a chord stays polyphonic. Other articulations (<code>"staccato"</code>, <code>"pizzicato"</code>, …) are detached and, on a sampler that ships them, also select the matching sample set.</p>

<h3>Composing &amp; arranging</h3>
<p>Combinators glue patterns together: <code>par</code> stacks them (play at once), <code>seq</code> lays them out in equal slots within a cycle, <code>cat</code> plays one per cycle. <code>arrange</code> places <code>cycles(n, x)</code> / <code>section("NAME", n, x)</code> blocks along the absolute timeline (and loops). Mapping a range with <code>.par</code>/<code>.seq</code>/<code>.cat</code> is the shortcut for "make N variations and combine them":</p>
<pre class="code-block">(0..8).par(i =&gt; n($i).off(i*0.1, gain(0.5)))     // eight detuned, staggered voices

arrange(
  section("INTRO", 4, intro),
  section("MAIN", 16, mainGroove),
  section("OUTRO", 4, outro),
)</pre>

<h3>Output: tracks (the mixer is code)</h3>
<p>A file's output is <code>tracks(track("name", pattern), …)</code> — a list of named channels, which <em>are</em> the mixer strips. The mixer is therefore code-first: there is no separate session file. <code>arrange</code> / <code>cat</code> / <code>par</code> are used <em>inside</em> a track for its own timeline.</p>
<pre class="code-block">tracks(
  track("bass",  bassline(c2)),
  track("drums", arrange(cycles(4, ~), cycles(24, drumGroove), cycles(4, ~))),
)</pre>

<h3>Generators &amp; signals</h3>
<p>Where a value is needed you can compute one. <strong>Generators</strong> produce values: <code>rand(lo, hi)</code> is a per-event random in a range, <code>choose(a, b, c)</code> picks one. <strong>Signals</strong> (<code>sine</code>, <code>saw</code>, <code>isaw</code>, <code>tri</code>, <code>square</code>) are continuous 0..1 LFOs you rescale with <code>.range(lo, hi)</code> and reshape with <code>.fast</code> / <code>.slow</code>. Both are <strong>seeded by cycle</strong>, so they're identical every loop — the same bar always sounds the same.</p>

<h3>Files &amp; samples</h3>
<p><code>sample("path")</code> loads an audio file as a one-shot (pitch with <code>.shift</code>), <code>audio("path")</code> loads a long stem that plays in full. Paths are project-relative.</p>

<h2>Editing from the panels</h2>
<p>The mixer and inspector knobs are a surgical bridge back to the source — the code stays the single source of truth:</p>
<ul class="prop-list">
  <li><code>gain</code> / <code>pan</code> are <strong>live + write-through</strong>: dragging a knob is heard instantly (a live override) and, once the gesture rests, the value is written into the source as a <code>.gain(…)</code> / <code>.pan(…)</code> literal on its own. <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd> flushes any pending write early.</li>
  <li><strong>Mute</strong> writes <code>.gain(0)</code> into the track's source (so the silence lives in the file); unmuting restores the gain it had before. <strong>Solo</strong> stays live-only — it has no source representation. The explicit mute flag is the source of truth across re-evaluations.</li>
  <li><code>room</code> (mixer) and <code>delay</code> (inspector: time / feedback / mix) are <strong>code-first</strong>: turning a knob writes the literal straight into the track's <code>.nemus</code> source — adding the method to the chain if it isn't there yet — and re-evaluates.</li>
  <li>A knob whose value is <em>calculated</em> in the source (e.g. <code>.gain(rand(0,1))</code>) is shown read-only: there is no single literal to rewrite. Muting such a track works live but can't be written to the source, and the strip flags that.</li>
</ul>
<div class="hint">Commits are ordinary editor edits — one <kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes them.</div>

<h2>Sections</h2>
<p>Wrap an arrangement block in <code>section("NAME", cycles, pattern)</code> — the named counterpart of <code>cycles(…)</code> inside <code>arrange(…)</code> — to label a stretch of the timeline. Named sections surface in the arrangement as coloured ruler chips and tinted lane bands, tiled across the loop, so the song's macro-structure (intro / build / drop / outro) reads at a glance.</p>

<h2>Tempo</h2>
<p><code>cps(n)</code> sets a constant clock (cycles-per-second). For tempo that changes over the song, use a <strong>tempo map</strong>: <code>tempo(cycles(8, 0.5), cycles(16, 0.6))</code> plays 8 cycles at 0.5 cps, then 16 at 0.6, then loops. The tempo changes on whole-cycle boundaries and the playhead position stays continuous; the footer shows the live tempo. (Smooth accelerando / rubato is a future addition — for now tempo steps between segments.)</p>

<h2>Key detection</h2>
<p>The footer continuously <strong>detects the key</strong> of the playing material — it fits the best scale (major, the modes, melodic / harmonic minor, and pentatonics including <em>hirajoshi</em> / <em>in-sen</em> / <em>iwato</em> / <em>kumoi</em>) to your notes and shows it, e.g. <code>E♭ dorian</code>. Notes that fall <strong>outside</strong> the detected scale are underlined in amber in the editor — hover one to see which note and key (e.g. <em>C♯4 isn't in C harmonic minor</em>) — and the footer key readout turns amber; its tooltip reports the coverage and how many notes are out of scale. It's advisory — a chromatic passing note is fine — but it makes an accidental wrong note easy to spot.</p>

<h2>Playing &amp; rendering</h2>
<ol class="step-list">
  <li>Press <kbd>Shift</kbd>+<kbd>F9</kbd> to Run — the engine opens the audio device and starts the transport (the button turns red / shows Stop).</li>
  <li>Edit while it plays: changes re-evaluate and swap in at the next cycle boundary, with the arrangement and meters updating live.</li>
  <li>Seek by clicking the arrangement ruler, or with <kbd>←</kbd> / <kbd>→</kbd> / <kbd>Home</kbd> while it has focus.</li>
  <li>Export to WAV with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd> — an offline render that runs as a background job. The export dialog renders the arrangement's natural loop period once by default; set <em>Loops</em> to repeat it, and the dialog shows the resulting cycles and a live duration · size estimate before you pick a file (<kbd>Ctrl</kbd>+<kbd>Enter</kbd> to continue).</li>
</ol>

<h2>Importing audio &amp; MIDI</h2>
<p>The <strong>Import</strong> button on the arrangement toolbar (also <kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd>, or the Command Palette) brings outside material in:</p>
<ul class="prop-list">
  <li><strong>A MIDI file</strong> (<code>.mid</code>) opens directly as an editable <code>.nemus</code> file — a deterministic, faithful conversion: notes are quantised to a grid, a key/scale is detected (so the result uses scale degrees and <code>.scale(...)</code> where it fits, including non-Western modes like <em>hirajoshi</em> and <em>in-sen</em>), chords are recognised into symbols, and repeating bars collapse into a loop. A long take isn't dumped as one giant pattern: the timeline is split into short phrases, repeated sections (a chorus, a refrain) are factored into reusable <code>let</code> variables, and the file plays them back through an <code>arrange(section(…))</code> structure — so the chorus is written once and every section is easy to find and edit.</li>
  <li><strong>Audio</strong> (WAV and common formats) is first <em>transcribed</em> to MIDI, then offered two ways: <em>Import as .nemus</em> (the transient MIDI never touches disk — you get an editable file straight away) or <em>Convert to MIDI file</em> (saves a <code>.mid</code> you can use elsewhere). Transcription is a background job with a progress bar in <strong>Downloads &amp; Exports</strong>.</li>
</ul>
<p>The built-in transcriber is fast and approximate (a monophonic melody plus a drum part) — a starting point you refine in the editor. The result opens in its own tab, ready to play.</p>
<p>For much better results on real, polyphonic audio, download the <strong>basic-pitch</strong> model from <strong>Settings → Transcription models</strong> (or the Command Palette: <em>Download polyphonic model</em>) — once present, audio import automatically uses it (polyphonic, chord-aware) instead of the DSP fallback. An optional <strong>Demucs</strong> model adds stem separation — once installed it engages automatically: the mix is split so drums are read from the isolated kit and pitch from a drum-free blend, giving noticeably cleaner notes. Models download on demand (the runtime is built in) and progress shows in <strong>Downloads &amp; Exports</strong>. Inference runs on the GPU (via DirectML) when one is available, falling back to the CPU otherwise.</p>

<h2>Command Palette</h2>
<p>The nemus window has its own Command Palette (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>) listing every window action — transport, project operations, panel toggles, settings. Type to filter, <kbd>↑</kbd> / <kbd>↓</kbd> to move, <kbd>Enter</kbd> to run.</p>

<h2>Keyboard shortcuts</h2>
<p>The full nemus cheat-sheet is in the window's gear menu under <em>Keyboard Shortcuts</em> (<kbd>F1</kbd>). The essentials:</p>
<table class="shortcuts-table">
  <thead><tr><th>Shortcut</th><th>Action</th></tr></thead>
  <tbody>
    <tr><td><kbd>Shift</kbd>+<kbd>F9</kbd></td><td>Run / Stop the transport</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd></td><td>Command Palette</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>O</kbd> / <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd></td><td>Open / new project</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd></td><td>Open a <code>.nemus</code> file</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>N</kbd></td><td>New <code>.nemus</code> file (editor)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>S</kbd></td><td>Save the active file</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>G</kbd></td><td>Go to line (editor)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>/</kbd> / <kbd>Ctrl</kbd>+<kbd>Y</kbd></td><td>Toggle comment / delete line (editor)</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>F7</kbd></td><td>Find usages of the symbol at the caret (editor)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>F12</kbd></td><td>File structure — jump to any track / fn / let / import (editor)</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd></td><td>Format document — reformat to canonical style (editor)</td></tr>
    <tr><td><kbd>Shift</kbd>+<kbd>F6</kbd></td><td>Rename the symbol under the caret + all its uses (editor)</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd></td><td>Extract the selected pattern into a named <code>let</code> (editor)</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>N</kbd></td><td>Inline the <code>let</code> under the caret into its uses (editor)</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Enter</kbd></td><td>Context actions / quick-fixes at the caret (editor)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Click</kbd></td><td>Go to declaration · preview an <code>inst("…")</code> / <code>s("…")</code> name (editor) · reveal hap source (arrangement)</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>F</kbd></td><td>Find in file (editor focused) · else search the Console / Problems</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>R</kbd></td><td>Export / render to WAV</td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>I</kbd></td><td>Import audio / MIDI as <code>.nemus</code></td></tr>
    <tr><td><kbd>Alt</kbd>+<kbd>Shift</kbd>+<kbd>C</kbd></td><td>Commit mixer overrides to source</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Z</kbd></td><td>Toggle Zen mode</td></tr>
    <tr><td><kbd>Ctrl</kbd>+<kbd>,</kbd> / <kbd>F1</kbd></td><td>Settings / keyboard shortcuts</td></tr>
  </tbody>
</table>

<h2>Settings</h2>
<p>nemus's settings live in the typed <code>[nemus]</code> section of the global Arbor config (open them from the gear menu or <kbd>Ctrl</kbd>+<kbd>,</kbd>). Changes apply immediately.</p>
<ul class="prop-list">
  <li><code>Default octave</code> The octave assigned to a bare note name (e.g. <code>c</code> → <code>c4</code>).</li>
  <li><code>Default tempo</code> Cycles-per-second used when a file omits <code>cps()</code>.</li>
  <li><code>Output device</code> Where playback is sent — pick any of the system's audio outputs, or leave it on the system default. Switching it moves a running session to the new device immediately.</li>
  <li><code>Log threshold</code> The minimum level emitted. Lines below it are never produced or transmitted — no IPC flood, even at <code>trace</code>.</li>
  <li><code>Sample rate</code> / <code>Bit depth</code> Format of the offline WAV render.</li>
  <li><code>Reverb tail</code> Extra seconds rendered after the last event so reverb / delay tails aren't cut off.</li>
</ul>

<h2>Sample banks</h2>
<p>The synth voices are always available. <strong>Sample banks</strong> add sampled instruments, downloaded on demand from the <strong>Sound bank</strong> panel — each card shows what the bank contains and an estimated download size, installs on a background job with a progress bar you can cancel, and its instruments appear once the install finishes (grouped under their bank in the <strong>Samplers</strong> list). An installed bank's card offers a <strong>Re-index</strong> action that rebuilds its instrument list from the files already on disk (no re-download — use it if a bank shows zero instruments) and a <strong>Delete</strong> action that removes its samples from disk; you can re-download it any time:</p>
<ul class="prop-list">
  <li><strong>VSCO 2</strong> — orchestral samplers indexed from the bank's raw samples into playable instruments named <code>&lt;family&gt;.&lt;instrument&gt;</code> — e.g. <code>strings.violin_section</code>, <code>strings.cello_section</code>, <code>brass.trumpet</code>, <code>ww.flute</code>. The bare name plays the sustain; each other articulation is its own voice with a <code>.&lt;articulation&gt;</code> suffix (<code>strings.violin_section.pizzicato</code>, <code>…spiccato</code>, <code>…tremolo</code>), so a voice only loads its own samples when you name it.</li>
  <li><strong>VCSL</strong> — the Versilian Community Sample Library (VSCO 2's sibling): pitched instruments index the same way (<code>winds.flute</code>, <code>mallets.vibraphone</code>, <code>strings.*</code>), while its unpitched percussion / one-shots become short <code>s("…")</code> names (<code>s("anvil")</code>, <code>s("clap")</code>, <code>s("woodblock")</code>, …).</li>
  <li><strong>Dirt-Samples</strong> — the Tidal/Strudel sample set (<code>bd</code>, <code>sd</code>, <code>hh</code>, <code>casio</code>, <code>jazz</code>, …).</li>
  <li><strong>Drum machines</strong> — classic boxes (<code>RolandTR808_bd</code>, <code>RolandTR909_sd</code>, <code>LinnDrum_*</code>, …).</li>
  <li><strong>General MIDI</strong> — the 128 GM instruments from a soundfont, converted to samplers on install (<code>gm_acoustic_grand_piano</code>, <code>gm_violin</code>, … and <code>gm_drums</code>).</li>
</ul>
<p>A sound that ships several samples exposes <strong>variants</strong>: <code>s("bd:3")</code> plays the fourth; with no index they round-robin per onset so repeats vary.</p>
<p>Bank downloads and WAV exports also collect in the <strong>Downloads &amp; Exports</strong> overlay — a badge always present in the footer that opens a list of every in-flight (and just-finished) transfer, each with its own live progress bar. A finished transfer offers <strong>Reveal in file explorer</strong> (the built-in explorer or your OS file manager, per <em>Settings → File Explorer</em>).</p>
<p>A <code>.inst("…")</code> or <code>s("…")</code> name the engine can't resolve to a built-in synth or an installed instrument is underlined as an <strong>error</strong> in the editor, so a typo surfaces immediately instead of silently playing a fallback voice.</p>
