<script lang="ts">
  /**
   * New Project: the wizard that creates a project from a generator — what it offers, what each
   * generator writes or runs, how the destination is protected, and the keyboard flow.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
</script>

<span class="eyebrow">Projects</span>
<h1>New Project</h1>

<p class="doc-lead">
  The New Project wizard creates a project on disk from a <strong>generator</strong>: you pick a project type,
  give it a name and a folder, adjust its options, and Bennu opens the result. It is reached from the palette
  (<em>New Project…</em>), from <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>P</kbd>, from the main menu, and from
  the empty Project panel when nothing is open.
</p>

<h2>The common case</h2>
<ol class="step-list">
  <li>Pick a project type on the left — the arrow keys move through the list and the form follows.</li>
  <li>Type a <strong>Name</strong>. The <strong>Location</strong> is the one you used last time.</li>
  <li>Press <kbd>Ctrl</kbd> + <kbd>Enter</kbd>.</li>
</ol>
<p>
  Everything else — the Java level, the group, the edition, Git — starts at a sensible default, and the
  choices you make are remembered for the next project.
</p>

<h2>Project types</h2>
<div class="feature-grid two-col">
  <div class="feature-card">
    <div class="fc-eyebrow">Java</div>
    <div class="fc-title">Spring, Quarkus, Maven, Gradle, web apps</div>
    <div class="fc-desc">
      <strong>Spring Initializr</strong> and <strong>Quarkus</strong> are downloaded from their own services, with
      the Boot version, language, packaging and a searchable list of dependencies or extensions.
      <strong>Maven</strong> and <strong>Gradle</strong> (Kotlin or Groovy DSL) are written locally with source
      folders, an optional sample class and JUnit 5. <strong>Servlet + JSP</strong> writes a WAR with a
      <code>web.xml</code>, a JSP using JSTL and a servlet, for <code>javax</code> (Tomcat 9) or
      <code>jakarta</code> (Tomcat 10.1+); <strong>Struts 1</strong> writes the legacy Struts 1.3 layout.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Rust</div>
    <div class="fc-title">Binary, library, workspace, Bevy, CLI, web, WASM</div>
    <div class="fc-desc">
      All written locally, so Cargo is not needed to create them. <strong>Bevy</strong> offers the releases
      crates.io knows, with a camera, a sprite, a component, a system and a plugin, the dev profile Bevy
      projects use, an <code>assets/</code> folder, an optional <code>dev</code> feature for dynamic linking
      and an optional RON settings file. <strong>CLI</strong> uses clap, <strong>Web service</strong> axum and
      tokio, <strong>WASM</strong> wasm-bindgen with a page that loads it.
    </div>
  </div>
  <div class="feature-card">
    <div class="fc-eyebrow">Node</div>
    <div class="fc-title">Vite, Angular, SvelteKit, Vue, Next.js, plain package</div>
    <div class="fc-desc">
      The framework generators run the official scaffolding tool (<code>create-vite</code>, the Angular CLI,
      <code>sv create</code>, <code>create-vue</code>, <code>create-next-app</code>) with the options you chose
      as flags, choosing npm, pnpm or Yarn. The tool's output streams into the wizard. A <strong>plain
      package</strong> is written locally, JavaScript or TypeScript, ES modules or CommonJS.
    </div>
  </div>
</div>

<h2>Name, location and the destination</h2>
<p>
  The line under the two fields always says what will happen: that the folder will be created (and which
  parent folders with it), that an existing <em>empty</em> folder will be used, or why the location can't be
  used. The name is checked against the rules of the project type — a Cargo package name, a Maven artifact id,
  an npm package name.
</p>
<Callout variant="warning" title="An existing folder is never written into">
  If the folder exists and has anything in it, the wizard asks for a different name. There is no override.
</Callout>

<h2>Git</h2>
<p>
  <em>Create Git repository</em> runs <code>git init</code> in the new project and adds a <code>.gitignore</code>
  suited to the project type. It is disabled when <code>git</code> isn't on the <code>PATH</code>.
</p>

<h2>Tools that must be installed</h2>
<p>
  The footer shows which of git, Node, npm, Cargo, Java and Maven were found. A generator that needs a tool
  that is missing — the Node scaffolders need <code>node</code>, <code>npm</code> and <code>npx</code> — is shown
  dimmed with the reason, and can't be created until it is installed and Bennu is restarted. Locally written
  projects never need a tool to be <em>created</em>, only to be built.
</p>

<h2>What leaves your machine</h2>
<p>
  Only four kinds of request exist, to fixed hosts: the Spring options and download from
  <code>start.spring.io</code>, the Quarkus extension list and download from <code>code.quarkus.io</code>, the Bevy
  release list from the crates.io index (the same switch as the other Cargo tools), and
  whatever the Node scaffolders themselves fetch from the npm registry. If a service can't be reached the form
  says so and offers <em>Try again</em>; the option lists last seen are shown meanwhile, marked as such.
</p>

<h2>Creating, cancelling, failing</h2>
<p>
  The project is built in a scratch folder next to the destination and moved into place in one step when it is
  complete. A downloaded archive is unpacked with limits on the number and size of the files and is rejected if
  any entry would land outside the project. If anything fails — or you cancel, which also stops the scaffolding
  tool and everything it started — the scratch folder and any parent folders the run created are removed, and
  the wizard returns to the form with everything you typed.
</p>
<p>
  When it succeeds the project opens (added to the current workspace, or instead of it — your choice when a
  project is already open) with its README or main file. A step that is allowed to fail without undoing the
  project, such as installing dependencies, is reported as a warning.
</p>

<h2>Keyboard</h2>
<table>
  <thead><tr><th>Shortcut</th><th>Does</th></tr></thead>
  <tbody>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>P</kbd></td><td>Opens the wizard</td></tr>
    <tr><td><kbd>↑</kbd> / <kbd>↓</kbd> in the type list</td><td>Selects the previous / next project type; <kbd>Enter</kbd> or <kbd>→</kbd> moves into the form</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>↑</kbd> / <kbd>↓</kbd></td><td>Switches the project type from anywhere in the form</td></tr>
    <tr><td><kbd>Enter</kbd> in Name or Location</td><td>Creates the project</td></tr>
    <tr><td><kbd>Ctrl</kbd> + <kbd>Enter</kbd></td><td>Creates the project from anywhere</td></tr>
    <tr><td>In a dependency or extension list</td><td>Type to filter; <kbd>↑</kbd> / <kbd>↓</kbd> move, <kbd>Enter</kbd> ticks, <kbd>Backspace</kbd> on an empty filter removes the last choice, <kbd>Esc</kbd> clears the filter</td></tr>
    <tr><td><kbd>Esc</kbd></td><td>Closes the wizard; during a run it cancels it</td></tr>
  </tbody>
</table>
