<script lang="ts">
  /**
   * Bennu's front page. Corvus's Getting Started is the model — an eyebrow, a lead, the
   * numbers that say what this is built out of, then the three things somebody does in
   * their first ten minutes: open a project, find their way around the window, and learn
   * that everything is on the keyboard.
   *
   * The rest of the manual is deliberately made of small pages; this one is the map to
   * them, which is the job a landing page actually has.
   */
  import Callout from '$lib/components/shared/ui/Callout.svelte';
  import { bennuOnboardingStore } from '$lib/stores/bennu/onboarding.svelte';
</script>

<span class="eyebrow">Welcome</span>
<h1>Getting Started</h1>

<p class="doc-lead">
  Bennu is a code intelligence engine with an editor around it, aimed at the projects where the
  behaviour is not in the code: Struts, Spring XML, MyBatis, JSP. It also opens a Cargo workspace
  as a first-class Rust editor. Everything it knows, it knows from an index it builds itself —
  which is why it answers about files you have never opened.
</p>

<div class="stat-row">
  <div class="stat">
    <div class="stat-value">Maven</div>
    <div class="stat-label">Java projects</div>
  </div>
  <div class="stat">
    <div class="stat-value">Cargo</div>
    <div class="stat-label">Rust workspaces</div>
  </div>
  <div class="stat">
    <div class="stat-value">LSP</div>
    <div class="stat-label">Every other language</div>
  </div>
  <div class="stat">
    <div class="stat-value">Rust</div>
    <div class="stat-label">Its own engine</div>
  </div>
</div>

<Callout variant="info" title="Welcome tour">
  The first-run walkthrough is re-openable any time —
  <button class="tour-link" type="button" onclick={() => bennuOnboardingStore.show()}>launch the welcome tour</button>
  — or find it in the Command Palette under <em>Welcome tour</em>.
</Callout>

<Callout variant="tip" title="Quick start">
  <kbd>Ctrl</kbd> + <kbd>O</kbd>, pick the folder holding the root <code>pom.xml</code> or
  <code>Cargo.toml</code>, and the tree, the index and the footer fill in on their own.
</Callout>

<h2>Opening a project</h2>
<ol class="step-list">
  <li>Press <kbd>Ctrl</kbd> + <kbd>O</kbd> and choose the folder with the root manifest in it</li>
  <li>Bennu reads the build model — modules or workspace crates, the JDK level, the frameworks</li>
  <li>The index builds in the background; the footer says how far along it is</li>
</ol>
<p>
  The <strong>title-bar switcher</strong> keeps recent projects one click away, and the hamburger
  menu carries the same list. On macOS those entries live in the system menu bar instead.
</p>

<Callout variant="info" title="No project to hand?">
  <strong>Load demo project</strong> in the hamburger menu opens a realistic sample — a Struts +
  Entando portal — with a populated tree, a highlighted Java file, and the footer showing the JDK
  and the encoding. Everything in this manual can be tried on it.
</Callout>

<h2>The window</h2>
<div class="feature-grid">
  <div class="feature-card">
    <div class="fc-title">Left rail</div>
    <div class="fc-desc">Project files, the Structure of the open file, and Dependencies. Each button toggles its tool window; <kbd>Alt</kbd> + <kbd>1</kbd> … <kbd>Alt</kbd> + <kbd>N</kbd>.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Editor</div>
    <div class="fc-desc">Tabs, folding, sticky scroll, rainbow brackets, and an overview ruler that marks every problem in the file rather than only the ones on screen.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Right rail</div>
    <div class="fc-desc">Maven or Cargo, the test catalogue, the parse and model Trees, and — on a translation bundle — the i18n panel.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Bottom dock</div>
    <div class="fc-desc">Build and Problems (two views of one run), Run (the launched program's console, and the debugger when it is one), TODO, Forms and the Terminal.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Footer</div>
    <div class="fc-desc">The JDK, the detected capabilities, the index state, the open file's encoding and the caret. On a Cargo project it reads the toolchain and the crate count.</div>
  </div>
  <div class="feature-card">
    <div class="fc-title">Activity bar</div>
    <div class="fc-desc">Both rails can be rearranged — <em>Customize Activity Bar…</em>, in the Command Palette and under the title bar's gear. Drag to reorder, and hide the tools you never use.</div>
  </div>
</div>

<h2>Everything from the keyboard</h2>
<p>
  <kbd>Ctrl</kbd> + <kbd>K</kbd> opens the <strong>Command Palette</strong> —
  every action Bennu has, by name. If you can do it, you can find it there, and it will tell you
  the shortcut on the way past.
</p>
<table class="shortcuts-table">
  <thead><tr><th>Action</th><th>Shortcut</th></tr></thead>
  <tbody>
    <tr><td>Go to class</td><td><kbd>Ctrl</kbd> + <kbd>N</kbd></td></tr>
    <tr><td>Go to file</td><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>N</kbd></td></tr>
    <tr><td>Go to declaration</td><td><kbd>Ctrl</kbd> + <kbd>B</kbd></td></tr>
    <tr><td>Find usages</td><td><kbd>Alt</kbd> + <kbd>F7</kbd></td></tr>
    <tr><td>Find in project</td><td><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>F</kbd></td></tr>
    <tr><td>Local history</td><td><kbd>Alt</kbd> + <kbd>Shift</kbd> + <kbd>H</kbd></td></tr>
    <tr><td>Build</td><td><kbd>Ctrl</kbd> + <kbd>F9</kbd></td></tr>
  </tbody>
</table>
<p>The full reference is under <em>Keyboard shortcuts</em>, and every binding is remappable there.</p>

<h2>Where to go next</h2>
<ul>
  <li><strong>Projects</strong> — what Bennu resolves when it opens a folder, and what a capability is.</li>
  <li><strong>Navigation</strong> — the jumps that make a legacy codebase readable.</li>
  <li><strong>Validation &amp; problems</strong> — the squiggles that appear before anything is compiled.</li>
  <li><strong>Language servers</strong> — how every non-Java language is served, and how to install one.</li>
  <li><strong>Local history</strong> — the record of a file's own past, including the files that are gone.</li>
</ul>

<style>
  .tour-link {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }
  .tour-link:hover { color: var(--accent-hover); }
</style>
