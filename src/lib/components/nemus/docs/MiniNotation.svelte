<h1>Mini-notation</h1>
<p class="doc-lead">
  Inside an island — <code>s(…)</code> for samples, <code>n(…)</code> for pitches — space
  is time. A row of tokens is one cycle; the tokens divide it evenly. Brackets, postfixes
  and operators bend that grid.
</p>

<h2>Sequences &amp; rests</h2>
<pre><code>s(bd hh sd hh)     // four even slots
s(bd ~ sd ~)       // ~ is a silent slot
n(c4 e4 _ g4)      // _ extends the previous note into the next slot</code></pre>

<h2>Grouping &amp; alternation</h2>
<pre><code>s(bd [hh hh] sd)   // [..] packs several events into one slot
s(bd &lt;sd cp&gt;)      // &lt;..&gt; plays one element per cycle (alternates)
s(&lbrace;bd sd, hh hh hh&rbrace;%4)  // &lbrace;..&rbrace;%n polymeter: lanes step at n slots/cycle</code></pre>

<h2>Postfixes</h2>
<table>
  <thead><tr><th>Postfix</th><th>Meaning</th></tr></thead>
  <tbody>
    <tr><td><code>*n</code></td><td>fast — repeat n times inside the slot (<code>bd*2</code>)</td></tr>
    <tr><td><code>/n</code></td><td>slow — play once every n cycles</td></tr>
    <tr><td><code>!n</code></td><td>replicate as n separate slots</td></tr>
    <tr><td><code>@n</code></td><td>weight — give the term n× the duration (<code>c4@3 e4</code>)</td></tr>
    <tr><td><code>(n,k)</code></td><td>Euclid — distribute n pulses over k steps (<code>bd(3,8)</code>)</td></tr>
    <tr><td><code>:n</code></td><td>sample variant index (sound islands only)</td></tr>
    <tr><td><code>'name</code></td><td>chord — expand a note into a chord (note islands only)</td></tr>
  </tbody>
</table>

<div class="callout accent">
  Factors can themselves be patterns: <code>bd*&lt;2 3&gt;</code> alternates the repeat count
  per cycle, <code>bd(&lt;3 5&gt;,8)</code> alternates the Euclid pulses.
</div>

<h2>Splicing host values</h2>
<pre><code>let riff = n(c4 e4 g4)
n($riff $riff a4)   // $name splices a bound pattern as a leaf</code></pre>
