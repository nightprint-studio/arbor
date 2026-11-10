---
title: "Imber: Working from the editor"
summary: "The sparkle in the margin, doc comments, Markdown selections, stack traces, pasted code and the expanded panel."
keywords: ["sparkle", "margin", "comment", "javadoc", "stack trace", "paste", "fence", "expand", "markdown"]
related: ["Imber: Getting started", "Imber: Keyboard and palette"]
source: src/lib/components/bennu/docs/imber/Editor.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Working from the editor

Imber can be asked from the margin of the editor, from a doc comment, from a Markdown selection and from a stack trace, each with a ready-made request; pasted code is fenced automatically and the panel can take the room of the editor.

## The sparkle in the margin

A sparkle in the margin sits on the line the caret is on, or where a selection starts, and offers what fits what is there:

- on a line with a problem: *Fix this error* or *Explain this problem*;
- on a selection: *Explain*, *Look for bugs*, *Suggest improvements*, *Write tests*;
- on a declaration: *Explain*, *Write tests*, *Where is it used?*, *Write documentation*;
- always: *Ask about this…*, which leaves the start of a message in the composer.

The request carries the file, the lines and the code itself, and starts a conversation of its own. From the keyboard, *Ask Imber about this…* in the Command Palette does the same, opening the menu at the caret.

## Doc comments

In a Java or Rust file a sparkle in the gutter sits on every doc comment, and under the caret on a declaration that has none. It offers *Improve this comment*, *Check against the code* (parameters, return value, exceptions, statements the code contradicts), *Make it shorter*, *Translate to English / Italian* and *Write documentation*. The request carries the file, the lines and the declaration, so there is nothing to type; the result comes back as a proposal like any other edit.

## Markdown files

In a Markdown file, selecting a passage shows a small button at its end: *Improve the writing*, *Make it shorter*, *Say more*, *Fix spelling and grammar*, *Translate*.

## Stack traces

In the Run and Build output, the line that starts a stack trace has a *Fix with Imber* button that sends the trace and asks for the cause in the project.

## Pasting code into the composer

Code pasted into the message box becomes a fenced block with its language (`` ```java … ``` ``), with a blank line either side, at the caret; one `Ctrl+Z` takes it back out. A paste counts as code when it spans several lines and looks like source (brackets, statements, indentation, a recognisable language such as Java, Kotlin, Rust, TypeScript, JSON, XML, SQL, YAML, properties or shell); prose, lists and single lines are left as they are. The language comes from the text, or from the open file when the text does not say. A paste made inside a fence does not open another one.

- Each block of the message being written is listed above the box with its language, line count and first line. Click a row to read the block highlighted, or the cross to remove it.
- In the thread your message shows its blocks highlighted, like Imber's own.
- `Ctrl+Shift+V` pastes exactly what is on the clipboard, with no fence. **Settings › Imber › Wrap pasted code in a fenced block** turns the wrapping off.
- A paste over 200 KB is cut at a line and a notice says so.

## Expanding the panel

The button in the panel header (or a double-click on the header, `Ctrl+Shift+U`, or *Imber: Expand panel*) covers the editor area and the bottom dock; the title bar, the rails and the side panels stay. The same button, the same keys, *Imber: Restore panel* or `Esc` from inside the panel (when no menu or window is open over it) put it back at the width it had. The editors are only covered, not closed: their scroll, cursors and unsaved changes stay as they were. Expanded, the conversation is a centred column: prose keeps a comfortable line length, while code, diagrams, diffs and the cards for live runs use the full width. Closing the panel, or opening another one on the right, ends the expansion.

## What travels with a message

The file you are in and the lines you selected travel with the message, shown as chips above the text; the selection is read when you click into the composer. Instruction files in play appear as chips too.
