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
  Read-only, and it says so in its own bar, which also has a button to open the file in whatever
  application owns it. The document never becomes a buffer, so there is nothing a stray
  <kbd>Ctrl</kbd>+<kbd>S</kbd> could write back over it. The old binary <code>.doc</code> format is
  not supported — Bennu says it cannot open one rather than rendering it wrong.
</p>
<h2><code>.ron</code> files</h2>
<p>
  RON gets a mode of its own rather than Rust's. It borrows Rust's syntax and none of its
  vocabulary, so a field perfectly reasonably called <code>type:</code>, <code>mod:</code> or
  <code>ref:</code> used to come out coloured as a keyword — while the thing a RON file is mostly
  made of, the <strong>field names</strong>, had no colour at all. Now the left-hand column reads as
  the left-hand column, struct and variant names are told apart from plain values, and
  <code>#![enable(…)]</code> reads as the header it is.
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
