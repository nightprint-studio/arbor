<!-- Bennu docs — the files that are not source: images, documents, data formats. -->
<h1>Other file types</h1>
<p class="doc-lead">
  A project is not only code. These open in the editor as themselves rather than as bytes —
  and each one is a viewer, not an editor: Bennu shows them, it does not claim to author them.
</p>

<h2>Images</h2>
<p>
  A <code>.png</code>, <code>.jpg</code>, <code>.gif</code>, <code>.bmp</code>, <code>.webp</code>,
  <code>.ico</code>, <code>.avif</code>, <code>.tiff</code> or <code>.svg</code> opens as a
  <strong>preview</strong> in a tab of its own, beside your source files — which is what makes
  checking the asset a <code>.ron</code> or a stylesheet names a click rather than a trip out of the
  window.
</p>
<ul>
  <li><strong>Fit</strong> (the default, <kbd>F</kbd>) and <strong>actual size</strong>
    (<kbd>0</kbd>). Fit never magnifies: a 16 × 16 icon blown up to fill the panel is a worse answer
    to "what is this" than a 16 × 16 icon.</li>
  <li><kbd>+</kbd> / <kbd>−</kbd> or <kbd>Ctrl</kbd> + scroll steps through a zoom ladder. Above
    1:1 the image is drawn with hard pixel edges rather than smoothed, because zooming into an icon
    is how you count its pixels.</li>
  <li>The status line gives the format, the pixel dimensions and the file size — the last two being
    different questions, and both worth answering about an asset.</li>
  <li>The chequerboard behind the image is there so a transparent background is distinguishable
    from a black one.</li>
</ul>
<p>
  An image tab has no buffer, so it cannot be edited and cannot be saved — <kbd>Ctrl</kbd> +
  <kbd>S</kbd> on one does nothing rather than writing an empty file over your artwork. Formats a
  browser cannot decode (<code>.psd</code>, <code>.xcf</code>, <code>.svgz</code>) are still
  declined, because a preview showing a broken-image glyph is worse than a clear refusal.
</p>
<h2>Word documents</h2>
<p>
  A <code>.docx</code> opens as a <strong>rendered page</strong> in a tab — pages, styles, tables and
  images as Word laid them out, not a converted approximation of them. You open one from a project
  tree to check <em>the document</em>: a spec, a hand-off, a table somebody sent, and a version with
  the layout thrown away cannot be checked against the one the sender is looking at.
</p>
<p>
  Read-only, and its bar says so with a badge rather than a footnote — it is the fact that decides
  what you can do there. The same bar has a button to open the file in whatever application owns it,
  and it is the <em>only</em> bar: a viewer has nothing for the editor's own toolbar to act on, so
  that one is not drawn above a preview. The document never becomes a buffer, so there is nothing a stray
  <kbd>Ctrl</kbd>+<kbd>S</kbd> could write back over it. The old binary <code>.doc</code> format is
  not supported — Bennu says it cannot open one rather than rendering it wrong.
</p>
<h2>Fonts</h2>
<p>
  A <code>.ttf</code>, <code>.otf</code>, <code>.woff</code> or <code>.woff2</code> opens as a
  <strong>specimen</strong>: a field you type into with size, weight, tracking and italic beside
  it, a waterfall of the same line at eight sizes, and a coverage column. Every question you have
  about a font in a project tree is visual — what it looks like, whether it has the accents this
  project needs, how it holds up at eleven pixels — and the alternative was Bennu refusing to open
  it as a binary and sending you to a system previewer for a file you were already looking at.
</p>
<p>
  Coverage is <strong>measured</strong> rather than read out of the font's tables: a code point
  counts when it draws as something other than the replacement box, which is the same judgement
  your eye makes. That is why it reports blocks rather than glyph names, and why it works on any
  font the browser can load with nothing installed. <code>.eot</code> is not among them — no
  engine loads it any more, so Bennu says it cannot open one rather than opening it and failing.
</p>
<h2><code>.ron</code> files</h2>
<p>
  RON gets a mode of its own rather than Rust's. It borrows Rust's syntax and none of its
  vocabulary, so a field perfectly reasonably called <code>type:</code>, <code>mod:</code> or
  <code>ref:</code> used to come out coloured as a keyword — while the thing a RON file is mostly
  made of, the <strong>field names</strong>, had no colour at all. Now the left-hand column reads as
  the left-hand column, struct and variant names are told apart from plain values, and
  <code>#![enable(…)]</code> reads as the header it is. A <strong>constructor</strong>
  (<code>Sequence(</code>, <code>Single(</code>) and a bare <strong>unit variant</strong>
  (<code>blend: Additive</code>) both read as the type names they are, which in an asset file
  written by a generator is most of the words on the page.
</p>
<h2><code>package.json</code></h2>
<p>
  Recognised by <strong>name</strong>, so a <code>tsconfig.json</code> is still ordinary JSON and a
  manifest under <code>node_modules</code> is left alone. On top of the JSON colouring it gets the
  two things the syntax cannot say.
</p>
<p>
  <strong>The sections read as headings</strong> — <code>scripts</code>, the four dependency
  sections, <code>engines</code>, <code>exports</code> — so a long manifest has landmarks instead of
  four hundred identical strings. A script's command is coloured as what it is: the one string in
  the file that is code.
</p>
<p>
  <strong>A version says how pinned it is.</strong> Three colours and no more, because this is a
  glance: a range that <em>floats</em> on install (<code>^5.0.0</code>, <code>~2.1</code>, a
  comparator), one that is <em>pinned</em> (<code>5.0.0</code>), and one that does not come from the
  registry at all (<code>workspace:*</code>, <code>file:../lib</code>, a git URL). "What will
  <code>install</code> actually change" becomes something you can see.
</p>
<p>
  <strong>Run a script from the line that declares it.</strong> Each <code>scripts</code> entry
  carries a ▶ control naming the command it will run — <code>pnpm dev</code>, not a generic
  <em>Run</em>: which package manager a repository uses is read off the lockfile beside the manifest
  (<code>bun.lockb</code>, <code>pnpm-lock.yaml</code>, <code>yarn.lock</code>, else npm), and a
  control that says what it will type is one you can trust without checking. Output goes to the same
  Run console a <code>cargo run</code> uses, and a script in a workspace member runs in that
  member's directory.
</p>
<p>
  <strong>A dependency that is behind</strong> gets the same <em>↑ 6.0.0 available</em> offer a
  <code>Cargo.toml</code> gets; pressing it writes the new version in place. It appears only where
  the answer is unambiguous — <code>^</code>, <code>~</code> and an exact version — and stays silent
  on a comparator range, an alternation, a dist-tag, and every <code>workspace:</code> /
  <code>file:</code> / git dependency. A wrong "update available" on something pinned on purpose is
  worse than a missing one. The lookups share the <em>Look crates up online</em> setting and its
  cache: turning that off makes Bennu local again for both registries.
</p>
<h2>merula <code>.merula</code> patterns</h2>
<p>
  A <code>.merula</code> file is a piece of music for <strong>Merula</strong>. Bennu highlights it
  with <em>the same grammar Merula's own editor uses</em> — not a second, simpler one — so a file
  looks the same in both windows, mini-notation included: notes and chords share the pitch colour,
  sound names the other, the island brackets and a <code>$splice</code> are picked out as the seams
  where a pattern meets host code, and <code>~</code> / <code>_</code> stay muted so a dense pattern
  reads as its sounds rather than as its rests.
</p>
<p>
  <strong>Folding</strong> collapses a call's arguments — which is how a whole track inside
  <code>tracks(…)</code> folds away — a <code>meta &#123; … &#125;</code> front-matter block, and a block
  comment. <kbd>Ctrl</kbd> + <kbd>/</kbd> toggles a <code>//</code> comment.
</p>
<p>
  Completion, hover and go-to are Merula's window, not this one: they are answered from the DSL
  catalogue the audio backend serves, and Bennu does not start it. Open the file in Merula for
  those; edit it here alongside the rest of the project.
</p>
<h2>geode <code>.dig</code> scripts</h2>
<p>
  A <code>.dig</code> file is a mole program for <strong>geode</strong> — indentation-delimited, with
  a fixed set of host builtins. Bennu parses it with geode's own grammar, so highlighting and
  <strong>folding</strong> (a <code>fn</code>, <code>if</code>, <code>while</code>, <code>for</code>,
  <code>match</code> or <code>struct</code> body, and multi-line lists and maps) work from the syntax
  tree, and <kbd>Ctrl</kbd> + <kbd>/</kbd> toggles a <code>#</code> comment.
</p>
<p>
  Because the vocabulary is <strong>closed</strong>, completion and hover are answered locally — no
  index, no waiting. <strong>Completion</strong> offers the builtins, the reserved words, the
  namespaces, and the <code>fn</code> / <code>struct</code> / <code>let</code> names declared in the
  file; after <code>Crystal.</code> / <code>Tool.</code> / <code>Tick.</code> / <code>Speed.</code> /
  <code>Item.</code> it offers <strong>that namespace's members and nothing else</strong>. After a
  dot on anything else it offers the <strong>collection methods</strong> — both list and map, each
  labelled with its receiver, since without type inference both are true and picking one would be a
  guess. An <code>import</code> line is not completed: a geode module is a library unlocked in the
  shop, not a file on disk.
</p>
<p>
  <strong>Hover</strong> shows the signature and the full explanation, including the examples —
  the same help text the game shows, so <code>ripe_left()</code> explains the enormous-number answer
  and <code>block_size()</code> warns that a value above 1 also means a wall. A qualified member is
  looked up <em>with</em> its namespace, so <code>Speed.MAX_VALUE</code> and
  <code>Tick.MAX_VALUE</code> never show each other's text. Over a name the language doesn't own,
  hover stays silent.
</p>
