<script lang="ts">
  /** Tyto docs — Captures library. */
</script>

<h1>Captures library</h1>
<p class="doc-lead">
  Every recording and screenshot lands in the library dock on the right. Toggle
  it with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd> or the rail button.
</p>

<h2>Finding captures</h2>
<p>
  The header shows how many captures you have and the total space they occupy.
  Filter the list by name or target with the search box.
</p>

<h2>Working with a capture</h2>
<ul>
  <li><b>View</b> — open the preview: screenshots show the real image, recordings
    play inline with full controls, alongside the metadata (target, size, when it
    was taken).</li>
  <li><b>Play a frame sequence</b> — a recording saved as frames opens in its own
    player: play/pause (<kbd>Space</kbd>), a draggable timeline, single-frame steps
    with <kbd>←</kbd> / <kbd>→</kbd> (hold <kbd>Shift</kbd> for ten), <kbd>Home</kbd>
    / <kbd>End</kbd>, looping (<kbd>L</kbd>) and 0.5× / 1× / 2× speed. It plays at
    real speed: each frame is shown for exactly as long as it was on screen.</li>
  <li><b>Export as sprite atlas</b> — only for frame sequences. Folds the whole
    recording into PNG <b>pages</b> plus an <code>atlas.ron</code> sheet that says
    where every frame sits and how long it is held, written to an
    <code>.atlas</code> folder beside the recording. Press <kbd>E</kbd> in the player,
    or use the button in the list. A recording too long for one page spills into
    <code>atlas_001.png</code>, <code>atlas_002.png</code> and so on, and each frame
    records which page it belongs to. Each frame gets a one-pixel guard of its own repeated
    border, so scaling the atlas can't smear one frame's edge into the next.</li>
  <li><b>Rename</b> — edit the name inline.</li>
  <li><b>Reveal in folder</b> — jump to the file on disk.</li>
  <li><b>Delete</b> — remove a single capture.</li>
</ul>

<div class="callout">
  <b>Why an atlas.</b> A renderer draws a sequence far more cheaply from one texture
  than from hundreds of files: an atlas is one upload and a rectangle that moves. The
  sheet carries each frame's real duration, so the exported atlas keeps the original
  timing even though identical frames were never stored — a still screen costs one
  frame, not thirty a second. Pages stop at 4096 px per edge by default; 8192 is the
  most any mainstream GPU accepts.
</div>

<div class="callout">
  <b>Delete all</b> removes every capture from disk — files and whole frame-sequence
  folders alike. It asks for confirmation first, and cannot be undone.
</div>
