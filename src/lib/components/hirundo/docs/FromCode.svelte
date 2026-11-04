<h1>Detected from code (Bennu)</h1>
<p class="doc-lead">The endpoints Bennu finds in your Spring projects, ready to call.</p>
<p>
  The <i>Detected from code</i> section lists the request mappings of the Bennu projects you choose (open a project in Bennu once so it is indexed). Pick one to make a request: the method,
  the URL on <code>&#123;&#123;baseUrl&#125;&#125;</code> with path variables as <code>&#123;&#123;id&#125;&#125;</code>, query parameters and headers with their Java type and whether they are required, <code>Accept</code> from what the handler produces
  (<code>MediaType</code> constants are read through to real values such as <code>application/json, text/event-stream</code>), <code>Content-Type</code> from what it consumes, and the Java signature in the notes.
  The variables left to fill in are named in a bar above the editor.
</p>
<p>
  Path variables become <code>path:</code> rows in Params with their Java type and a sample value; query parameters, headers and cookies are rows marked required or optional. The body of a <code>@RequestBody</code> starts as a JSON skeleton built from the DTO's fields (nested objects, lists and enums followed a few levels), and the fields it cannot be null (primitives) are listed in the notes.
  In the tests, <code>body.</code> completes with the fields of the endpoint's return type; in a JSON body, keys complete from the request type. Media types written as project constants (<code>Api.JSON_V1</code>) are read from the source around the controller.
</p>
<p>A request whose method and path match a detected endpoint shows <b>Go to the controller</b> in its toolbar; it opens the source file.</p>
<p>Bennu itself runs <code>.http</code> files through Hirundo's engine: the ▶ beside a request in a <code>.http</code> file sends it, and the answer shows in Bennu's HTTP panel.</p>
