<!-- Bennu docs — WGSL shaders: highlighting, intelligence, and the naga_oil boundary. -->
<h1>Shaders (WGSL)</h1>
<p class="doc-lead">
  <code>.wgsl</code> files get their own highlighting, completion, go-to, find-usages, hover and
  compiler diagnostics — from Bennu itself, with no language server involved. The one thing it
  will not pretend to do is resolve a Bevy composition, and it says so rather than guessing.
</p>

<h2>Shaders</h2>
<p>
  A <code>.wgsl</code> file gets highlighting built for WGSL rather than borrowed from a
  C-family language: the <code>@group</code> / <code>@binding</code> / <code>@vertex</code>
  attributes read as the interface they are, <code>vec4&lt;f32&gt;</code> and
  <code>texture_2d&lt;f32&gt;</code> as types, and the standard library
  (<code>textureSample</code>, <code>mix</code>, <code>dot</code>) apart from the functions you wrote.
</p>
<p>
  <strong>Go to declaration</strong>, <strong>find usages</strong> and <strong>hover</strong> work on
  a shader with nothing installed, within the file. A hover on something you declared shows its
  signature and the <code>//</code> block above it: WGSL has no doc-comment syntax, so those lines
  <em>are</em> the documentation. A hover on a built-in says what the language says it is, and one on
  an attribute answers about the attribute — <code>@fragment</code> and the <code>fn fragment</code>
  under it are different things, and the <code>@</code> is what tells them apart.
</p>
<p>
  <strong>Imports complete.</strong> On an <code>#import</code> line — both the plain form and the
  braced <code>bevy_pbr::&#123;…&#125;</code> one, across the lines it spans — Bennu offers the modules
  your own project declares with <code>#define_import_path</code> first, then Bevy's own. Inside the
  braces only the tail is offered, since the package is already written.
</p>
<p>
  <strong>It compiles the shader.</strong> With no language server installed, Bennu validates a
  <code>.wgsl</code> through <strong>naga</strong> — the same front end wgpu and Bevy compile with —
  so a squiggle here is an error the shader would really have hit at pipeline creation, types and
  binding rules included, not an approximation of the grammar. Completion offers what the file
  declares first (its bindings, structs and functions) and then the language's own vocabulary, and
  find-usages resolves whole words within the file.
</p>
<p>
  A shader composed with <strong>naga_oil</strong> — anything with an <code>#import</code> or an
  <code>#ifdef</code>, which is most of a Bevy project's — is deliberately <em>not</em> compiled on
  its own: half its identifiers are declared in the module it imports, so checking it alone would
  report a hundred problems on a shader that is correct. It keeps its highlighting, its completion
  and its find-usages, and gets no compiler errors.
</p>
<p>
  <strong>A language server does not currently fix that.</strong> <code>wgsl-analyzer</code> is in the
  catalogue and can be installed with one button, but its module system is <strong>WESL</strong>
  (<code>import foo;</code>) rather than naga_oil's <code>#import</code> — its parser has no
  preprocessor directives at all. Installing it takes the file over, so on a Bevy shader it replaces
  the silence above with its own opinion of those lines. On a <em>standalone</em> shader — a compute
  pass, a material with no imports — it adds what an IDE engine adds and naga alone cannot: cross-file
  navigation and rename.
</p>
<p>
  Until you install it, nothing changes: a catalogue entry whose binary is not on this machine does
  not take the language over, so everything on this page is what a <code>.wgsl</code> file gets by
  default. See <em>Installing a language server</em> for the rule.
</p>

<h2>The material on the other side</h2>
<p>
  A shader is half of something. The other half is a Rust <code>#[derive(AsBindGroup)]</code>
  struct that says what the pipeline supplies and an <code>impl Material</code> that names the
  file — and <strong>nothing in the toolchain checks that the two agree</strong>. Write
  <code>f32</code> where the shader says <code>vec4&lt;f32&gt;</code>, or put two fields in the
  other order, and it all still compiles: the uniform is simply read with a different layout than
  it was written with, and what you get is a colour that is not the colour you asked for.
</p>
<p>Bennu reads both files and says so.</p>
<ul>
  <li><strong>The layout, field by field.</strong> Names, order and types, with
    <code>Vec4</code> ↔ <code>vec4&lt;f32&gt;</code>, <code>Mat4</code> ↔
    <code>mat4x4&lt;f32&gt;</code> and the rest of the scale. A type it does not recognise on
    either side ends the comparison for that field rather than guessing.</li>
  <li><strong>The bindings.</strong> Every <code>#[uniform(0)]</code>, <code>#[texture(1)]</code>
    and <code>#[sampler(2)]</code> has to exist in the shader's material bind group. One that is
    declared in <em>another</em> group is told apart from one that is absent — the two send you
    looking in different places.</li>
  <li><strong>The entry point.</strong> A material naming a shader for the fragment stage wants a
    <code>@fragment</code> in it.</li>
  <li><strong>The file.</strong> A path that resolves to no asset at all.</li>
</ul>

<h2>Getting from one to the other</h2>
<p>
  The seam has three joins, and <kbd>Ctrl</kbd> + <kbd>B</kbd> follows whichever the caret is on.
  Each works <strong>both ways</strong>: a declaration split over two files has no primary half,
  and you are as likely to be reading the shader and wanting the Rust as the other way round.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Caret on</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td>the path in <code>fragment_shader()</code></td><td>the <code>.wgsl</code></td></tr>
    <tr><td>a <code>#[uniform(0)]</code> field</td><td>the shader's <code>@binding(0)</code></td></tr>
    <tr><td>a <code>ShaderType</code> struct or field</td><td>the shader's <code>struct</code>, or that member</td></tr>
    <tr><td>a shader <code>struct</code> or member</td><td>the Rust layout, or that field</td></tr>
    <tr><td>a shader binding variable</td><td>the Rust field that supplies it</td></tr>
    <tr><td>anywhere else in a <code>.wgsl</code></td><td>the materials that run it</td></tr>
  </tbody>
</table>
<ul>
  <li>The shader path works when it is written as a <code>const</code> too, which is how a crate
    that embeds its shaders writes them.</li>
  <li>The last row is a fallback rather than an answer: a shader has no single declaration to
    jump to, and more than one material may run it.</li>
  <li>A mark in the gutter beside every material, naming its shaders.</li>
  <li>The <strong>Shaders</strong> panel — in the Command Palette — is the list keyed the other
    way round: one row per shader, the materials that run it underneath, and whatever the two
    disagree about.</li>
</ul>

<div class="callout">
  Two forms of asset path are understood: <code>shaders/x.wgsl</code>, relative to the project's
  <code>assets/</code> directory, and <code>embedded://crate_name/shaders/x.wgsl</code>, which is
  how a library crate ships shaders under its own <code>src/</code>.
</div>

<h2>What it will not claim</h2>
<p>
  A project that ships no shaders <em>of a given form</em> is told nothing about missing ones. An
  engine crate declares the materials and the game that depends on it ships the
  <code>assets/</code> — so an unresolved file path there belongs to somebody else's project, and
  a warning would be permanently wrong. Likewise a shader <code>struct</code> that arrives through
  an <code>#import</code> rather than being declared in the file: this side does not resolve
  naga_oil's composition, so it says nothing instead of comparing against something it cannot see.
</p>
