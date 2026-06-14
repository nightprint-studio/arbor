<h1>Scratch &amp; testing</h1>
<p class="doc-lead">
  Try out a fragment of music without committing it to the song. Evaluate any chunk
  of <code>.nemus</code> in isolation to see the events it generates, and play it
  one-shot to hear its effect — none of it touches the live arrangement.
</p>

<h2>The Scratch panel</h2>
<p>
  Open <strong>Scratch</strong> with <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>S</kbd>,
  the flask icon on the bottom rail, or the Command Palette. Paste or type a snippet,
  or press <strong>Load selection</strong> to pull in whatever is selected in the
  editor. As you edit, the panel evaluates the snippet and lists every event it
  produces (time · track · note/sound · gain); parse or instrument errors show inline.
  <kbd>Play</kbd> (or <kbd>Ctrl</kbd> + <kbd>Enter</kbd>) sounds it once over its
  detected loop length and stops on its own.
</p>
<p>
  Scratch is <strong>tabbed</strong> — keep several experiments side by side. The
  <kbd>+</kbd> opens a fresh tab, and <em>Load selection</em> / <em>Send to Scratch</em>
  each open the chunk in its own tab. <strong>Double-click a tab</strong> to give it a
  title. Scratch tabs <strong>persist across sessions</strong>, so your experiments are
  there when you come back. A snippet is resolved against the active file's constants,
  functions and imports, so a bare variable (or a no-argument function) evaluates and
  plays here too.
</p>

<h2>Quick play</h2>
<ul>
  <li><strong>Selection</strong> — select any expression in the editor and press
    <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Enter</kbd>, or right-click → <em>Play
    selection</em>. A bare variable plays its own notes — the selection is resolved
    against the file's constants, functions and imports, so references just work.
    With nothing selected, the whole file plays one-shot.</li>
  <li><strong>Right-click → Send to Scratch</strong> drops the selection into the
    Scratch panel for a closer look.</li>
  <li><strong>Outline ▶</strong> — hover a track or constant in the Outline and click
    its play button to hear just that declaration. Its dependencies (other constants,
    imports) resolve automatically.</li>
</ul>
<p>
  A snippet plays on a separate preview bus, so it sounds cleanly whether or not the
  song is playing — and stopping it never stops the song.
</p>

<h2>Selection → arrangement</h2>
<p>
  Selecting a note, a variable, a function, a cycle (<code>&lt;…&gt;</code> / <code>cat</code> /
  <code>every</code>), or a whole block in the editor outlines the matching regions on the
  arrangement timeline, so you can see exactly which part of the song a piece of code
  produces. Selecting a variable or function name resolves to its definition, so every
  note it generates lights up — even where it's used through transforms. Clear the
  selection to remove the outlines.
</p>

<div class="callout">
  Resting the caret on a name (no selection) still tints the lanes that reference it —
  the selection highlight is the finer-grained, per-region version.
</div>
