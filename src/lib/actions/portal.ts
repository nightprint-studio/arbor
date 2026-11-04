/**
 * `use:portal` — move the element to the end of `<body>` (or `target`) while it is mounted.
 *
 * For a floating panel drawn from inside a modal: a modal's content can carry a transform or clip its
 * overflow, either of which would make `position: fixed` mean something other than the viewport.
 */
export function portal(node: HTMLElement, target: HTMLElement = document.body) {
  target.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
