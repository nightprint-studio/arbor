<h1>The request editor</h1>
<p class="doc-lead">Method and URL on top, sections below, the answer in the bottom panel.</p>

<h2>The bar</h2>
<p>
  The method keeps its own colour everywhere (GET green, POST amber, PUT blue, PATCH violet, DELETE red). The URL is a code-editor field: variables are coloured, a typed <code>/</code> offers the paths Bennu found in your code,
  and <code>?a=1&amp;b=2</code> edits the Params. <b>Send</b> is <kbd>Ctrl</kbd>+<kbd>Enter</kbd>. <b>Save</b> is in the title bar, with <i>Save as…</i> in its menu.
</p>

<h2>Sections</h2>
<ul>
  <li><b>Params</b> and <b>Headers</b> — a table with a switch per row. <i>Bulk edit</i> in the table's header turns it into text, one entry per line (or paste a block of headers). Header names and their usual values are suggested as you type.</li>
  <li><b>Auth</b> — inherit from the folder or collection, none, Bearer, Basic or API key; <i>WS-Security</i> appears only on a SOAP request.</li>
  <li><b>Body</b> — none, JSON, text, XML, form, multipart (files as <code>@/path/to/file</code>) or SOAP.</li>
  <li><b>Tests</b> — see <i>Tests and runner</i>.</li>
  <li><b>Docs</b> — notes in Markdown; the answer can be pinned there as an example.</li>
</ul>

<h2>Streams and required fields</h2>
<p>
  A request whose <code>Accept</code> is <code>text/event-stream</code> (or with <i>Server-Sent Events</i> on in the request settings) is a stream: <b>Send</b> listens to it and the events appear in the Event stream panel.
  Rows marked <i>required</i> in their description (they come from a detected endpoint) that are still empty make Send ask first. A <code>path:name</code> row in Params is a <b>path variable</b>: its value replaces <code>&#123;&#123;name&#125;&#125;</code> in the URL and is never sent as a query parameter.
</p>

<h2>Tabs</h2>
<p>
  A click in the tree opens a <i>preview</i> tab (italic) that the next click replaces; editing or double-clicking keeps it. Drag tabs to reorder (or <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>PageUp</kbd>/<kbd>PageDown</kbd>);
  right click for Close, Close Others, Close All, Close to the Right, Rename (<kbd>F2</kbd>), Duplicate, Copy Path and Show in File Explorer.
</p>
