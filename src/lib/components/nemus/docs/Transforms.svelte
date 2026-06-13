<h1>Transforms &amp; combinators</h1>
<p class="doc-lead">
  Patterns chain with methods. Combinators build bigger patterns from smaller ones;
  transforms shape sound and time. Every method returns a pattern, so they compose.
</p>

<h2>Combining patterns</h2>
<table>
  <thead><tr><th>Combinator</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>cat(a, b, …)</code></td><td>concatenate — one argument per cycle (a "bar per line")</td></tr>
    <tr><td><code>stack(a, b, …)</code></td><td>layer — play all at once</td></tr>
    <tr><td><code>choose(a, b, …)</code></td><td>pick one at random per cycle</td></tr>
  </tbody>
</table>
<pre><code>let theme = cat(m_a, m_b, m_c, m_d)   // 4 bars, one motif per cycle
stack(flute, oboe)                    // double a line across two instruments</code></pre>

<h2>Voicing &amp; mix</h2>
<table>
  <thead><tr><th>Transform</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>.inst("…")</code></td><td>choose the instrument that sounds the pattern</td></tr>
    <tr><td><code>.gain(x)</code></td><td>level (0–1)</td></tr>
    <tr><td><code>.pan(x)</code></td><td>stereo position (0 = left, 0.5 = centre, 1 = right)</td></tr>
    <tr><td><code>.room(x)</code></td><td>reverb send</td></tr>
    <tr><td><code>.art("legato")</code></td><td>articulation — e.g. tie notes so they don't re-attack</td></tr>
  </tbody>
</table>

<h2>Time &amp; pitch</h2>
<table>
  <thead><tr><th>Transform</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>.fast(n)</code> / <code>.slow(n)</code></td><td>speed the pattern up / down</td></tr>
    <tr><td><code>.rev</code></td><td>reverse within the cycle</td></tr>
    <tr><td><code>.add(n)</code></td><td>transpose by n semitones</td></tr>
    <tr><td><code>.scale("root:mode")</code></td><td>resolve degree leaves to pitches</td></tr>
  </tbody>
</table>

<pre><code>let violini = theme.inst("strings.violin_section").gain(0.9).room(0.18).art("legato")
let flauti  = theme.add(12).inst("ww.flute").gain(0.4)   // doubled an octave up</code></pre>

<div class="callout accent">
  See the <strong>Language reference</strong> panel for the full, searchable catalogue of
  islands, combinators, transforms, generators and signals — it's generated from the
  evaluator, so it never drifts.
</div>
