---
title: "Imber: Screenshots and diagrams"
summary: "Seeing a front end on localhost and drawing Mermaid diagrams in the thread."
keywords: ["screenshot", "playwright", "localhost", "mermaid", "diagram", "flowchart", "sequence"]
related: ["Imber: Tools it can use", "Imber: Cost and context"]
source: src/lib/components/bennu/docs/imber/Visuals.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Screenshots and diagrams

Imber can look at a front end running on localhost, draws Mermaid diagrams and shows pictures in the thread when a picture explains better than text, and you can attach pictures and files to what you send it.

## Screenshots of a page

The *Screenshots of a page* family lets Imber see a page served from `localhost`, `127.0.0.1` or `[::1]`, and nothing else. `screenshot_page` takes a URL, or a port of a process it started plus a path, and can wait for a selector or a number of milliseconds. `page_diagnostics` reports the console, failed requests and the page text.

- **With Playwright in the project**: the picture plus console errors, failed requests and the page text; it can capture one element or the full page.
- **Without Playwright**: an installed Chrome, Edge, Brave or Chromium gives the picture only.
- A screenshot appears in the thread as a small preview and enlarges on click or Enter. The model keeps only the latest two, and they count in the context bar. A picture over about 3.5 MB is refused.
- No file is read or written and nothing asks; the restriction to this machine is in the tool.

Pictures are expensive in tokens, which is why the family can be switched off on its own.

## Diagrams

When a picture explains better than text (an architecture, a flow, a call chain, the entities of a model) Imber draws a **Mermaid** diagram (flowchart, sequence, class, state, ER, Gantt…) with `show_diagram`, in a chat as well as in an agent conversation.

- Each diagram is checked before it is shown. If the source does not parse, Imber is told the error with its line and sends a corrected one, up to three tries; after that the source is shown as text with a notice. The failed attempts are dropped from what is sent to the model afterwards, so they cost nothing later.
- A diagram follows the theme, is capped to a readable height and scrolls inside its box. Its buttons copy the Mermaid source and open it larger in a window.
- A `mermaid` code block in any rendered answer, or in an answer opened as a document, is drawn the same way.
- The source is capped at 20,000 characters.

## Pictures in the thread

`show_image` puts a picture in the thread: an image file of the project (PNG, JPEG, GIF, WebP or SVG) or an SVG Imber wrote itself, such as a logo or an icon. It shows as a small preview with a caption; click it, or focus it and press Enter, to open it large. When a message holds several pictures the **←** and **→** keys walk them and **Esc** closes. In the large view a click on the picture zooms to 2× at that point and a second click fits it again; `+` and `−` zoom in steps up to 8× (`0` fits), `Ctrl`/`⌘` + wheel or a pinch zooms at the pointer, and a zoomed picture is moved by dragging or scrolling.

- The picture is for you: Imber does not get its pixels back. To have it *look* at an image, attach the image.
- A file is read like any other (the reach and the Read family apply, and a file outside the reach asks); a chat has no files, so it shows SVG it wrote. A file over 5 MB is not shown.
- An SVG is drawn as an image, so nothing inside it can run.
- The pictures are kept beside the conversation and go with it when it is deleted.

## Attaching files

The **paperclip** in the composer (or the command *Imber: Attach files to the message…*) opens Bennu's file explorer to pick files; you can also paste a picture from the clipboard, or **drag files** from the desktop onto the composer (a dropped file is sent as its contents, so it must be under 4 MB). Attachments appear as chips above the text, each with an ✕; **Backspace** in an empty field removes the last one. They go with the next message and appear above it in the thread.

- **Pictures** (PNG, JPEG, GIF, WebP) are sent to the model as images, up to 3.5 MB each. Like screenshots they cost tokens and count in the context bar; the model keeps seeing the latest four you attached and older ones become a line.
- **SVG** is shown as a picture to you and sent as its source text: the model cannot read the image itself.
- **Text and code** files are sent inline, named, up to 200 KB each and 500 KB per message. A file that is not UTF-8 is decoded with the project's encoding.
- **Anything else** (PDF, archives, Office documents, binaries) is refused with a reason before anything is sent, and your text and files stay where they are.
- Up to eight files per message. Choosing a file is your consent: an attachment is read wherever it is, whatever the reach is set to.
