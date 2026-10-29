<span class="eyebrow">Imber</span>
<h1>Imber: Screenshots and diagrams</h1>

<p class="doc-lead">
  Imber can look at a front end running on localhost, and it draws Mermaid diagrams in the thread when a picture explains better than text.
</p>

<h2>Screenshots of a page</h2>
<p>
  The <em>Screenshots of a page</em> family lets Imber see a page served from <code>localhost</code>, <code>127.0.0.1</code> or <code>[::1]</code>, and nothing else. <code>screenshot_page</code> takes a URL, or a port of a process it started plus a path, and can wait for a selector or a number of milliseconds. <code>page_diagnostics</code> reports the console, failed requests and the page text.
</p>
<ul>
  <li><strong>With Playwright in the project</strong>: the picture plus console errors, failed requests and the page text; it can capture one element or the full page.</li>
  <li><strong>Without Playwright</strong>: an installed Chrome, Edge, Brave or Chromium gives the picture only.</li>
  <li>A screenshot appears in the thread and enlarges on click. The model keeps only the latest two, and they count in the context bar. A picture over about 3.5 MB is refused.</li>
  <li>No file is read or written and nothing asks; the restriction to this machine is in the tool.</li>
</ul>
<p>Pictures are expensive in tokens, which is why the family can be switched off on its own.</p>

<h2>Diagrams</h2>
<p>
  When a picture explains better than text (an architecture, a flow, a call chain, the entities of a model) Imber draws a <strong>Mermaid</strong> diagram (flowchart, sequence, class, state, ER, Gantt…) with <code>show_diagram</code>, in a chat as well as in an agent conversation.
</p>
<ul>
  <li>Each diagram is checked before it is shown. If the source does not parse, Imber is told the error with its line and sends a corrected one, up to three tries; after that the source is shown as text with a notice. The failed attempts are dropped from what is sent to the model afterwards, so they cost nothing later.</li>
  <li>A diagram follows the theme, is capped to a readable height and scrolls inside its box. Its buttons copy the Mermaid source and open it larger in a window.</li>
  <li>A <code>mermaid</code> code block in any rendered answer, or in an answer opened as a document, is drawn the same way.</li>
  <li>The source is capped at 20,000 characters.</li>
</ul>
