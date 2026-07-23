import { useEffect, useState, type RefObject } from "react";

/**
 * True while the given scroll container is scrolled away from its top edge.
 * Drives the Tahoe scroll-edge effect: toolbars are transparent at rest and
 * gain a glass wash + hairline once content moves beneath them.
 */
export function useScrollEdge(ref: RefObject<HTMLElement>): boolean {
  const [scrolled, setScrolled] = useState(false);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const update = () => setScrolled(el.scrollTop > 0);
    update();
    el.addEventListener("scroll", update, { passive: true });
    return () => el.removeEventListener("scroll", update);
  }, [ref]);

  return scrolled;
}
