<script lang="ts">
  /**
   * Shaders (WGSL): highlighting and intelligence without a language server, imported modules, compiling through naga and
   * the naga_oil boundary, checking a material against its shader, navigating between them, naming vec4 lanes, and how the
   * preview handles textures.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { highlightCode } from '$lib/utils/highlight';
</script>

<span class="eyebrow">Rust</span>
<h1>Shaders (WGSL)</h1>

<p class="doc-lead">
  <code>.wgsl</code> files get their own highlighting, completion, go-to, find usages, hover and compiler diagnostics — from Bennu itself, with no language server. The one thing
  it will not pretend to do is resolve a Bevy composition, and it says so rather than guessing.
</p>

<h2>The file on its own</h2>
<p>
  Highlighting is built for WGSL rather than borrowed from a C-family language: the <code>@group</code>, <code>@binding</code> and <code>@vertex</code> attributes read as the
  interface they are, <code>vec4&lt;f32&gt;</code> and <code>texture_2d&lt;f32&gt;</code> as types, and the standard library — <code>textureSample</code>, <code>mix</code>,
  <code>dot</code> — apart from your functions.
</p>
<dl class="meta-grid">
  <dt>Go-to, find usages, hover</dt>
  <dd>Within the file, with nothing installed. A hover on your declaration shows its signature and the <code>//</code> block above it — WGSL has no doc comments, so those lines <em>are</em> the documentation. On a built-in it says what the language says; on an attribute it answers about the attribute — <code>@fragment</code> and the <code>fn fragment</code> under it are different things.</dd>
  <dt>Imports complete</dt>
  <dd>On an <code>#import</code> line — plain or braced, <code>bevy_pbr::&#123;…&#125;</code>, across its lines — your project's <code>#define_import_path</code> modules come first, then Bevy's. Inside braces only the tail is offered. Once the path names a module, the names <em>inside it</em> complete: <code>bevy_pbr::forward_io::</code> lists its structs and functions.</dd>
</dl>

<h2>What an import brings with it</h2>
<p>
  A Bevy shader is a fragment of something larger, and most of its names — <code>VertexOutput</code>, <code>globals</code>, <code>apply_pbr_lighting</code> — are declared
  elsewhere. Bennu reads those modules: your own <code>.wgsl</code>, and the shader sources of the <code>bevy_*</code> crates the project resolved, at the versions in
  <code>Cargo.lock</code>. Everything an <code>#import</code> names then behaves like your own declaration:
</p>
<ul>
  <li><strong>Completion</strong> offers it, with its module beside the name;</li>
  <li><strong>hover</strong> shows its real signature and the comment above it;</li>
  <li><strong>go-to</strong> opens the file declaring it.</li>
</ul>
<p>
  Importing a <code>struct</code> brings its <em>fields</em> — <code>mesh.uv</code> is why <code>VertexOutput</code> gets imported. A name your file declares always wins over an
  imported one of the same name: the local declaration is what runs.
</p>
<Callout variant="info" title="Only what this file imports">
  A name resolving only because another shader imports it is not in scope here. Without the <code>bevy_*</code> sources on the machine — never built, or a git or path dependency —
  the module list falls back to a catalogue of the common paths, and imported names do not resolve.
</Callout>

<h2>It compiles the shader</h2>
<p>
  With no language server installed, Bennu validates a <code>.wgsl</code> through <strong>naga</strong> — the front end wgpu and Bevy compile with — so a squiggle is an error the
  shader would hit at pipeline creation, types and binding rules included. Completion offers the file's bindings, structs and functions first, then the language's vocabulary.
</p>
<Callout variant="warning" title="A naga_oil composition is not compiled alone">
  A shader with an <code>#import</code> or an <code>#ifdef</code> — most of a Bevy project's — has half its identifiers in the modules it imports, so checking it alone would
  report a hundred problems on a correct shader. It keeps highlighting, completion and find usages, and gets no compiler errors.
</Callout>
<p>
  <strong>A language server does not fix that yet.</strong> <code>wgsl-analyzer</code> is in the catalogue and installs with one button, but its module system is
  <strong>WESL</strong> (<code>import foo;</code>), not naga_oil's <code>#import</code> — its parser has no preprocessor directives. Installed, it takes the file over, so on a
  Bevy shader it replaces the silence above with its own reading of those lines; on a <em>standalone</em> shader — a compute pass, a material with no imports — it adds
  cross-file navigation and rename. Until installed nothing changes: see <strong>Installing a language server</strong>.
</p>

<h2>The material on the other side</h2>
<p>
  A shader is half of something. The other half is a Rust <code>#[derive(AsBindGroup)]</code> struct saying what the pipeline supplies, and an <code>impl Material</code> naming the
  file — and <strong>nothing in the toolchain checks the two agree</strong>. Write <code>f32</code> where the shader says <code>vec4&lt;f32&gt;</code>, or swap two fields, and it
  all compiles: the uniform is read with a different layout than written, and the colour is not the one you asked for. Bennu reads both files:
</p>
<table>
  <thead><tr><th>Checked</th><th>How</th></tr></thead>
  <tbody>
    <tr><td><strong>The layout</strong></td><td>Field by field — names, order, types, with <code>Vec4</code> ↔ <code>vec4&lt;f32&gt;</code>, <code>Mat4</code> ↔ <code>mat4x4&lt;f32&gt;</code> and the rest. An unrecognised type on either side ends the comparison for that field.</td></tr>
    <tr><td><strong>The bindings</strong></td><td>Every <code>#[uniform(0)]</code>, <code>#[texture(1)]</code> and <code>#[sampler(2)]</code> must exist in the shader's material bind group — one in <em>another</em> group is told apart from one that is absent.</td></tr>
    <tr><td><strong>The entry point</strong></td><td>A material naming a fragment shader wants a <code>@fragment</code> in it.</td></tr>
    <tr><td><strong>The file</strong></td><td>A path resolving to no asset.</td></tr>
  </tbody>
</table>

<h2>Getting from one to the other</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>B</kbd> follows whichever join the caret is on, <strong>both ways</strong> — a declaration split over two files has no primary half:
</p>
<table>
  <thead><tr><th>The caret on</th><th>Goes to</th></tr></thead>
  <tbody>
    <tr><td>The path in <code>fragment_shader()</code> — a <code>const</code> too</td><td>The <code>.wgsl</code></td></tr>
    <tr><td>A <code>#[uniform(0)]</code> field</td><td>The shader's <code>@binding(0)</code></td></tr>
    <tr><td>A <code>ShaderType</code> struct or field</td><td>The shader's <code>struct</code>, or that member</td></tr>
    <tr><td>A shader <code>struct</code> or member</td><td>The Rust layout, or that field</td></tr>
    <tr><td>A shader binding variable</td><td>The Rust field supplying it</td></tr>
    <tr><td>Anywhere else in a <code>.wgsl</code></td><td>The materials that run it — a fallback, since several may</td></tr>
  </tbody>
</table>
<p>
  A gutter mark beside every material names its shaders, and the <strong>Shaders</strong> panel in the palette is the list the other way round: one row per shader, the materials
  running it underneath, and whatever the two disagree about.
</p>
<Callout variant="tip" title="Two forms of asset path">
  <code>shaders/x.wgsl</code>, relative to the project's <code>assets/</code>, and <code>embedded://crate_name/shaders/x.wgsl</code>, how a library crate ships shaders under its own
  <code>src/</code>.
</Callout>

<h2>Naming the lanes of a <code>vec4</code></h2>
<p>
  A Bevy material extension binds a <code>vec4&lt;f32&gt;</code> and packs four unrelated things into it — a frequency, two amounts, another frequency. WGSL gives them one name,
  so whatever drives the material has <code>X Y Z W</code> and a guessed range. A comment fixes it:
</p>
<pre><code>{@html highlightCode(`// @preview grain_freq 0.2..8 = 1.6
// @preview albedo_splotch 0..1 = 0.45
// @preview bump 0..1 = 0.65
// @preview band_freq 0..2 = 0.22
@group(#{MATERIAL_BIND_GROUP}) @binding(100)
var<uniform> rock_params: vec4<f32>;`, 'wgsl')}</code></pre>
<Callout variant="info" title="Why a comment and not an attribute">
  <code>naga</code> fails with <em>unknown attribute</em> on anything outside the spec, so a custom <code>@range(…)</code> would stop the shader compiling — in the game as well as
  the preview.
</Callout>
<p>
  The form is <code>// @preview «label» [«min»..«max»] [= «default»] [: «hint»]</code>, one line per lane in declaration order, everything after the label optional. On a struct
  member it works the same, the label a nicer name for one that has one. The lines are highlighted apart from ordinary comments, because a load-bearing comment that looks like
  prose gets deleted while tidying. Attribute lines between comment and declaration are stepped over; a blank line detaches it. A shader without any is read as before.
</p>
<dl class="meta-grid">
  <dt>A colour</dt>
  <dd>A hex default — <code>// @preview hot = #ff6b14 : the centre of a crack</code>. Otherwise whether a <code>vec4</code> is a colour is guessed from its name, which works for <code>sand_color</code> and fails for <code>hot</code>, <code>deep</code> and <code>foam</code>; and even a right guess opens on a palette entry rather than your colour.</dd>
  <dt>A texture</dt>
  <dd>The line names <em>what it is</em>: <code>// @preview normal</code>, <code>diffuse</code>, <code>pbr</code>, <code>ao</code>, <code>height</code> — any word. It picks the picture the preview generates (a flat normal, white, a chequer) and decides what <strong>shares a slot</strong>: <code>top_normal</code> and <code>side_normal</code> would get byte-identical images, so one slot; write <code>normal.top</code> and <code>normal.side</code> to have them apart.</dd>
</dl>

<h2>Materials with textures</h2>
<p>
  A preview cannot ask for a bind-group layout matching whatever binding indices a shader uses: <code>AsBindGroup</code> fixes a material's layout when <em>it</em> is compiled.
  Widening covers a binding missing or too small, never one of the wrong <em>kind</em> — index 101 is a buffer in one material and a sampler in the next.
</p>
<p>
  So the shader is renumbered. Bennu rewrites a copy — only the numbers inside <code>@binding(…)</code> — onto slots the viewer has: eight uniform blocks, twelve 2D textures,
  three samplers, and a pair each of array and cube textures. Names, offsets and <code>// @preview</code> lines are untouched, and nothing is written back. A material with ten
  textures and a shared sampler renders with generated images in place of the atlas, and the panel lists one row per <em>kind</em>.
</p>
<Callout variant="info" title="How many slots depends on where you look">
  wgpu's GL backend gives a texture unit to <em>every</em> layout entry of every bind group, used or not, and WebGL2 has sixteen: the view and mesh groups spend seven, a material
  extending <code>StandardMaterial</code> six more. So the panel's viewport has two slots for an extension and four for a material owning its bind group, while the headless
  renderer behind <code>bennu_shader_render</code> has twelve for both. A material with more kinds says so on the row that lost out. And because the layout is static, a shader
  sampling <em>no</em> textures is charged for its type's slots anyway — which is why the counts stay tight.
</Callout>
<p>
  What no renumbering reaches is named, not attempted: a storage buffer, a storage texture, a comparison sampler, a depth texture — things a pass fills, and the preview runs none —
  and anything past the slot counts.
</p>
<p>
  Meshes carry a <strong>second UV channel</strong> and <strong>tangents</strong>, so a material branching on <code>VERTEX_UVS_B</code> or <code>VERTEX_TANGENTS</code> runs the
  branch it runs in the game. The values are derived from the geometry — the second channel reads as depth, 1 in the middle and 0 at the rim — a guess in the right range that
  exercises the real path instead of the <code>#else</code>.
</p>

<h2>What it will not claim</h2>
<ul>
  <li>A project shipping no shaders <em>of a given form</em> is told nothing about missing ones: an engine crate declares materials and the game depending on it ships the
    <code>assets/</code>, so an unresolved path there belongs to another project.</li>
  <li>A shader <code>struct</code> arriving through an <code>#import</code> can be navigated and its fields shown, but a Rust layout is not checked against it: that needs the composed
    module, <code>#ifdef</code> branches resolved and <code>#&#123;SHADER_DEF&#125;</code> values substituted — and reading a file is not composing one.</li>
</ul>
