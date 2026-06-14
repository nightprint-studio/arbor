<!-- Nemus docs — Generators. Plain semantic HTML; DocsShell supplies the typography. -->
<h1>Generators</h1>
<p class="doc-lead">
  Generators write music for you — euclidean rhythms, chord progressions, randomised
  variation — either as language constructs you type, or as dialogs that build the notation
  and drop it into your file.
</p>

<h2>Euclidean rhythms</h2>
<p>
  <code>(n, k)</code> spreads <code>n</code> hits as evenly as possible over <code>k</code>
  steps — the basis of countless grooves. A third number rotates the pattern:
</p>
<pre><code>s(bd(3,8))        // three kicks over eight steps
n(c2(5,8,2))      // five notes over eight, rotated by two</code></pre>
<p>
  The <strong>Insert euclidean rhythm…</strong> dialog (Command Palette) builds one
  interactively: pick the leaf, set hits / steps / rotation, preview it, and insert at the cursor.
</p>

<h2>Chord progressions</h2>
<p>
  <strong>Insert chord progression…</strong> (Command Palette) writes a scale-aware progression:
  choose a root + mode, type the degrees (Roman <code>I IV V</code> or numbers <code>1 4 5</code>),
  pick triads or sevenths, preview, and insert. It emits stacked scale degrees so the engine
  voices each chord diatonically — e.g. <code>n(0 4 5 3 &amp; 2 6 7 5 &amp; 4 8 9 7).scale("c:major")</code>.
</p>

<h2>Humanize</h2>
<p>
  <code>.humanize(time?, gain?)</code> nudges every onset and wobbles every level by a small
  random amount, seeded per onset so it's identical every loop — enough to take the stiffness off
  a programmed part. Apply it from <kbd>Alt</kbd> + <kbd>Enter</kbd> → <strong>Humanize
  pattern</strong>, or type it directly.
</p>

<h2>Freeze to notes</h2>
<p>
  Generated a phrase you like? Select it and <kbd>Alt</kbd> + <kbd>Enter</kbd> →
  <strong>Freeze pattern to notes</strong> rewrites it in place as the concrete notes it produces
  over one cycle — handy for committing a happy accident, then editing it by hand. (See
  <em>Scratch &amp; testing</em>.)
</p>

<h2>Randomness &amp; motion</h2>
<ul>
  <li><code>rand(lo, hi)</code> is a per-event random value; <code>choose(a, b, c)</code> picks one option per cycle. Both are <strong>seeded by cycle</strong>, so every loop is identical.</li>
  <li><code>degrade</code> / <code>degradeBy(p)</code> drop random events; <code>sometimes(f)</code> / <code>sometimesBy(p, f)</code> apply a transform to a random share of them.</li>
  <li>The continuous signals <code>sine</code>, <code>saw</code>, <code>isaw</code>, <code>tri</code>, <code>square</code> are LFOs you rescale with <code>.range(lo, hi)</code> and feed into any control: <code>.lpf(sine.range(400, 3000).slow(4))</code>.</li>
</ul>

<div class="callout accent">
  The full, searchable list — every generator, signal and operator with its signature and an
  example — is in the right-rail <strong>Language reference</strong> panel.
</div>
