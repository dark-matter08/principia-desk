/**
 * A textarea that grows with its text up to a ceiling, then scrolls: enough
 * to read what is there without a field that takes over the page. The
 * `value` in the parameters is only there so the action refits when the
 * text is set from outside, as when a draft arrives from the tutor.
 */
export interface AutosizeOptions {
  /** Lines shown at least. */
  min?: number;
  /** Lines shown at most before the field scrolls. */
  max?: number;
  value?: string;
}

export function autosize(node: HTMLTextAreaElement, options: AutosizeOptions = {}) {
  let current = options;
  function fit() {
    const style = getComputedStyle(node);
    const line = parseFloat(style.lineHeight) || 20;
    const chrome = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom) + parseFloat(style.borderTopWidth) + parseFloat(style.borderBottomWidth);
    const floor = (current.min ?? 2) * line + chrome;
    const ceiling = (current.max ?? 10) * line + chrome;
    node.style.height = 'auto';
    const wanted = Math.max(floor, node.scrollHeight);
    node.style.height = `${Math.min(wanted, ceiling)}px`;
    node.style.overflowY = wanted > ceiling ? 'auto' : 'hidden';
  }
  node.style.resize = 'none';
  node.addEventListener('input', fit);
  const observer = new ResizeObserver(() => fit());
  observer.observe(node);
  requestAnimationFrame(fit);
  return {
    update(next: AutosizeOptions) {
      current = next;
      requestAnimationFrame(fit);
    },
    destroy() {
      node.removeEventListener('input', fit);
      observer.disconnect();
    },
  };
}
