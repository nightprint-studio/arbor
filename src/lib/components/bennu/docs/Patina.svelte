<script lang="ts">
  /**
   * Patina (`.ptn`), the Voriz mod language: where its intelligence comes from (a `language-intel` extension), what a
   * request carries, and what happens without the package.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Rust</span>
<h1>Patina (Voriz mods)</h1>

<p class="doc-lead">
  <code>.ptn</code> files are mod scripts for Voriz, written in Patina. Bennu colours them itself; everything that needs to understand the
  language — completion, hover, go-to, find usages, rename, diagnostics — comes from a <strong>language extension</strong>, a wasm package
  that implements the <code>language-intel</code> interface.
</p>

<h2>What you get</h2>
<dl class="meta-grid">
  <dt>Highlighting</dt>
  <dd>Local and instant, from the shape of the text: calls and method calls, fields, associated functions (<code>BlockPos::new</code>, in
    italic like a static), variants and constants (<code>Mood::Calm</code>, <code>Some</code>, <code>POWER</code>), declarations, module
    paths, primitive types (<code>int</code>, <code>fx</code>, <code>f32</code>, <code>bool</code>, <code>str</code>) in a colour of their own, apart from keywords and other types, named arguments, <code>#[attributes]</code>, <code>block!(…)</code> forms, labels, numbers with their suffixes and durations.
    Brackets are coloured by depth and blocks fold by their braces. It never claims to know which names exist, so it cannot go out of
    date — telling a parameter from a local, or a <code>mut</code> binding, would need the backend. In Markdown, a fenced block tagged
    <code>ptn</code> or <code>patina</code> (any case, extra words after the tag allowed) is coloured the same way, in both the rendered and the source view.</dd>
  <dt>Comments as Markdown</dt>
  <dd>A run of whole-line <code>//</code>, <code>///</code> or <code>//!</code> comments with the same indentation reads as one Markdown document, drawn live as in the
    Markdown editor: headings, <strong>bold</strong> and <em>italic</em>, <code>inline code</code>, links, lists and task lists, quotes and
    callouts (<code>&gt; [!WARNING]</code>), fenced code blocks, Mermaid diagrams and tables. A paragraph can span several comment lines. The
    <code>//</code> shows only on the line the caret is on, and inline markers show while the caret is inside them. A table is edited cell by
    cell, as in the Markdown editor, and keeps its <code>//</code> when written back. A comment after code on the same line stays plain.</dd>
  <dt>Completion and hover</dt>
  <dd>After a dot, the fields and methods of the receiver's type, with its generics filled in; after <code>::</code>, variants and associated items;
    inside a call, the parameters by name. A name the file does not import yet is offered with the <code>use</code> line it needs. Hover shows the
    declaration as written, with the attributes that matter (<code>#[context(server)]</code>, <code>#[capability("world.write")]</code>), and the
    inferred type of a local.</dd>
  <dt>Go-to, find usages, rename</dt>
  <dd>Across the whole mod. A rename lists every edit with the text it expects to find, and refuses names from a library.</dd>
  <dt>Implementations, callers, structure</dt>
  <dd>Go to implementation from a trait (the types that implement it), a trait method (the methods of its <code>impl</code>s) or an
    <code>impl</code> method (the other <code>impl</code>s). <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd> on a function or method opens who calls
    it, grouped by the calling function with each call site, one level at a time (there is no type hierarchy). The Structure panel and the
    <kbd>Ctrl</kbd> + <kbd>F12</kbd> popup list the file's functions, structs, enums, traits and the methods of their <code>impl</code>s. Imber asks
    the same things (<code>bennu_implementors</code>, <code>bennu_callers</code>, <code>bennu_outline</code>). Who calls needs a version of the extension
    that has it; an older one answers with no callers.</dd>
  <dt>Diagnostics</dt>
  <dd>Syntax, names, types, contexts (<code>#[server]</code>, <code>#[client]</code>, <code>#[worldgen]</code>), the capabilities the mod's
    <code>manifest.ron</code> declares, exhaustive <code>match</code>, writes to what is not <code>mut</code>.</dd>
</dl>

<h2>What a request carries</h2>
<p>
  Every request carries the mod, so the answer never depends on what the extension remembers. The Patina package is
  <code>persistent</code>: Arbor keeps one instance of it alive, and it keeps the mod's index between questions, re-reading only the
  files whose text changed — about a millisecond per question instead of rebuilding the mod. A request carries the
  nearest folder above the file with a <code>manifest.ron</code> whose <code>script/</code> holds the file, that manifest, and every
  <code>.ptn</code> under <code>script/</code> as it is on disk — with the editor's buffer standing in for the file you are typing in. Without a
  manifest, the file's own folder is the mod.
</p>

<Callout variant="info" title="Other open buffers are read from disk">
  Only the file you are asking about travels with its unsaved text. A change in another open <code>.ptn</code> that is not saved yet is not seen by
  the question until you save it.
</Callout>

<h2>UI ids and translations</h2>
<p>
  Inside <code>ui!("barra", "matassa-1")</code> and <code>t!("ui:avvio.solo")</code> Patina checks the id or the key, offers the known ones as you type,
  and gives the UI nodes as typed constants (<code>ui::barra::MATASSA_1</code>). What it checks against is read by the package itself, in two layers:
</p>
<dl class="meta-grid">
  <dt>The base game</dt>
  <dd>The UI node catalog and the translations of the base Voriz project, <strong>embedded in the package</strong>: each version of the plugin knows the game it
    was built for, and does not need to find it on your disk.</dd>
  <dt>The open mod</dt>
  <dd>Its own documents (<code>presentazione/hud/**/*.ron</code>) and translations (<code>presentazione/i18n/«lang»/**/*.toml</code> and <code>.md</code>), read through
    Arbor's read-only <code>fs</code> interface. A mod's node scopes begin with the mod's <code>id</code> (<code>ui!("my_mod/hud", "kills")</code>). On a collision the
    <strong>mod wins</strong>. Only changed files are re-read.</dd>
</dl>

<Callout variant="info" title="Needs the read permission, inside the open repository">
  The package declares <code>fs = "read"</code>, so Arbor asks for it when you install or update it, and the mod has to be inside the repository open in Arbor. If the
  permission is declined — or the mod is somewhere else — Patina keeps working and checks <strong>no</strong> ids and no text keys, instead of reporting every
  one as unknown. The mods this one depends on are found the same way (see <em>Dependencies</em> below), so what they export is known too.
</Callout>

<h2>Dependencies</h2>
<p>
  The manifest says <em>what</em> the mod needs (<code>dependencies: [(id: "marmo")]</code>), not where it is. The package finds each one <strong>by id</strong>,
  with the same read-only <code>fs</code> interface: it looks in the folders <strong>next to the mod</strong> and in a <code>dlc/</code> or <code>plugins/</code> folder next to
  the project (and, for a mod that lives inside such a folder, one level up). A folder is a mod if it has a <code>manifest.ron</code>, and its id is the manifest's — the folder's name does not matter.
  It reads that mod's <code>script/**/*.ptn</code>, and its own dependencies in turn. The libraries built into the package (<code>voriz</code>, <code>patina</code>) are not searched for.
</p>
<p>
  The same lookup feeds the editor: with a dependency found, <code>use marmo::Gem</code> resolves, diagnostics see the dependency's <code>lib.ptn</code>, and go-to-definition lands in its file.
  The lookup happens when the session for the mod is built (opening the mod, changing its manifest), not on every keystroke, so a change to a dependency's files shows up the next time the session is rebuilt.
</p>

<Callout variant="info" title="A dependency outside the repository cannot be read">
  The host lets the package read only the repository open in Arbor and the paths in <code>fs_scope</code>. A dependency that lives outside is reported as
  <em>cannot be read by the plugin</em>, not as missing — open the folder that contains both, or add it to <code>fs_scope</code>.
</Callout>

<h2>Build</h2>
<p>
  <strong>Build</strong> runs the real Patina compiler on the whole mod and shows what it says. Nothing is written: a mod is compiled from its sources at every load,
  and there is no bytecode cache, so a build is a full compile whose result is the report. Dependencies are needed at run time and are not embedded.
</p>
<dl class="meta-grid">
  <dt>Where</dt>
  <dd>With a <code>.ptn</code> file active: <kbd>Ctrl+F9</kbd>, the title bar's Build button (it reads <em>Build Patina mod</em>), or <em>Build Patina mod</em> in the command palette. Opening the mod
    folder itself as the project makes the ordinary Build do the same. The output goes to the Build panel, whose problems are clickable and marked in the editor.</dd>
  <dt>What it reports</dt>
  <dd>The checker's diagnostics (errors and warnings) and the compiler's, each with file, line and column; the counts (files, functions, entries, schemas) and the time; the
    <code>#[state]</code>, <code>#[event]</code> and <code>#[intent]</code> schemas; the capabilities the mod really uses and the host functions it imports; the dependencies that were found.</dd>
  <dt>Full compile</dt>
  <dd>A construct the compiler cannot lower yet is an <strong>error</strong> named after the construct (<code>compile.unsupported</code>), because that is what the game would do with it.
    If the checker finds errors the compiler does not run (the game refuses a mod that does not check), and the report says so.</dd>
  <dt>Missing dependency</dt>
  <dd>A dependency that is not found is an error entry (<em>dependency `marmo` not found</em>) and the build stops there.</dd>
</dl>

<Callout variant="info" title="The build reads the files on disk">
  Like a question to the language extension, a build sees other files as they are saved. Save before building.
</Callout>

<Callout variant="warning" title="An older package has no build">
  The package has to export <code>build</code> (an optional function of <code>language-intel</code>). With an older one the build answers <em>This Patina backend has no build</em> and
  everything else keeps working; rebuild and reinstall the package.
</Callout>

<h2>Installing the extension</h2>
<p>
  The package declares <code>[[provides]] interface = "language-intel"</code> with <code>id = "ptn"</code>: the <code>id</code> is the extension
  it serves. A debug build of Arbor finds packages in the repository's <code>plugins/</code> folder; an installed one in the profile's plugin folder.
</p>

<Callout variant="warning" title="Without the package, a .ptn answers nothing">
  No completion, no hover, no diagnostics — and no errors either. A <code>.ptn</code> is never handed to the Java engine, which would read it as Java and
  answer confidently about the wrong language.
</Callout>
