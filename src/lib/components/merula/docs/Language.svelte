<script lang="ts">
  import MerulaCode from '../editor/MerulaCode.svelte';
</script>

<h1>Language basics</h1>
<p class="doc-lead">
  The host language is small and total — no loops, no recursion, so a file always
  finishes evaluating. You bind names, define functions, import from other files, and end
  with one output expression (usually <code>tracks(…)</code>).
</p>

<h2>Tempo</h2>
<MerulaCode code={`cps(0.6)                          // constant clock: cycles per second
tempo(cycles(8, 0.5), cycles(16, 0.6))  // piecewise tempo map (wins over cps)`} />
<p>
  <code>cps(n)</code> is a constant clock. <code>tempo(…)</code> is a piecewise-constant
  map: each <code>cycles(n, cps)</code> plays <code>n</code> cycles at that rate, stepping
  on cycle boundaries and looping over the total.
</p>

<h2>Front-matter</h2>
<MerulaCode code={`meta {
  title = "Inno alla Gioia"
  description = "Beethoven — Ode to Joy theme"
  tags = ["orchestral", "beethoven"]
}`} />
<p>
  An optional <code>meta &lbrace; … &rbrace;</code> block carries metadata (title, description, tags)
  shown in the Files panel. It is pure metadata — it does not sound.
</p>

<h2>Bindings &amp; functions</h2>
<MerulaCode code={`let bass = n(c2 g1).inst("synth.bass")   // a value (does not sound on its own)
fn echo(p) = stack(p, p.slow(2).gain(0.4))  // a one-shot function, no recursion`} />
<p>
  <code>let</code> binds a reusable pattern. <code>fn</code> defines an expression-bodied
  function; inside, a parameter is spliced into an island with <code>$name</code>.
</p>

<h2>Imports</h2>
<MerulaCode code={`import { kick, snare } from "lib/drums.merula"`} />
<p>
  Bring <code>let</code>/<code>fn</code> declarations from another file into scope. The
  imported file's own <code>tracks(…)</code> output is ignored.
</p>

<h2>The output</h2>
<MerulaCode code={`tracks(
  track("drums", s(bd ~ sd ~)),
  track("bass",  bass),
)`} />
<p>
  <code>tracks(…)</code> lays out one <code>track("name", pattern)</code> per strip, in
  order. A file may instead end with a single bare pattern to audition it alone.
</p>
