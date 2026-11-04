<h1>The response</h1>
<p class="doc-lead">Status, time and size at a glance; the body as it should be read.</p>
<ul>
  <li><b>Pretty</b> — JSON indented, XML indented (a SOAP Fault is lifted into a card), HTML highlighted. <b>Raw</b> is the text as it arrived. <b>Preview</b> renders HTML in a sandbox: no scripts, no forms.</li>
  <li><b>Text is decoded by the charset the server declared</b> (<code>ISO-8859-1</code>, <code>windows-1252</code>, UTF-8…), else by what the document says about itself (BOM, <code>&lt;meta charset&gt;</code>, XML declaration), else UTF-8 and finally Windows-1252.</li>
  <li><b>Images</b> are shown; any other binary (a PDF, an archive) is described and can be saved with <i>Save as…</i>.</li>
  <li><b>Headers</b> lists them; <b>Tests</b> shows the checks, with the ones that failed first in the runner.</li>
  <li><b>Compare with the previous response</b> shows what changed since the last send of the same request.</li>
</ul>
<p>Bodies over 8 MB are cut, and the panel says so.</p>
