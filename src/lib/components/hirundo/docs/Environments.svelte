<script lang="ts">
  /** Hirundo docs — environments, types, variables and secrets. */
</script>

<h1>Environments</h1>
<p class="doc-lead">
  An <b>environment</b> is a place your API runs — <code>staging-eu</code>, <code>prod</code>, your laptop. Its <b>type</b>
  says what kind of place that is and carries what every place of that kind shares.
</p>

<h2>Types</h2>
<p>
  <i>Local</i>, <i>Dev</i>, <i>Staging</i> and <i>Prod</i> come with Hirundo; edit them, delete them, add your own
  (<code>qa</code>, <code>customer-acme</code>…) in <kbd>Ctrl</kbd>+<kbd>E</kbd> → Types. A type has:
</p>
<ul>
  <li>a <b>colour</b> — the environment selector at the top of the request area is tinted with it, with a “protected” mark when the type asks first, so you read where a request is going before sending it;</li>
  <li><b>protection</b> — “Ask before changing anything”: POST, PUT, PATCH and DELETE need a confirmation. Reading is never asked about.
    The runner asks once, up front, for the whole run;</li>
  <li><b>default variables</b> for every environment of the type.</li>
</ul>
<p>
  Removing a type does not remove its environments: they just lose the colour and the protection.
</p>

<h2>Variables</h2>
<p>
  Write <code>&#123;&#123;baseUrl&#125;&#125;</code> in a URL, header, body or auth field. A name is looked up in layers, and the later layer wins:
  collection → folders → type → environment → local → what a test set at run time.
</p>

<h2>Secrets</h2>
<p>
  Mark a variable as a secret (the lock) and its value is kept in the operating system's keychain — never in a file, never in the
  diff. The same name in staging and in prod are two different secrets. A secret that is not in the keychain is reported as missing,
  and the request is not sent.
</p>

<h2>Local values</h2>
<p>
  Variables that exist only on this machine, in <code>local.toml</code>. A test that calls <code>hirundo.env.set</code> writes here, which is
  how a login feeds the next request.
</p>
