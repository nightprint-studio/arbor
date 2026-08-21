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
  braces only the tail is offered, since the package is already written. Once the path names a
  module, what completes next are <em>the names inside it</em>: type
  <code>bevy_pbr::forward_io::</code> and the list is that module's structs and functions.
</p>

<h2>What an import brings with it</h2>
<p>
  A Bevy shader is a fragment of something larger, and most of the names in it are declared
  somewhere else — <code>VertexOutput</code>, <code>globals</code>,
  <code>apply_pbr_lighting</code>. Bennu reads those modules: your project's own
  <code>.wgsl</code>, and the shader sources of the <code>bevy_*</code> crates your project
  resolved, taken from the versions in <code>Cargo.lock</code>. Everything an
  <code>#import</code> line names then behaves like a declaration you wrote:
</p>
<ul>
  <li><strong>Completion</strong> offers it, with the module it came from beside the name.</li>
  <li><strong>Hover</strong> shows its real signature and the comment above it.</li>
  <li><strong>Go-to</strong> opens the file that declares it, on the declaration.</li>
</ul>
<p>
  Importing a <code>struct</code> brings its <em>fields</em> too — writing <code>mesh.uv</code> is
  the reason <code>VertexOutput</code> gets imported in the first place. And a name your file
  declares itself always wins over an imported one of the same name: the local declaration is what
  runs, so it is what gets described.
</p>
<p>
  A name only resolves if the file actually imports it. One that resolves merely because some
  other shader in the project imports it is not in scope here, and jumping to it would answer a
  question this file did not ask. If the <code>bevy_*</code> sources are not on the machine — a
  project cargo has never built, or a git or path dependency — the module list falls back to a
  built-in catalogue of the common paths, and imported names simply do not resolve.
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

<h2>Naming the lanes of a <code>vec4</code></h2>
<p>
  A Bevy material extension binds <code>vec4&lt;f32&gt;</code> and packs four unrelated things
  into it — a frequency, two amounts, another frequency. WGSL gives the four of them one name,
  so anything driving the material has <code>X Y Z W</code> and a range it guessed.
</p>
<p>
  A comment fixes it, and only a comment can: <code>naga</code> fails with
  <em>unknown attribute</em> on anything outside the spec's list, so a custom
  <code>@range(…)</code> would stop the shader compiling — in the game as well as in the
  preview. Write the lanes on the lines above the declaration:
</p>
<pre><code>{`// @preview grain_freq 0.2..8 = 1.6
// @preview albedo_splotch 0..1 = 0.45
// @preview bump 0..1 = 0.65
// @preview band_freq 0..2 = 0.22
@group(#{'{'}MATERIAL_BIND_GROUP{'}'}) @binding(100)
var<uniform> rock_params: vec4<f32>;`}</code></pre>
<p>
  The form is <code>// @preview &lt;label&gt; [&lt;min&gt;..&lt;max&gt;] [= &lt;default&gt;] [: &lt;hint&gt;]</code>,
  one line per lane in declaration order. Everything after the label is optional — naming a
  lane without bounding it is the common case. On a struct member the same line works, where
  the label is a nicer name for one that already has one.
</p>
<p>
  The lines are highlighted apart from ordinary comments, because they do something: a
  load-bearing comment that looks like prose is one somebody deletes while tidying. Attribute
  lines between the comment and the declaration are stepped over; a blank line detaches it.
</p>
<p>
  Nothing needs them. A shader without any is read exactly as it was, and the preview goes on
  guessing from the variable's name.
</p>
<p>
  A <strong>colour</strong> is declared with a hex default:
  <code>// @preview hot = #ff6b14 : the centre of a crack</code>. That says two things the name
  cannot. Whether a <code>vec4</code> is a colour is otherwise guessed from the variable's
  name — which works for <code>sand_color</code> and fails for <code>hot</code>,
  <code>deep</code> and <code>foam</code>, all of which are colours and none of which say so —
  and even a correct guess opens on a palette entry rather than on the colour you chose.
</p>
<p>
  Above a <strong>texture</strong> the same line names <em>what it is</em>:
  <code>// @preview normal</code>, or <code>diffuse</code>, <code>pbr</code>, <code>ao</code>,
  <code>height</code> — any word you like. Without one it is guessed from the variable's name,
  which in every material anybody writes already says so.
</p>
<p>
  The key decides two things. It picks the picture the preview generates — a
  <code>normal</code> opens on a flat normal, an <code>ao</code> on white, a
  <code>diffuse</code> on a chequer — and it decides what <strong>shares a slot</strong>.
  <code>top_normal</code> and <code>side_normal</code> are the same kind of thing, and a
  preview has no assets, so both would be handed byte-identical images: one slot, nothing lost.
  Write <code>normal.top</code> and <code>normal.side</code> to have them apart.
</p>

<h2>Materials with textures</h2>
<p>
  A preview cannot ask for a bind-group layout that matches whatever binding indices a shader
  uses: <code>AsBindGroup</code> decides a material's layout when <em>it</em> is compiled, so
  one material type has one layout. Widening it covers a binding that is missing and a binding
  that is too small; it cannot cover a binding of the wrong <em>kind</em>, because index 101 is
  a buffer in one material and a sampler in the next.
</p>
<p>
  So the shader is renumbered instead. Bennu rewrites a copy — only the numbers inside
  <code>@binding(…)</code> — onto slots the viewer already has: eight uniform blocks, twelve
  2D textures, three samplers, and a pair each of array and cube textures. Names, offsets and
  <code>// @preview</code> lines are untouched, and nothing is written back to the file. A
  material with ten textures and a shared sampler therefore renders, with the images the
  preview generates in place of the game's atlas; the panel lists one row per <em>kind</em>, so
  a material sampling ten textures of four kinds is four decisions rather than ten.
</p>
<p>
  How many slots there are depends on where you are looking, and it is arithmetic rather than a
  preference. wgpu's GL backend hands a texture unit to <em>every</em> layout entry across every
  bind group, used or not, and WebGL2 has sixteen: the view and mesh groups spend seven, and a
  material extending <code>StandardMaterial</code> spends six more. So the panel's viewport has
  two slots for an extension and four for a material that owns its bind group, while the
  headless renderer behind <code>bennu_shader_render</code> has twelve for both. A material with
  more kinds than that says so on the row that lost out, rather than the viewport dying.
</p>
<p>
  The consequence worth knowing: because the layout is static, a shader that samples
  <em>no</em> textures is charged for the slots its material type declares anyway. That is why
  the numbers are kept tight — the cost lands on every shader, not only the ones with atlases.
</p>
<p>
  What no renumbering reaches is named rather than attempted: a storage buffer, a storage
  texture, a comparison sampler, a depth texture — things a pass fills, and the preview runs no
  pass — and anything past the slot counts. The panel says which binding and why, instead of
  the viewport dying on pipeline validation.
</p>
<p>
  Meshes carry a <strong>second UV channel</strong> and <strong>tangents</strong> too, so a
  material that branches on <code>VERTEX_UVS_B</code> or <code>VERTEX_TANGENTS</code> runs the
  branch it runs in the game. The values are derived from the geometry — the second channel
  reads as depth, 1 in the middle of the shape and 0 at its rim — which is a guess in the right
  range rather than the baked data, but it exercises the real path instead of the
  <code>#else</code>.
</p>

<h2>What it will not claim</h2>
<p>
  A project that ships no shaders <em>of a given form</em> is told nothing about missing ones. An
  engine crate declares the materials and the game that depends on it ships the
  <code>assets/</code> — so an unresolved file path there belongs to somebody else's project, and
  a warning would be permanently wrong. Likewise a shader <code>struct</code> that arrives through
  an <code>#import</code>: Bennu can say where it is declared and show you its fields, but it does
  not check a Rust layout against it — that comparison needs the composed module, with its
  <code>#ifdef</code> branches resolved and its <code>#&#123;SHADER_DEF&#125;</code> values
  substituted, and reading a file is not composing one.
</p>
