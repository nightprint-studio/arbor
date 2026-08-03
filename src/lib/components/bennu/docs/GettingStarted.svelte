<!-- Bennu docs — Getting Started. Plain semantic HTML; DocsShell supplies typography. -->
<h1>Getting started with Bennu</h1>
<p class="doc-lead">
  Bennu is a focused editor for legacy Java web projects — the Struts / Entando / JDBC
  stack. It opens a Maven project, resolves its JDK and the domain frameworks in play,
  and gives you a fast, keyboard-first place to read and navigate the code. It also opens
  a <strong>Cargo</strong> project, as an editor: tree, search, highlighting and
  <code>cargo check</code>, without the Java machinery.
</p>

<h2>Open a project</h2>
<ul>
  <li><strong>Open project</strong> (<kbd>Ctrl</kbd> + <kbd>O</kbd>) — pick the folder that holds the root <code>pom.xml</code> (Maven) or <code>Cargo.toml</code> (Rust). Bennu reads the build model and, for Maven, detects the frameworks.</li>
  <li>The <strong>Project</strong> tool window (left rail, top) shows the file tree; click a file to open it in a tab.</li>
  <li>The <strong>title-bar switcher</strong> keeps your recent projects one click away.</li>
</ul>

<div class="callout">
  No backend yet? Choose <strong>Load demo project</strong> from the hamburger menu to explore a
  realistic sample (a Struts + Entando portal) with a populated tree, a highlighted Java file
  and the footer showing the JDK and encoding.
</div>

<h2>The layout</h2>
<ul>
  <li><strong>Left rail</strong> — Project (files), Structure (symbols of the open file) and Dependencies. Its buttons toggle the matching tool window.</li>
  <li><strong>Right rail</strong> — Maven and Services, plus the <strong>Forms</strong> toggle (<kbd>Alt</kbd> + <kbd>3</kbd>) in its bottom cluster.</li>
  <li><strong>Bottom dock</strong> — Build, Problems, TODO, <strong>Forms</strong> (the JSP form analysis for the open file) and the Terminal, tabbed.</li>
  <li><strong>Footer</strong> — the project JDK, detected capabilities, the index status, the open file's encoding and the caret position. On a Cargo project it reads the toolchain and the crate count instead, and the Java-only tools drop out of both rails.</li>
  <li><strong>Settings / Docs</strong> live at the top-right of the title bar (the gear and the book).</li>
</ul>
