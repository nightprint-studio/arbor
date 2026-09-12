<script lang="ts">
  /**
   * Other file types: the files a project holds that are not source — pages, images, documents, spreadsheets, fonts — and
   * the languages Bennu colours and understands beyond Java: RON, package.json, merula, dig, Jinja.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Editor</span>
<h1>Other file types</h1>

<p class="doc-lead">
  A project is not only code. These open in the editor as themselves rather than as bytes — the documents as viewers, not editors: Bennu shows
  them, it does not claim to author them.
</p>

<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-eyebrow">Source, with a preview beside it</div>
    <div class="fc-title">HTML pages</div>
    <div class="fc-desc">The rendered page next to the buffer.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Read-only viewers</div>
    <div class="fc-title">Images, Word, spreadsheets, fonts</div>
    <div class="fc-desc">The file as itself, in a tab.</div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Coloured and understood</div>
    <div class="fc-title">RON, <code>package.json</code>, merula, dig, Jinja</div>
    <div class="fc-desc">Languages with their own modes.</div>
  </div>
</div>

<h2>HTML pages</h2>
<p>
  An <code>.html</code> opens as source, with a <strong>Preview</strong> button in the toolbar. It puts the rendered page <strong>beside</strong> the source, in a
  pane you can drag wider — a page is edited and looked at in the same breath, and a preview replacing the buffer would make every fix a round trip.
  <kbd>⤢</kbd> grows it to nearly the whole window and back.
</p>
<p>
  It shows the <em>buffer</em>, not the file on disk, so an edit and <kbd>⟳</kbd> is the loop, and the page's stylesheets and images load from beside it.
</p>
<Callout variant="info" title="Rendering asks nothing">
  The frame is sandboxed and has <em>no origin of its own</em>: it cannot read Arbor's window, its storage, or anything the application holds, whatever the page
  contains. A dialog in front of every preview would be one nobody reads.
</Callout>
<p>
  <strong>Its own scripts are the only question</strong> — that is what lets a page <em>act</em>: run its code and reach the network. The pill on the preview's bar
  says which of the two you are in, and <em>is</em> the switch:
</p>
<ol class="step-list">
  <li>Press the pill to allow scripts.</li>
  <li>Choose <strong>this once</strong>, or <strong>always for this file</strong> — a report opened every morning should not ask every morning.</li>
  <li>Press it again to block.</li>
</ol>
<p>The files remembered are kept in your profile, never inside the repository.</p>

<h2>Images</h2>
<p>
  <code>.png</code>, <code>.jpg</code>, <code>.gif</code>, <code>.bmp</code>, <code>.webp</code>, <code>.ico</code>, <code>.avif</code>, <code>.tiff</code> and
  <code>.svg</code> open as a <strong>preview</strong> in a tab of their own — so checking the asset a <code>.ron</code> or a stylesheet names is a click.
</p>
<table>
  <thead><tr><th>Control</th><th>What it does</th></tr></thead>
  <tbody>
    <tr><td><strong>Fit</strong> <kbd>F</kbd></td><td>The default. It never magnifies: a 16 × 16 icon blown up to fill the panel answers "what is this" worse than a 16 × 16 icon.</td></tr>
    <tr><td><strong>Actual size</strong> <kbd>0</kbd></td><td>1:1.</td></tr>
    <tr><td><kbd>+</kbd> / <kbd>−</kbd>, <kbd>Ctrl</kbd> + scroll</td><td>Steps through a zoom ladder. Above 1:1 pixels are drawn with hard edges — zooming into an icon is how you count its pixels.</td></tr>
  </tbody>
</table>
<p>
  The status line gives the format, the pixel dimensions and the file size. A chequerboard behind the image tells a transparent background from a black one.
</p>
<Callout variant="info" title="An image cannot be saved over">
  An image tab has no buffer, so <kbd>Ctrl</kbd> + <kbd>S</kbd> on one does nothing rather than writing an empty file over your artwork. Formats a browser cannot
  decode — <code>.psd</code>, <code>.xcf</code>, <code>.svgz</code> — are declined: a broken-image glyph is worse than a clear refusal.
</Callout>

<h2>Word documents</h2>
<p>
  A <code>.docx</code> opens as a <strong>rendered page</strong> — pages, styles, tables and images as Word laid them out, not a converted approximation. You open one
  from a project to check <em>the document</em> — a spec, a hand-off, a table somebody sent — and a version with the layout thrown away cannot be checked against the
  one the sender is looking at.
</p>
<p>
  It is read-only, and its bar says so with a badge. The same bar opens the file in whatever application owns it, and it is the <em>only</em> bar: a viewer has
  nothing for the editor's toolbar to act on. The document never becomes a buffer, so no stray <kbd>Ctrl</kbd> + <kbd>S</kbd> can write back over it. The old binary
  <code>.doc</code> is not supported — Bennu says so rather than rendering it wrong.
</p>

<h2>Spreadsheets</h2>
<p>
  A workbook opens as a <strong>grid</strong>: the values, the sheet tabs, the column letters and row numbers pinned, and numbers aligned right so one that does not
  belong in a column shows itself.
</p>
<table>
  <thead><tr><th>Format</th><th>Files</th></tr></thead>
  <tbody>
    <tr><td>Office Open XML</td><td><code>.xlsx</code>, <code>.xlsm</code></td></tr>
    <tr><td>Binary</td><td><code>.xlsb</code></td></tr>
    <tr><td>Excel 97-2003</td><td><code>.xls</code></td></tr>
    <tr><td>OpenDocument</td><td><code>.ods</code></td></tr>
  </tbody>
</table>
<dl class="meta-grid">
  <dt>The bytes decide, not the name</dt>
  <dd>A file saved as OOXML under an <code>.xls</code> name — every export tool has shipped one — opens as what it is, and the bar above the grid says which format that turned out to be.</dd>
  <dt>A grid, not a rendering</dt>
  <dd>The opposite call from Word, deliberately: nobody opens a spreadsheet from a source tree for its borders and fills, but for the column mapping an import expects, the codes, the translations.</dd>
  <dt>What the file records</dt>
  <dd>A formula shows its last computed value, saved with the workbook, never recalculated; a cell whose value was never stored is empty rather than guessed. A date is a number in a spreadsheet, shown as a date only where the cell's own format says so.</dd>
  <dt>Read-only</dt>
  <dd>Nothing for a stray <kbd>Ctrl</kbd> + <kbd>S</kbd> to write over. A long or wide sheet shows its beginning and says so; <em>Open externally</em> hands the file to the spreadsheet application.</dd>
</dl>

<h2>Fonts</h2>
<p>
  A <code>.ttf</code>, <code>.otf</code>, <code>.woff</code> or <code>.woff2</code> opens as a <strong>specimen</strong>: a field you type into, with size, weight,
  tracking and italic beside it; a waterfall of the line at eight sizes; and a coverage column. Every question about a font in a project is visual — how it looks,
  whether it has this project's accents, how it holds up at eleven pixels.
</p>
<Callout variant="info" title="Coverage is measured">
  A code point counts when it draws as something other than the replacement box — the judgement your eye makes — so it reports blocks rather than glyph names, and works
  on any font the browser can load. <code>.eot</code> is not among them: no engine loads it any more, so Bennu says it cannot open one.
</Callout>

<h2><code>.ron</code> files</h2>
<p>
  RON borrows Rust's syntax and none of its vocabulary, so it has a mode of its own. A field reasonably called <code>type:</code>, <code>mod:</code> or
  <code>ref:</code> is a field, not a keyword, and what a RON file is mostly made of — the <strong>field names</strong> — has its colour:
</p>
<ul>
  <li>the left-hand column reads as the left-hand column;</li>
  <li>struct and variant names are told apart from plain values — a <strong>constructor</strong> (<code>Sequence(</code>, <code>Single(</code>) and a bare
    <strong>unit variant</strong> (<code>blend: Additive</code>) both read as the type names they are;</li>
  <li><code>#![enable(…)]</code> reads as the header it is.</li>
</ul>

<h2><code>package.json</code></h2>
<p>
  Recognised by <strong>name</strong>, so a <code>tsconfig.json</code> stays ordinary JSON and a manifest under <code>node_modules</code> is left alone. On top of the
  JSON colouring it gets what the syntax cannot say:
</p>
<dl class="meta-grid">
  <dt>Sections as headings</dt>
  <dd><code>scripts</code>, the four dependency sections, <code>engines</code>, <code>exports</code> — landmarks instead of four hundred identical strings. A script's command, the one string that is code, is coloured as code.</dd>
  <dt>How pinned a version is</dt>
  <dd>Three colours and no more: a range that <em>floats</em> on install (<code>^5.0.0</code>, <code>~2.1</code>, a comparator), one <em>pinned</em> (<code>5.0.0</code>), and one not from the registry (<code>workspace:*</code>, <code>file:../lib</code>, a git URL).</dd>
  <dt>Run a script from its line</dt>
  <dd>Each <code>scripts</code> entry has a ▶ naming the command it runs — <code>pnpm dev</code>, not <em>Run</em>. The package manager is read off the lockfile (<code>bun.lockb</code>, <code>pnpm-lock.yaml</code>, <code>yarn.lock</code>, else npm). Output goes to the Run console, and a workspace member's script runs in its directory.</dd>
  <dt>A dependency that is behind</dt>
  <dd>An <em>↑ 6.0.0 available</em> offer, as in a <code>Cargo.toml</code>, writing the version in place — only for <code>^</code>, <code>~</code> and exact versions, never a comparator range, an alternation, a dist-tag or a <code>workspace:</code>, <code>file:</code> or git dependency. The lookups share the <em>Look crates up online</em> setting and its cache.</dd>
</dl>

<h2>merula <code>.merula</code> patterns</h2>
<p>
  A piece of music for <strong>Merula</strong>, highlighted with <em>the grammar Merula's own editor uses</em>, so a file looks the same in both windows, mini-notation
  included: notes and chords share the pitch colour, sound names the other, the island brackets and a <code>$splice</code> mark where a pattern meets host code, and
  <code>~</code> / <code>_</code> stay muted so a dense pattern reads as its sounds.
</p>
<p>
  <strong>Folding</strong> collapses a call's arguments — so a whole track inside <code>tracks(…)</code> folds away — a <code>meta &#123; … &#125;</code> block, and a
  block comment. <kbd>Ctrl</kbd> + <kbd>/</kbd> toggles a <code>//</code> comment.
</p>
<Callout variant="info" title="Completion and hover are Merula's">
  They come from the DSL catalogue the audio backend serves, which Bennu does not start. Open the file in Merula for those; edit it here beside the rest of the project.
</Callout>

<h2>geode <code>.dig</code> scripts</h2>
<p>
  A mole program for <strong>geode</strong> — indentation-delimited, with a fixed set of host builtins — parsed with geode's own grammar. Highlighting and
  <strong>folding</strong> — a <code>fn</code>, <code>if</code>, <code>while</code>, <code>for</code>, <code>match</code> or <code>struct</code> body, multi-line lists and
  maps — work from the tree, and <kbd>Ctrl</kbd> + <kbd>/</kbd> toggles a <code>#</code> comment.
</p>
<p>The vocabulary is <strong>closed</strong>, so completion and hover are answered locally, with no index and no waiting:</p>
<ul>
  <li><strong>Completion</strong> offers the builtins, reserved words, namespaces, and the <code>fn</code>, <code>struct</code> and <code>let</code> names of the file.
    After <code>Crystal.</code>, <code>Tool.</code>, <code>Tick.</code>, <code>Speed.</code> or <code>Item.</code>, <strong>that namespace's members and nothing
    else</strong>; after a dot on anything else, the <strong>collection methods</strong> of both lists and maps, each labelled — without type inference both are true.
    An <code>import</code> line is not completed: a module is a library unlocked in the shop, not a file.</li>
  <li><strong>Hover</strong> shows the signature and the full explanation with its examples — the game's own help text, so <code>ripe_left()</code> explains the enormous
    number and <code>block_size()</code> warns that above 1 also means a wall. A member is looked up <em>with</em> its namespace, so <code>Speed.MAX_VALUE</code> and
    <code>Tick.MAX_VALUE</code> never swap texts. Over a name the language does not own, hover is silent.</li>
</ul>

<h2>Jinja templates</h2>
<p>
  A <code>.jinja</code>, <code>.jinja2</code> or <code>.j2</code> file is coloured as the language it generates, with its tags on top: <code>OrderTest.java.jinja</code>
  is Java, <code>page.html.jinja</code> HTML. The name says which — Java, Kotlin, Rust, SQL, Go, Lua, XML, HTML and JSP, YAML, properties, TOML, JSON, CSS, Python,
  JavaScript, shell — and one whose name does not say is text with coloured tags. Each tag sits on a faint ground of its own, so the output and the logic writing it read as two layers, and the file wears the Jinja
  mark in the tree.
</p>
<ul>
  <li>Inside a tag, completion offers the statement names after <code>{'{%'}</code>, the filters after <code>|</code>, the tests after <code>is</code>, and the names the
    template binds with <code>set</code> and <code>for</code> — plus <code>loop</code> inside a loop. Hovering any of them says what it is.</li>
  <li>A code template also completes and explains the data its kind is rendered with, and the eye button previews what it renders beside the source — see
    <strong>Code templates</strong>.</li>
  <li><kbd>Ctrl</kbd> + <kbd>/</kbd> comments a line out with <code>{'{# … #}'}</code>.</li>
  <li>In Markdown, a fence marked <code>jinja</code> is coloured as a template, and one marked with the language it writes — <code>java.jinja</code>,
    <code>yml.jinja</code> — as a template that writes it.</li>
</ul>
