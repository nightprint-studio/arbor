<!-- Nemus docs — Sound bank & instruments. Plain semantic HTML; DocsShell supplies
     the typography. -->
<h1>Sound bank &amp; instruments</h1>
<p class="doc-lead">
  Every pattern needs a voice. nemus ships a set of synths that always work, and resolves
  sample names against any <strong>sample packs</strong> you install. The Sound bank panel is
  where you browse, audition and download them.
</p>

<h2>Two ways to sound a pattern</h2>
<table>
  <thead><tr><th>Form</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><code>n(c4 e4 g4).inst("…")</code></td><td>play <strong>pitched notes</strong> through a named instrument (a synth, a GM voice, a sampled instrument)</td></tr>
    <tr><td><code>s(bd sd hh)</code></td><td>play <strong>samples by name</strong> — one hit per leaf (drums, one-shots, foley). The leaf <em>is</em> the sound</td></tr>
  </tbody>
</table>
<p>
  <code>.inst</code> chooses who sounds the notes; <code>s(…)</code> names sounds directly. A
  drum kit is the classic <code>s(…)</code> case; a melody is the classic <code>.inst</code> case.
</p>

<h2>Built-in synths (no pack needed)</h2>
<p>These resolve out of the box — nothing to download:</p>
<ul>
  <li><strong>Presets</strong> — <code>synth.bass</code>, <code>synth.sub</code>, <code>synth.pad</code>, <code>synth.pluck</code>, <code>synth.lead</code>, <code>synth.supersaw</code>, <code>synth.noise</code>, <code>synth.hat</code> (and bare <code>synth</code>, the soft default any <em>unresolved</em> name falls back to).</li>
  <li><strong>Bare oscillators</strong> — <code>sine</code>, <code>sawtooth</code>, <code>square</code>, <code>triangle</code>, <code>pulse</code>, plus the aliases <code>sin</code> / <code>saw</code> / <code>sqr</code> / <code>tri</code>, and the wide <code>supersaw</code>.</li>
  <li><strong>Noise colours</strong> — <code>white</code>, <code>pink</code>, <code>brown</code>, <code>crackle</code>.</li>
</ul>
<pre><code>n(c2 g1).inst("synth.bass")
n(c4 e4 g4).inst("triangle").lpf(2000)</code></pre>

<h2>Sample packs</h2>
<p>
  Download these from the Sound bank (each card shows a description and a size estimate). Once
  installed, their names resolve in <code>s(…)</code> / <code>.inst(…)</code>:
</p>
<table>
  <thead><tr><th>Pack</th><th>How you name it</th></tr></thead>
  <tbody>
    <tr><td><strong>Dirt-Samples</strong></td><td>bare leaves — <code>s(bd sd hh oh cp rim …)</code>, plus hundreds of melodic / foley folders</td></tr>
    <tr><td><strong>Drum machines</strong></td><td><code>&lt;Machine&gt;_&lt;drum&gt;</code> — <code>s(RolandTR808_bd RolandTR909_hh LinnDrum_sn)</code></td></tr>
    <tr><td><strong>General MIDI</strong></td><td><code>gm_&lt;name&gt;</code> for the 128 melodic programs (<code>gm_celesta</code>, <code>gm_flute</code>, <code>gm_koto</code>…), and <code>gm_drums</code> for the percussion kit</td></tr>
    <tr><td><strong>VSCO 2</strong> (orchestral)</td><td><code>family.instrument</code> — families <code>strings</code>, <code>brass</code>, <code>ww</code>, <code>keys</code>, <code>perc</code>, <code>guitar</code></td></tr>
  </tbody>
</table>

<div class="callout">
  <strong>Drum names are pack-specific.</strong> Bare <code>s(bd)</code> comes from
  <strong>Dirt-Samples</strong>. The <strong>Drum machines</strong> pack names every voice
  <code>&lt;Machine&gt;_&lt;drum&gt;</code> instead, so use <code>s(RolandTR808_bd …)</code> there.
  If a name underlines red (<em>unknown instrument</em>) it isn't in any installed pack — check
  the Sound bank for the exact spelling, or install the pack that has it. Until then it falls
  back to the default synth.
</div>

<h2>The GM drum kit</h2>
<p>
  <code>gm_drums</code> is one note-mapped kit (General-MIDI channel-10 keys), so you play it
  with <em>notes</em>, not leaf names:
</p>
<pre><code>// c2 kick · cs2 side-stick · d2 snare · ef2 clap · fs2 closed-hat · bf2 open-hat
n(fs2*8 &amp; ~ d2 ~ d2 &amp; c2 ~ ~ ~).inst("gm_drums")</code></pre>

<h2>Variants &amp; your own files</h2>
<ul>
  <li><strong>Sample variant</strong> — <code>s(bd:3)</code> picks the fourth sample in the <code>bd</code> folder (0-based). Without <code>:n</code> you get the first.</li>
  <li><strong>Project files</strong> — <code>sample("path")</code> loads an audio file as a one-shot (pitch it with <code>.shift</code>); <code>audio("path")</code> plays a long stem in full. Paths are project-relative.</li>
</ul>

<h2>The Sound bank panel</h2>
<ul>
  <li><strong>Browse &amp; filter</strong> every resolvable voice, grouped by kind; click one to copy its name into the editor.</li>
  <li><strong>Preview</strong> (the speaker button) auditions it on the Preview panel's on-screen piano — without touching the song.</li>
  <li><strong>Star</strong> a voice to keep it under <strong>Favourites</strong>; voices you use surface under <strong>Recently used</strong>.</li>
  <li><strong>Download / manage</strong> the packs — each download is a background job you can cancel.</li>
</ul>

<div class="callout accent">
  Lazy loading: at play time the engine decodes only the instruments your song actually
  references, so installing a multi-gigabyte pack never slows a patch that uses three voices.
</div>
