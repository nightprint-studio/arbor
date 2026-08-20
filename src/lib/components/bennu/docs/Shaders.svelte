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
