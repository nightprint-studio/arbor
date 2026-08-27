<!-- Bennu docs — language servers: what they add to the editor. -->
<h1>Language servers</h1>
<p class="doc-lead">
  Bennu's Java intelligence is its own engine. Every other language is served by an external
  <strong>language server</strong>, and the editor treats both the same way. This page is what a
  server gives you; getting one running is <em>Installing a language server</em>.
</p>

<h2>What you get</h2>
<p>
  For a file a server owns, the whole editing surface is live — the same gestures as in a Java
  file, answered by the server instead:
</p>
<ul>
  <li><strong>Completion</strong> as you type, and after the punctuation the language cares about
    (for Rust that is <code>.</code> and <code>::</code>). An item that needs an import brings the
    <code>use</code> line with it.</li>
  <li><strong>Go to declaration</strong> — <kbd>Ctrl</kbd> + <kbd>B</kbd> or
    <kbd>Ctrl</kbd>-click. On the declaration itself it flips to find-usages, as it does in Java.</li>
  <li><strong>Find usages</strong> — <kbd>Alt</kbd> + <kbd>F7</kbd>.</li>
  <li><strong>Hover</strong> — the signature, where the item lives, and its documentation.</li>
  <li><strong>Diagnostics</strong>, in the editor and in the Problems panel.</li>
  <li><strong>Rename</strong> — <kbd>Shift</kbd> + <kbd>F6</kbd>, with the same preview.</li>
  <li><strong>Quick fixes and refactorings</strong> under <kbd>Alt</kbd> + <kbd>Enter</kbd> — the
    server's, and on a file it owns they are the whole list: "import <code>HashMap</code>", "fill match
    arms", "inline macro". Fixes come first, then refactorings; an action the server offers but cannot
    apply here is shown last <em>with its reason</em> rather than hidden, because "selection crosses a
    block" tells you what to change and an absent row does not. Many of its refactorings want a
    <strong>selection</strong> — extract a variable or a function has to be told what — so at a bare
    caret the list is short by design.</li>
  <li><strong>Format</strong> — <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd>, with the
    language's own formatter (<code>rustfmt</code> for Rust) and the project's own configuration.</li>
  <li><strong>Semantic colouring</strong> — see below.</li>
  <li><strong>Signature help</strong> — the parameter list of the call the caret is inside.</li>
  <li><strong>Go to type / symbol</strong> — <kbd>Ctrl</kbd> + <kbd>N</kbd> and
    <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd>, searched across the whole workspace. See
    below.</li>
  <li><strong>Structure</strong> — the outline in the Structure panel and in the
    <kbd>Ctrl</kbd> + <kbd>F12</kbd> popup, in the language's own vocabulary: a Rust file lists its
    structs, traits, impls and functions, with the same icons a Java type and member get.</li>
  <li><strong>Occurrence highlighting</strong> — every other place the symbol under the caret appears
    in this file, tinted as you rest on it. A write is tinted differently from a read, because "where
    is this assigned" is a different question from "where is this used".</li>
  <li><strong>Folding</strong> — from the server, so it folds by <em>item</em>: a <code>use</code>
    block, a doc comment, a <code>#[cfg]</code>-gated module, a match arm. The usual fold keys and the
    gutter arrows work on it.</li>
  <li><strong>Expand / shrink selection</strong> — <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>→</kbd>
    and <kbd>←</kbd>, one syntactic step at a time.</li>
  <li><strong>Placeholders</strong> in an accepted completion — <kbd>Tab</kbd> between them; see
    below.</li>
  <li><strong>Code lenses</strong> — counts above an item, clickable; see below.</li>
  <li><strong>Call and type hierarchy</strong> — <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> and
    <kbd>Ctrl</kbd> + <kbd>H</kbd>. Not server-only: Java answers from its own index (see
    <em>Navigation</em>), and the panel is the same one. See below for what a server adds.</li>
</ul>
<h2>Go to type, go to symbol</h2>
<p>
  On a project with no Java index, <kbd>Ctrl</kbd> + <kbd>N</kbd> asks the server instead. The tab
  is called <strong>Types</strong> rather than Classes, because what it finds are structs, enums,
  traits, unions and type aliases — and <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>Y</kbd>
  (<strong>Symbols</strong>) finds everything else: functions, methods, constants, statics, fields.
  The same split as the Java pair, in the vocabulary of the language actually open.
</p>
<p>
  The matching is done by the <em>server</em>, not here, which is why it needs at least two
  characters: a workspace is far too large to hand over whole, and a one-character query is a great
  many rows that discriminate nothing.
</p>
<p>
  Each row is <strong>named and marked in the language's own vocabulary</strong>. The protocol has a
  fixed list of 26 symbol kinds and every language squeezes into it — rust-analyzer reports a
  <code>trait</code> as an interface, an <code>impl</code> block as an object, a type alias as a type
  parameter — so those are translated back before they are shown. In <strong>Types</strong>, where
  every row is a type and a shape would say nothing, the mark is the lettered ring a Java class wears:
  <strong>S</strong> struct, <strong>T</strong> trait, <strong>E</strong> enum, <strong>A</strong> type
  alias. In <strong>Symbols</strong> it is a shape instead, because there the distinction worth
  drawing is function against constant against field.
</p>
<p>
  Two are deliberately <em>not</em> translated: a <code>union</code> arrives as a struct and a
  <code>static</code> as a constant, and nothing downstream can tell them apart — so both keep the
  protocol's word rather than being guessed at.
</p>
<h2>Code lenses</h2>
<p>
  A line of text above an item, saying how many things there are of a kind and taking you to them:
  <strong>implementations</strong> of a trait, and <strong>references</strong> to a type or a trait.
  Pressing one shows the list — one result jumps straight there, several open the same popover
  <kbd>Alt</kbd> + <kbd>F7</kbd> fills.
</p>
<p>
  It costs nothing extra to press: the server has already found those places, because finding them is
  how it counted them, and it sends the list along with the count. Reference counts are asked for on
  types and traits only — each one is a reference query per item, and methods are by far the most
  numerous items in a file.
</p>
<h2>Call and type hierarchy</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> on a function opens its <strong>callers</strong>;
  <kbd>Ctrl</kbd> + <kbd>H</kbd> on a type opens its <strong>implementors</strong>. Both land in the
  Hierarchy panel at the bottom, which has a direction chip to walk the other way — callees, or what a
  type is built on. On a file the server owns the server answers; on a <code>.java</code> Bennu's own
  index does, into the same panel.
</p>
<p>
  The tree is expanded <strong>one level at a time</strong>, and that is not laziness for its own sake:
  expanding a call graph eagerly does not terminate. Mutual recursion, a trait implemented by a type
  that uses it, or simply a widely-called helper turns "expand everything" into a sweep of the
  workspace — so a recursive chain is something you walk into as far as you care to, and no further.
</p>
<p>
  A caller row jumps to the <em>call</em>, not to the head of the function containing it, and says
  <code>3×</code> when there is more than one call to the same thing inside it. The panel takes the
  keyboard as it opens: arrows walk and expand, <kbd>Enter</kbd> jumps.
</p>
<h2>Placeholders in a completion</h2>
<p>
  Accepting a completion that has holes in it puts the caret in the first one and
  <kbd>Tab</kbd> moves to the next — <code>println!</code> lands between the parentheses, a function
  lands on its first argument. <kbd>Shift</kbd> + <kbd>Tab</kbd> goes back, <kbd>Esc</kbd> leaves the
  run, and moving the caret anywhere else ends it.
</p>
<p>
  Two placeholders that the server marked as the <em>same</em> value are two stops you tab between
  rather than one that updates the other as you type. Nothing half-works: they behave as two ordinary
  stops.
</p>
<h2>Expanding a macro</h2>
<p>
  <kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>M</kbd> on a macro call shows what it generates, in a
  dialog you can read, copy and re-expand from. It is also offered under
  <kbd>Alt</kbd> + <kbd>Enter</kbd>, named after the macro it found — <em>Expand vec!</em> — beside the
  server's own quick fixes, and only when the caret is genuinely inside a macro call. Worth knowing
  what it is and is not:
</p>
<ul>
  <li>the expansion is <strong>recursive</strong> — all the way down. The server offers no single-step
    form, so neither does Bennu;</li>
  <li>it is <strong>text</strong>, not a file the server knows, so there is no go-to, no hover and no
    completion inside it. To expand a macro <em>within</em> the expansion, point at it in the real
    source and expand again;</li>
  <li><strong>Re-expand</strong> asks about the caret as it is now — move it in the file behind the
    dialog and press it.</li>
</ul>
<h2>Semantic colouring</h2>
<p>
  A file is coloured twice. The first pass is local and instant: it recognises keywords, strings,
  comments and numbers, and it is what you see the moment a tab opens. The second comes from the
  server and refines it — a struct told apart from a trait, a macro from a function, a
  <code>mut</code> binding from an immutable one, a parameter from a local.
</p>
<p>
  The layers are deliberate. The local pass means a file is never a wall of plain text while a
  request is in flight, or when the server is not running at all; the semantic pass means that
  when it <em>is</em> running, the colours are facts rather than guesses.
</p>
<h2>Diagnostics arrive on save</h2>
<p>
  For Rust the real errors — types, borrows, unreachable code — come from
  <code>cargo check</code>, which the server runs when a file is <strong>saved</strong>. So they
  appear a moment after a save rather than as you type, and what you see while typing is what the
  parser alone can tell.
</p>
<p>
  <strong>Settings → Language Servers → Rust</strong> chooses what that command is:
  <code>cargo check</code>, or <code>cargo clippy</code> — a superset, every check error plus several
  hundred lints, at the cost of a slower build after every save. It is an option the server reads when
  it starts, so it takes effect on the next start.
</p>
<p>
  Autosave counts as a save, so with it on (the default) the loop is: stop typing, wait a beat,
  see the compiler's answer.
</p>
