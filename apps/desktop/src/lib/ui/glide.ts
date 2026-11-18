/**
 * Svelte action for a positioned container with a `[data-pill]` highlight and
 * `[data-item]` children: keeps the highlight on the item at the given index,
 * so a new selection glides over instead of jumping. The first placement and
 * layout changes (such as another language) do not animate.
 */
export function glide(container: HTMLElement, index: number) {
  let current = index;

  function place(animate: boolean) {
    const pill = container.querySelector<HTMLElement>("[data-pill]");
    const item = container.querySelectorAll<HTMLElement>("[data-item]")[current];
    if (!pill) return;
    pill.style.visibility = item ? "visible" : "hidden";
    if (!item) return;
    if (!animate) pill.style.transition = "none";
    pill.style.transform = `translate(${item.offsetLeft}px, ${item.offsetTop}px)`;
    pill.style.width = `${item.offsetWidth}px`;
    pill.style.height = `${item.offsetHeight}px`;
    if (!animate) {
      void pill.offsetWidth; // apply the jump before transitions come back
      pill.style.removeProperty("transition");
    }
  }

  place(false);
  const observer = new ResizeObserver(() => place(false));
  observer.observe(container);
  return {
    update(next: number) {
      if (next === current) return;
      current = next;
      place(true);
    },
    destroy() {
      observer.disconnect();
    },
  };
}
