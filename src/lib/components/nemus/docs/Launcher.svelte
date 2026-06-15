<!-- Nemus docs — Clip launcher & scenes. Plain semantic HTML; DocsShell supplies
     the typography. Code samples use NemusCode (real syntax highlight). -->
<script lang="ts">
  import NemusCode from '../editor/NemusCode.svelte';
</script>

<h1>Clip launcher &amp; scenes</h1>
<p class="doc-lead">
  The clip launcher is a session grid for performing the song live — like a hardware
  groovebox or a DAW's session view, but the clips are written in code. Declare
  <strong>scenes</strong> in the source, then fire their clips from the grid while the song plays.
</p>

<h2>Declaring clips</h2>
<NemusCode code={`tracks(
  track("drums", s(bd ~ sd ~),
    clip("chorus", s(bd bd sn bd)),
    clip("break",  s(bd ~ ~ ~)),
  ),
  track("bass", n(c2 g1),
    clip("chorus", n(c2 c2 ef2 g2)),
  ),
)`} />
<p>
  A <strong>clip</strong> is a launchable variation of <em>its own</em> track: add
  <code>clip("scene", pattern)</code> arguments after a track's base pattern. The clip belongs
  to that track, so there's nothing to wire up — its column in the grid is the track, and the
  <strong>scene</strong> is the <strong>row</strong> formed by every clip sharing that name
  across tracks. A track lists clips only for the scenes it varies in; the rest of the time it
  plays its base pattern. Scenes appear in the grid in the order their clips first show up.
</p>

<h2>The grid</h2>
<p>
  Open it with <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>G</kbd>, the grid icon on the bottom
  rail, or the Command Palette. Rows are scenes, columns are tracks:
</p>
<ul>
  <li><strong>Fire a clip</strong> (▶ in a cell) — swaps just that one track to the clip. Combine clips from different scenes this way to <strong>mix</strong>.</li>
  <li><strong>Fire a scene</strong> (▶ on the row) — sets the <strong>whole row as one picture</strong>: every track with a clip plays it, and every track <em>without</em> a clip in that row returns to base (an empty cell stops it).</li>
  <li><strong>Stop a clip</strong> — click an active cell again to return that track to what the code plays.</li>
  <li><strong>Stop all</strong> (top-left) — every track back to base, song keeps playing.</li>
  <li>Each <strong>column header</strong> shows which scene that track is currently playing, so a mixed state is readable at a glance.</li>
</ul>

<h2>Quantized launching</h2>
<p>
  Launches are quantized so they land in time. A fired clip lights up at once and
  <strong>pulses</strong> until the next grid line — <strong>1</strong>, <strong>2</strong> or
  <strong>4</strong> cycles, set with the <strong>QUANTIZE</strong> selector in the panel header —
  where the audio actually swaps and the cell goes solid.
</p>

<h2>Playing live</h2>
<p>
  The launcher rides the running song. Fire a clip from a <strong>stopped</strong> transport and
  the song <strong>starts</strong> with that clip; while it plays, fire more to reshape the
  arrangement on the fly. It never plays a clip in isolation — the tracks you don't touch keep
  playing what the code says. The transport <strong>Stop</strong> clears the launcher too (one
  stop); the grid's own <strong>Stop</strong> is the soft "clips off, song keeps going".
</p>

<div class="callout accent">
  Scenes are in the Command Palette too — <em>Launch scene …</em> fires a row without the mouse,
  and <em>Stop all clips</em> clears them.
</div>

<h2>What it's for</h2>
<ul>
  <li><strong>Arranging live</strong> — build up by firing tracks one at a time; switch the whole feel by launching a scene (day → night).</li>
  <li><strong>Song sections</strong> — keep verse / chorus / bridge as scenes and perform the structure.</li>
  <li><strong>Trying ideas</strong> — A/B different drum patterns or basslines while it plays, no editing.</li>
  <li><strong>Hybrids</strong> — mix the harp from one scene with the bass from another (fire individual cells).</li>
</ul>
