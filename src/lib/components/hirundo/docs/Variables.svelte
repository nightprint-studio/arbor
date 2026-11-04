<h1>Variables</h1>
<p class="doc-lead">Write <code>&#123;&#123;baseUrl&#125;&#125;</code> where a value changes with the environment, and say once what it is.</p>

<h2>Using a variable</h2>
<p>
  Anywhere in the URL, a param, a header, the auth or the body: <code>&#123;&#123;baseUrl&#125;&#125;/orders/&#123;&#123;id&#125;&#125;</code>. Typing <code>&#123;&#123;</code> in the URL bar or in a header field lists the variables that exist, each with the layer it comes from and its value.
  A variable nothing defines is underlined in the URL, and a <b>To fill in</b> bar above the editor names it; click the name to define it.
</p>

<h2>Defining one</h2>
<p><b>Create variable</b> (from that bar) asks for the name, the value and where it lives:</p>
<ul>
  <li><b>This environment</b> — staging's <code>baseUrl</code>, with the other environments untouched.</li>
  <li><b>Every environment of a type</b> — the same value for all “staging” environments.</li>
  <li><b>The collection</b> — shared by every request in it, in every environment.</li>
  <li><b>This machine only</b> — not committed: your own port, your own token.</li>
</ul>
<p>A <b>secret</b> is stored in the operating system's keychain and leaves no value in any file; the file only says <code>@secret</code>.</p>
<p>You can also edit all of them in <b>Environments</b> (<kbd>Ctrl</kbd>+<kbd>E</kbd>) and in a collection's settings. <b>Resolved variables</b> (<kbd>Ctrl</kbd>+<kbd>Shift</kbd>+<kbd>V</kbd>) shows what each name is worth for the open request, and from where.</p>

<h2>Dynamic variables</h2>
<ul>
  <li><code>&#123;&#123;$uuid&#125;&#125;</code> — a new UUID on every send.</li>
  <li><code>&#123;&#123;$timestamp&#125;&#125;</code> — seconds since 1970; <code>&#123;&#123;$isoTimestamp&#125;&#125;</code> — the same as ISO-8601.</li>
  <li><code>&#123;&#123;$randomInt&#125;&#125;</code> — a whole number from 0 to 999.</li>
</ul>
