---
title: "Imber: Screenshots and diagrams"
summary: "Seeing a front end on localhost and drawing Mermaid diagrams in the thread."
keywords: ["screenshot", "playwright", "localhost", "mermaid", "diagram", "flowchart", "sequence"]
related: ["Imber: Tools it can use", "Imber: Cost and context"]
source: src/lib/components/bennu/docs/imber/Visuals.svelte
---
<!-- Generated from the in-app docs - do not edit. Edit the Svelte pages and run `node scripts/build-help-index.mjs`. -->

# Imber: Screenshots and diagrams

Imber can look at a front end running on localhost, and it draws Mermaid diagrams in the thread when a picture explains better than text.

## Screenshots of a page

The *Screenshots of a page* family lets Imber see a page served from `localhost`, `127.0.0.1` or `[::1]`, and nothing else. `screenshot_page` takes a URL, or a port of a process it started plus a path, and can wait for a selector or a number of milliseconds. `page_diagnostics` reports the console, failed requests and the page text.

- **With Playwright in the project**: the picture plus console errors, failed requests and the page text; it can capture one element or the full page.
- **Without Playwright**: an installed Chrome, Edge, Brave or Chromium gives the picture only.
- A screenshot appears in the thread and enlarges on click. The model keeps only the latest two, and they count in the context bar. A picture over about 3.5 MB is refused.
- No file is read or written and nothing asks; the restriction to this machine is in the tool.

Pictures are expensive in tokens, which is why the family can be switched off on its own.

## Diagrams

When a picture explains better than text (an architecture, a flow, a call chain, the entities of a model) Imber draws a **Mermaid** diagram (flowchart, sequence, class, state, ER, Gantt…) with `show_diagram`, in a chat as well as in an agent conversation.

- Each diagram is checked before it is shown. If the source does not parse, Imber is told the error with its line and sends a corrected one, up to three tries; after that the source is shown as text with a notice. The failed attempts are dropped from what is sent to the model afterwards, so they cost nothing later.
- A diagram follows the theme, is capped to a readable height and scrolls inside its box. Its buttons copy the Mermaid source and open it larger in a window.
- A `mermaid` code block in any rendered answer, or in an answer opened as a document, is drawn the same way.
- The source is capped at 20,000 characters.
