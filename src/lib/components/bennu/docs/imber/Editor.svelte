<span class="eyebrow">Imber</span>
<h1>Imber: Working from the editor</h1>

<p class="doc-lead">
  Imber can be asked from the margin of the editor, from a doc comment, from a Markdown selection and from a stack trace, each with a ready-made request; pasted code is fenced automatically and the panel can take the room of the editor.
</p>

<h2>The sparkle in the margin</h2>
<p>
  A sparkle in the margin sits on the line the caret is on, or where a selection starts, and offers what fits what is there:
</p>
<ul>
  <li>on a line with a problem: <em>Fix this error</em> or <em>Explain this problem</em>;</li>
  <li>on a selection: <em>Explain</em>, <em>Look for bugs</em>, <em>Suggest improvements</em>, <em>Write tests</em>;</li>
  <li>on a declaration: <em>Explain</em>, <em>Write tests</em>, <em>Where is it used?</em>, <em>Write documentation</em>;</li>
  <li>always: <em>Ask about this…</em>, which leaves the start of a message in the composer.</li>
</ul>
<p>
  The request carries the file, the lines and the code itself, and starts a conversation of its own. From the keyboard, <em>Ask Imber about this…</em> in the Command Palette does the same, opening the menu at the caret.
</p>

<h2>Doc comments</h2>
<p>
  In a Java or Rust file a sparkle in the gutter sits on every doc comment, and under the caret on a declaration that has none. It offers <em>Improve this comment</em>, <em>Check against the code</em> (parameters, return value, exceptions, statements the code contradicts), <em>Make it shorter</em>, <em>Translate to English / Italian</em> and <em>Write documentation</em>. The request carries the file, the lines and the declaration, so there is nothing to type; the result comes back as a proposal like any other edit.
</p>

<h2>Markdown files</h2>
<p>
  In a Markdown file, selecting a passage shows a small button at its end: <em>Improve the writing</em>, <em>Make it shorter</em>, <em>Say more</em>, <em>Fix spelling and grammar</em>, <em>Translate</em>.
</p>

<h2>Stack traces</h2>
<p>
  In the Run and Build output, the line that starts a stack trace has a <em>Fix with Imber</em> button that sends the trace and asks for the cause in the project.
</p>

<h2>Pasting code into the composer</h2>
<p>
  Code pasted into the message box becomes a fenced block with its language (<code>```java … ```</code>), with a blank line either side, at the caret; one <kbd>Ctrl</kbd> + <kbd>Z</kbd> takes it back out. A paste counts as code when it spans several lines and looks like source (brackets, statements, indentation, a recognisable language such as Java, Kotlin, Rust, TypeScript, JSON, XML, SQL, YAML, properties or shell); prose, lists and single lines are left as they are. The language comes from the text, or from the open file when the text does not say. A paste made inside a fence does not open another one.
</p>
<ul>
  <li>Each block of the message being written is listed above the box with its language, line count and first line. Click a row to read the block highlighted, or the cross to remove it.</li>
  <li>In the thread your message shows its blocks highlighted, like Imber's own.</li>
  <li><kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>V</kbd> pastes exactly what is on the clipboard, with no fence. <strong>Settings › Imber › Wrap pasted code in a fenced block</strong> turns the wrapping off.</li>
  <li>A paste over 200 KB is cut at a line and a notice says so.</li>
</ul>

<h2>Expanding the panel</h2>
<p>
  The button in the panel header (or a double-click on the header, <kbd>Ctrl</kbd> + <kbd>Shift</kbd> + <kbd>U</kbd>, or <em>Imber: Expand panel</em>) covers the editor area and the bottom dock; the title bar, the rails and the side panels stay. The same button, the same keys, <em>Imber: Restore panel</em> or <kbd>Esc</kbd> from inside the panel (when no menu or window is open over it) put it back at the width it had. The editors are only covered, not closed: their scroll, cursors and unsaved changes stay as they were. Expanded, the conversation is a centred column: prose keeps a comfortable line length, while code, diagrams, diffs and the cards for live runs use the full width. Closing the panel, or opening another one on the right, ends the expansion.
</p>

<h2>What travels with a message</h2>
<p>
  The file you are in and the lines you selected travel with the message, shown as chips above the text; the selection is read when you click into the composer. Instruction files in play appear as chips too.
</p>
