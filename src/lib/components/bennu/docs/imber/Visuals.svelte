<span class="eyebrow">Imber</span>
<h1>Imber: Screenshots and diagrams</h1>

<p class="doc-lead">
  Imber can look at a front end running on localhost, draws Mermaid diagrams and shows pictures in the thread when a picture explains better than text, and you can attach pictures and files to what you send it.
</p>

<h2>Screenshots of a page</h2>
<p>
  The <em>Screenshots of a page</em> family lets Imber see a page served from <code>localhost</code>, <code>127.0.0.1</code> or <code>[::1]</code>, and nothing else. <code>screenshot_page</code> takes a URL, or a port of a process it started plus a path, and can wait for a selector or a number of milliseconds. <code>page_diagnostics</code> reports the console, failed requests and the page text.
</p>
<ul>
  <li><strong>With Playwright in the project</strong>: the picture plus console errors, failed requests and the page text; it can capture one element or the full page.</li>
  <li><strong>Without Playwright</strong>: an installed Chrome, Edge, Brave or Chromium gives the picture only.</li>
  <li>A screenshot appears in the thread as a small preview and enlarges on click or Enter. The model keeps only the latest two, and they count in the context bar. A picture over about 3.5 MB is refused.</li>
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

<h2>Pictures in the thread</h2>
<p>
  <code>show_image</code> puts a picture in the thread: an image file of the project (PNG, JPEG, GIF, WebP or SVG) or an SVG Imber wrote itself, such as a logo or an icon. It shows as a small preview with a caption; click it, or focus it and press Enter, to open it large. When a message holds several pictures the <strong>←</strong> and <strong>→</strong> keys walk them and <strong>Esc</strong> closes. In the large view a click on the picture zooms to 2× at that point and a second click fits it again; <kbd>+</kbd> and <kbd>−</kbd> zoom in steps up to 8× (<kbd>0</kbd> fits), <kbd>Ctrl</kbd>/<kbd>⌘</kbd> + wheel or a pinch zooms at the pointer, and a zoomed picture is moved by dragging or scrolling.
</p>
<ul>
  <li>The picture is for you: Imber does not get its pixels back. To have it <em>look</em> at an image, attach the image.</li>
  <li>A file is read like any other (the reach and the Read family apply, and a file outside the reach asks); a chat has no files, so it shows SVG it wrote. A file over 5 MB is not shown.</li>
  <li>An SVG is drawn as an image, so nothing inside it can run.</li>
  <li>The pictures are kept beside the conversation and go with it when it is deleted.</li>
</ul>

<h2>Attaching files</h2>
<p>
  The <strong>paperclip</strong> in the composer (or the command <em>Imber: Attach files to the message…</em>) opens Bennu's file explorer to pick files; you can also paste a picture from the clipboard, or <strong>drag files</strong> from the desktop onto the composer (a dropped file is sent as its contents, so it must be under 4 MB). Attachments appear as chips above the text, each with an ✕; <strong>Backspace</strong> in an empty field removes the last one. They go with the next message and appear above it in the thread.
</p>
<ul>
  <li><strong>Pictures</strong> (PNG, JPEG, GIF, WebP) are sent to the model as images, up to 3.5 MB each. Like screenshots they cost tokens and count in the context bar; the model keeps seeing the latest four you attached and older ones become a line.</li>
  <li><strong>SVG</strong> is shown as a picture to you and sent as its source text: the model cannot read the image itself.</li>
  <li><strong>Text and code</strong> files are sent inline, named, up to 200 KB each and 500 KB per message. A file that is not UTF-8 is decoded with the project's encoding.</li>
  <li><strong>Anything else</strong> (PDF, archives, Office documents, binaries) is refused with a reason before anything is sent, and your text and files stay where they are.</li>
  <li>Up to eight files per message. Choosing a file is your consent: an attachment is read wherever it is, whatever the reach is set to.</li>
</ul>
