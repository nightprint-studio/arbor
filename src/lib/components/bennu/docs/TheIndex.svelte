<!-- Bennu docs — the index: what it holds, when it is built, and what it means to be warm. -->
<h1>The index</h1>
<p class="doc-lead">
  Nearly everything else in this manual is a question asked of the index. This is what it is, when
  it exists, and what an empty answer means while it is still building.
</p>

<h2>The index</h2>
<p>
  Completion, go-to-definition, find-usages, rename, hover and Go-to-Class all answer from a
  <strong>semantic index</strong> Bennu builds in the background when a project opens. The footer shows
  its progress and reads <em>Indexed · N types</em> once it's warm.
</p>
<p>
  The <strong>Index inspector</strong> (Command Palette → <em>Index inspector…</em>) browses what the
  index holds — types, members, jars, JDK, beans, actions and relations — with a filter and jump-to.
  If something looks stale or a class you know exists isn't turning up, press <strong>Rebuild</strong>
  there (or run <em>Rebuild index</em> from the palette) to invalidate the index and recompute it from
  scratch. This is a pure re-scan of the sources on disk — it doesn't compile the project (that's
  <kbd>Ctrl</kbd> + <kbd>F9</kbd>).
</p>

<div class="callout">
  Everything is reachable from the keyboard. The <strong>Command Palette</strong>
  (<kbd>Ctrl</kbd> + <kbd>K</kbd>) lists the editor and tool-window actions; the tool windows toggle
  with <kbd>Alt</kbd> + <kbd>1</kbd> / <kbd>2</kbd> (Project · Structure), <kbd>Alt</kbd> +
  <kbd>0</kbd> / <kbd>6</kbd> / <kbd>7</kbd> / <kbd>F12</kbd> (Build · Problems · TODO · Terminal), and
  <kbd>Alt</kbd> + <kbd>8</kbd> (Maven). Build the project with
  <kbd>Ctrl</kbd> + <kbd>F9</kbd> and run it with <kbd>Shift</kbd> + <kbd>F10</kbd>. In a Cargo
  project the Java-only tools and Run are hidden, and <kbd>Ctrl</kbd> + <kbd>F9</kbd> runs
  <code>cargo check</code>.
</div>
