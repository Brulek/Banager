/**
 * Marks a box that scrolls `data-more-below` while there is more of it
 * below what is in sight -- as it is laid out, as it is scrolled, and as
 * what it holds grows or changes -- and takes the mark off once its end is
 * in sight. index.css fades such a box's bottom edge (a dialog's body), so
 * that its last line in sight reads as cut by the edge, not as the end of
 * what it says. Set on the element itself, not through React: a scroll
 * draws nothing again. Returns what stops watching.
 */
export function watchMoreBelow(box: HTMLElement): () => void {
  const check = () => {
    const more = box.scrollHeight - box.scrollTop - box.clientHeight > 1;
    if (more !== box.hasAttribute("data-more-below")) box.toggleAttribute("data-more-below", more);
  };
  // The box, and each thing directly in it: one that grows -- a plan
  // arriving, a list drawn in turn -- grows the box's content, whose own
  // size stays put once it has reached its height.
  const resize = new ResizeObserver(check);
  const observe = () => {
    resize.disconnect();
    resize.observe(box);
    for (const child of box.children) resize.observe(child);
  };
  const added = new MutationObserver(() => {
    observe();
    check();
  });
  observe();
  added.observe(box, { childList: true });
  box.addEventListener("scroll", check, { passive: true });
  check();
  return () => {
    resize.disconnect();
    added.disconnect();
    box.removeEventListener("scroll", check);
  };
}
