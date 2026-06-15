<script lang="ts">
  import NemusCode from '../editor/NemusCode.svelte';
</script>

<h1>Notes, chords &amp; scales</h1>
<p class="doc-lead">
  Pitches live in <code>n(…)</code> islands. A note name carries its octave; chords expand
  a root; scale degrees turn numbers into pitches.
</p>

<h2>Note names</h2>
<p>
  In the host, the octave is <strong>mandatory</strong>: <code>c4</code> is middle C
  (MIDI 60), <code>a3</code> is MIDI 57. Accidentals use <code>s</code> (sharp) and
  <code>f</code> (flat): <code>fs4</code> = F♯4, <code>ef3</code> = E♭3.
</p>
<NemusCode code={`n(c4 e4 g4 c5)     // an arpeggio
n(fs4 a4 d5)`} />

<h2>Chords</h2>
<NemusCode code={`n(c4'maj)          // C major triad
n(d3'min7)         // D minor seventh
n(g3'5)            // power chord (root + fifth)`} />

<h2>Scale degrees</h2>
<NemusCode code={`n(0 2 4).scale("c:major")   // degrees resolve against a scale → C E G
n(0 1 2).scale("a:minor")`} />
<p>
  Bare integers in a note island are scale degrees; <code>.scale("root:mode")</code>
  resolves them to pitches. Degree <code>0</code> is the tonic at the default octave.
  Stacking degrees a third apart in parallel lanes builds diatonic chords —
  <code>n(0 4 5 3 &amp; 2 6 7 5 &amp; 4 8 9 7).scale("c:major")</code> is a I–V–vi–IV
  progression whose chord qualities follow the scale.
</p>

<div class="callout">
  <strong>Insert chord progression…</strong> (Command Palette) builds exactly that for
  you: pick a key, a progression (<code>I V vi IV</code> or <code>1 5 6 4</code>) and
  triads or sevenths, preview it, and drop it in at the caret. <strong>Insert euclidean
  rhythm…</strong> does the same for <code>(n,k)</code> rhythms.
</div>

<h2>Transposing</h2>
<NemusCode code={`theme.add(12)      // up one octave (semitones)
theme.add(-24)     // down two octaves — e.g. a contrabass part`} />
<div class="callout">
  When a track is transposed, the arrangement shows the <em>sounding</em> pitch while
  Ctrl-clicking an event jumps to the <em>written</em> note. The tooltip shows both — e.g.
  <code>MIDI 33 · written a3</code> for an <code>a3</code> moved down two octaves.
</div>
