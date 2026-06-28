<h1>Importing audio &amp; MIDI</h1>
<p class="doc-lead">
  Bring existing material in as an editable <code>.merula</code> file: a MIDI file converts
  directly, and audio is transcribed to notes with optional ONNX models.
</p>

<h2>From the toolbar or palette</h2>
<ul>
  <li><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>I</kbd>, the <strong>Import</strong> button on the arrangement toolbar, or the command palette.</li>
  <li><strong>MIDI</strong> (<code>.mid</code>) converts deterministically: tracks, notes and tempo become a <code>tracks(…)</code> program with note-name or scale-degree islands.</li>
  <li><strong>Audio</strong> (<code>.wav</code>) is transcribed to MIDI first, then converted.</li>
</ul>

<h2>Transcription models</h2>
<p>
  Audio import uses optional models, downloaded on demand under
  <strong>Settings → Transcription</strong>:
</p>
<ul>
  <li><strong>basic-pitch</strong> — polyphonic, chord-aware pitch detection.</li>
  <li><strong>Demucs</strong> — splits a mix into stems for cleaner per-instrument notes.</li>
</ul>

<div class="callout">
  Import is best-effort: it gives you a real, editable starting point, not a perfect score.
  Clean it up with the editor's refactors (rename, extract, scale-snap) once it's in.
</div>
