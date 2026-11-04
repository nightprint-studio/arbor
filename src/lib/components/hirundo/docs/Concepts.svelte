<h1>Concepts</h1>
<p class="doc-lead">A project, its collections, the requests in them, and the environments they go to.</p>

<h2>Project</h2>
<p>
  A <b>project</b> is a folder with a <code>.hirundo</code> folder inside — usually the root of the repository whose API you call. Everything Hirundo keeps for it
  is there as plain files you can commit and review. Open one with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>, make one with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>.
  Projects are grouped in <b>workspaces</b>, switched from the title bar.
</p>

<h2>Collections, folders, requests</h2>
<p>
  A <b>collection</b> groups the requests of one API. It can hold <b>folders</b>, and both carry settings that everything below them inherits: auth, headers and variables
  (right click → <i>Collection settings</i>). A <b>request</b> is one file: method, URL, params, headers, auth, body, tests and notes.
</p>

<h2>Environments and their types</h2>
<p>
  An <b>environment</b> is a place the API runs — your machine, staging, production — with its own variables (above all <code>baseUrl</code>). Each environment has a <b>type</b>
  (colour, variables shared by every environment of that type, and whether to ask before <code>POST</code>, <code>PUT</code>, <code>PATCH</code> and <code>DELETE</code>). The one selected in the title bar
  is where the next request goes.
</p>

<h2>Variables, in order of strength</h2>
<ol>
  <li><b>This machine</b> — values that are yours alone, never committed (<code>local.toml</code>).</li>
  <li><b>The environment</b> and then its <b>type</b>.</li>
  <li><b>Folders</b>, innermost first, then the <b>collection</b>.</li>
</ol>
<p>The first layer that defines a name wins. Write one as <code>&#123;&#123;name&#125;&#125;</code>; see <i>Variables</i>.</p>
