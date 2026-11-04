<script lang="ts">
  /** Hirundo docs — Getting Started. */
</script>

<h1>Hirundo</h1>
<p class="doc-lead">
  Hirundo is Arbor's HTTP client: collections of requests that live <b>as files in your project</b>, environments
  that know whether they are your laptop or production, auth that you set once, and tests you can run again.
</p>

<h2>What a project is, and how to make one</h2>
<p>
  A <b>project</b> is a folder with a <code>.hirundo</code> folder inside it — usually the root of the repository whose API you call, so the requests live
  next to the code. Open one with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd> (recent projects, or browse for a folder; a folder without <code>.hirundo</code> becomes a project the first time you save something),
  or make one with <b>New project…</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>P</kbd>): choose where it lives and, if you like, a name — a new folder of that name is created inside it.
  The empty window, the menu, the workspace switcher and the command palette all offer both.
</p>
<p>
  Every project you open joins the <b>active workspace</b>, a named group of projects you switch between from the title bar; <i>Add project…</i> in the switcher adds one to a chosen workspace.
</p>

<h2>A project, and what Hirundo keeps in it</h2>
<p>
  Open a folder with <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>O</kbd>. Hirundo writes a <code>.hirundo/</code> folder inside
  it, the first time you save something:
</p>
<ul>
  <li><code>collections/&lt;name&gt;/</code> — one readable file per request, with <code>collection.toml</code> (and a
    <code>folder.toml</code> per folder) holding the auth, shared headers and variables for everything below.</li>
  <li><code>environments/</code> and <code>env-types.toml</code> — the places requests go, and the kinds of place there are.</li>
  <li><code>local.toml</code> — the active environment and your own variables. Git-ignored: it never travels.</li>
</ul>
<p>
  The request files are plain text, written so that a change reads as a change in a diff and a review in Corvus is
  possible. Secrets are never in them.
</p>

<h2>The window</h2>
<p>
  The rail on the left opens the sidebar — <b>Collections</b> (<kbd>Ctrl</kbd>+<kbd>1</kbd>), <b>Detected from code</b>
  (<kbd>Ctrl</kbd>+<kbd>2</kbd>) and <b>Environments</b> (<kbd>Ctrl</kbd>+<kbd>3</kbd>) — and the button at its foot toggles the
  <b>bottom panel</b> (<kbd>Ctrl</kbd>+<kbd>J</kbd>): the <i>Response</i> of the open request, the <i>Runner</i>, the <i>History</i> of what was
  sent and run, and the <i>Schedules</i>. Both panels resize by their edge. The environment a request goes to is picked at the top of the
  request area, next to the tabs.
</p>
<p>
  In the Collections tree, a right click (or the menu key) offers new request, new folder, rename (<kbd>F2</kbd>), duplicate, copy as cURL,
  settings, run and delete (<kbd>Del</kbd>) — whatever fits what the row is.
</p>

<p>
  Open requests are tabs: drag one to reorder it (or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PageUp</kbd>/<kbd>PageDown</kbd>). A right click on a tab offers Close, Close Others, Close All,
  Close to the Right, Close Without Changes, Rename (<kbd>F2</kbd>, the request is renamed in its file), Duplicate, Copy Path and Show in File Explorer; closing several tabs at once asks first when
  some hold unsaved changes. The same verbs are in the command palette.
</p>

<h2>Workspaces</h2>
<p>
  Like Bennu and Corvus, Hirundo groups projects in <b>workspaces</b>: the switcher in the title bar lists each workspace with its projects,
  and a click goes straight to one. “Manage workspaces…” renames, recolours, adds and removes projects. A workspace is only a list; the
  projects are never moved.
</p>

<h2>Sending a request</h2>
<p>
  Create a collection, add a request, type the URL and press <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. Text after <code>?</code>
  in the URL bar becomes the params. A request with a <code>&#123;&#123;variable&#125;&#125;</code> that has no value is not sent:
  the response pane lists what is missing. <kbd>Esc</kbd> cancels a request in flight.
</p>
<p>
  A single click on a request in the tree opens a <i>preview</i> tab (italic) that the next click replaces; edit it, or
  double-click, and it stays. A dot on a tab means unsaved edits.
</p>

<h2>Auth</h2>
<p>
  Set Bearer, Basic or an API key on a <b>collection</b>, a <b>folder</b> or a single <b>request</b>. Whoever is below inherits it,
  can replace it, or can choose “No auth”. The request's Auth tab says what it inherits and from where.
</p>

<h2>Bulk edit, cURL, code</h2>
<ul>
  <li><b>Bulk edit</b> (the toggle in the header of each table, or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>B</kbd>) shows params, headers, form fields and variables as text: paste
    <code>key: value</code> lines, <code>k=v&amp;k2=v2</code>, or a block of headers copied from a browser.</li>
  <li><b>cURL</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>U</kbd>) turns a pasted <code>curl</code> command — Chrome's “Copy as cURL” included —
    into a request, and copies the open request as one, with variables filled in or kept.</li>
  <li><b>Generate code</b> gives the request as cURL, Rust, Java or JavaScript.</li>
  <li><b>Resolved variables</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>) shows the request exactly as it will go out.</li>
</ul>

<h2>Import</h2>
<ul>
  <li><b>OpenAPI 3 (JSON)</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>Y</kbd>) — from a file, a URL or pasted. A dialog lists the operations grouped by tag, with a
    filter, a checkbox per tag and per operation, and the count; the ones you keep become a collection (or a folder in one) named after the spec, with a
    sub-folder per tag, parameters, an example body built from the schema, the spec's descriptions in the Docs, the spec's server as <code>&#123;&#123;baseUrl&#125;&#125;</code> and its security
    scheme as the auth of the folder. YAML specs are not supported yet.</li>
  <li><b>WSDL or SOAP envelope</b> — one request per operation of a WSDL (pasted, or by URL), or one request from an envelope.</li>
  <li><b>.http files</b> — IntelliJ's HTTP Client format, with the environments of its <code>http-client.env.json</code>. A collection can be exported as a <code>.http</code> file. JavaScript response
    handlers are reported and not run: Hirundo's tests are assertions and Lua. <code>&#123;&#123;$uuid&#125;&#125;</code>, <code>&#123;&#123;$timestamp&#125;&#125;</code>, <code>&#123;&#123;$isoTimestamp&#125;&#125;</code>, <code>&#123;&#123;$randomInt&#125;&#125;</code> and
    <code>&#123;&#123;$random.integer(a, b)&#125;&#125;</code> are generated on every send.</li>
</ul>

<h2>SOAP, multipart, cookies and event streams</h2>
<ul>
  <li><b>SOAP</b> — choose the SOAP body: an envelope, the version (1.1 or 1.2) and the SOAPAction, from which the right <code>Content-Type</code> and <code>SOAPAction</code> header are set.
    WS-Security (a <code>UsernameToken</code>) is an auth type. A <code>Fault</code> in the answer is shown on its own, and the XML is indented.</li>
  <li><b>Multipart</b> — a form body of text fields and files: a value starting with <code>@</code> is a file, the description is its content type. cURL's <code>-F</code> is read and written.</li>
  <li><b>Cookies</b> (<kbd>Ctrl</kbd>+<kbd>7</kbd>) — each environment has a jar, kept in your profile and never in the project: <code>Set-Cookie</code> is kept and the cookies are sent back by domain,
    path, expiry and <code>Secure</code>. A request opts out in its settings.</li>
  <li><b>Event streams</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>L</kbd>) — Server-Sent Events of the open request, live, with pause, clear, a filter by event name and the log; it reconnects on its own with
    <code>Last-Event-ID</code>, and <kbd>Esc</kbd> stops it.</li>
</ul>

<h2>.http files in Bennu</h2>
<p>
  Bennu opens <code>.http</code> and <code>.rest</code> files with colouring, completion (methods, common headers, variables) and a ▶ beside every request; <kbd>Ctrl</kbd>+<kbd>Enter</kbd> runs
  the request under the caret and <i>HTTP: Run every request in this file</i> (palette) runs them all, through Hirundo's engine. The answer is in the HTTP panel on the right, with the same tabs and status bar as
  Hirundo's. <i>HTTP: New .http file</i> creates a starter file at the project root.
</p>
<ul>
  <li><b>Variables</b>: <code>@name = value</code> lines, the selected environment of <code>http-client.env.json</code> (and its <code>http-client.private.env.json</code>) in the file's folder or above, and
    <code>&#123;&#123;$uuid&#125;&#125;</code>, <code>&#123;&#123;$timestamp&#125;&#125;</code>, <code>&#123;&#123;$isoTimestamp&#125;&#125;</code>, <code>&#123;&#123;$randomInt&#125;&#125;</code>, <code>&#123;&#123;$random.integer(a, b)&#125;&#125;</code>. An undefined variable and a request with no URL are underlined.</li>
  <li><b>Environments</b>: a name that contains <i>prod</i> (or <i>production</i>) is protected like Hirundo's Prod type: a request that changes something asks first.</li>
  <li><b>Checks</b>: under a request, <code># @hirundo-test status == 200</code> lines and <code># @hirundo-lua …</code> lines are Hirundo's assertions and Lua; IntelliJ ignores them as comments.</li>
  <li><b>Not compatible</b>: JavaScript response handlers (<code>&gt; &#123;% … %&#125;</code>), <code>&gt; file</code> response saving, <code>&lt;&gt;</code> references, <code>client.*</code> / <code>request.*</code> objects, and
    the <code>.env.json</code> keys of IntelliJ's authentication helpers are read and ignored. Bodies from a file (<code>&lt; ./body.json</code>) work for text and JSON bodies; not inside a multipart body.</li>
</ul>

<h2>Responses</h2>
<p>
  Pretty, Raw, Headers and Tests views; the last two answers can be compared side by side, and a response can be kept as an
  example in the request's Docs. What this window sent is in the History section until it is closed.
</p>

<h2>Detected from code</h2>
<p>
  The endpoints Bennu's index holds — Spring request mappings, grouped by controller — for the Bennu projects you choose in the
  <i>Projects</i> menu (only those are read; until you choose, it is the Bennu project that is also this window's project). Enter opens an
  endpoint as a new request: method, URL on <code>&#123;&#123;baseUrl&#125;&#125;</code>, its query parameters and the shape of its body, taken from the code.
  A project must have been opened in Bennu once so that its index exists. <i>Go to the controller</i> appears in the request toolbar only for a request that matches a detected endpoint, and opens its source.
</p>

