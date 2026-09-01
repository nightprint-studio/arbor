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
  It also reports <strong>Type names</strong>: how many distinct class names completion can offer,
  counting the JDK's, every resolved dependency jar's and your project's own. That figure is what
  tells <em>completion is not offering my library classes</em> apart from <em>the library classes
  were never loaded</em> — from the popup the two look the same, and only the first is about
  completion. A few thousand means the JDK alone answered; a project with jars runs to tens of
  thousands.
  If something looks stale or a class you know exists isn't turning up, press <strong>Rebuild</strong>
  there (or run <em>Rebuild index</em> from the palette) to invalidate the index and recompute it from
  scratch. This is a pure re-scan of the sources on disk — it doesn't compile the project (that's
  <kbd>Ctrl</kbd> + <kbd>F9</kbd>).
</p>

<h2>It keeps up with your edits</h2>
<p>
  The index is not a snapshot of the project as you opened it. As you type, the file you are editing
  is re-read into it — so a method you have just written has its usages, and one you have just
  stopped calling loses them. Nothing has to be saved, rebuilt or reopened for
  <strong>find-usages</strong>, <strong>rename</strong> and <strong>hover</strong> to agree with
  go-to-definition about what the code says.
</p>
<p>
  When an edit changes what a file <em>declares</em> — a method added, renamed, removed, a signature
  or a supertype changed — the files that resolve against it are re-read too, since their view of it
  is what just moved. Typing inside a method body changes nothing anyone else resolves, so it costs
  one file. <strong>Rebuild index</strong> is still there for the rare case where something looks
  stale, but it is no longer what you reach for after adding a method.
</p>
<p>
  <strong>Who extends whom</strong> keeps up in the same way. It matters more than it sounds: a
  method rename carries its whole <em>override family</em> with it, so a class that has just started
  implementing an interface has to be known to be one — otherwise the rename would move the
  interface's method and leave that class declaring the old name, no longer overriding anything.
  Changing an <code>extends</code> or <code>implements</code> clause re-files the type immediately.
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
