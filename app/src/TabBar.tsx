// The tab strip.
//
// It PAGES rather than scrolls. With fifteen tabs open, the ones that do not
// fit used to be simply unreachable: the strip was a scroll container with its
// scrollbar hidden, so there was nothing to drag and nothing to click, and the
// only way back to tab 12 was to close tabs until it reappeared. The ‹ and ›
// buttons before + move by a whole strip-width at a time, the way Notepad++
// does, and they land on a tab boundary so a page never starts mid-tab.
//
// A tab's close button never asks anything — closing is immediate and the text
// is already on disk. The dot on a tab means "this text lives in the session,
// not in a file", which for a scratch note is the normal resting state, not a
// warning. See CLAUDE.md.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import type { Buffer } from "./ipc";

interface Props {
  /** Rendered at the left of the strip — the File menu. */
  leading?: React.ReactNode;
  buffers: Buffer[];
  active: number;
  onSelect: (index: number) => void;
  onClose: (id: string) => void;
  onNew: () => void;
  onReorder: (from: number, to: number) => void;
}

export function TabBar({
  leading,
  buffers,
  active,
  onSelect,
  onClose,
  onNew,
  onReorder,
}: Props) {
  const dragFrom = useRef<number | null>(null);
  const [dragOver, setDragOver] = useState<number | null>(null);
  const strip = useRef<HTMLDivElement | null>(null);
  /** Whether there are tabs hidden off each end, which is what shows the
   *  arrows at all — an editor with four tabs should not grow furniture. */
  const [hidden, setHidden] = useState({ left: false, right: false });

  /**
   * How far a tab sits from the start of the strip's scrollable content.
   *
   * ⚠️ NOT `offsetLeft`: that is measured from the nearest POSITIONED
   * ancestor, and `.tabs` is not positioned, so it answers in the page's
   * coordinates instead of the strip's. Paging on it landed mid-tab — five
   * pixels of "new 7" showing at the left edge, which is exactly the thing
   * these buttons exist to prevent.
   */
  const contentLeft = (strip: HTMLElement, tab: HTMLElement) =>
    tab.getBoundingClientRect().left - strip.getBoundingClientRect().left + strip.scrollLeft;

  const measure = useCallback(() => {
    const el = strip.current;
    if (!el) return;
    // A pixel of slack: fractional layout widths make an exact comparison
    // claim there is more to the right when the last tab is fully visible.
    const left = el.scrollLeft > 1;
    const right = el.scrollLeft + el.clientWidth < el.scrollWidth - 1;
    // Only when something actually changed. This runs on every scroll event,
    // and while a drag is auto-scrolling that is sixty times a second — a new
    // object each time would re-render the whole strip for nothing.
    setHidden((was) => (was.left === left && was.right === right ? was : { left, right }));
  }, []);

  /** Moves one strip-width, then backs up to the nearest tab's left edge so
   *  the page begins with a whole tab instead of a sliver of one. */
  const page = (direction: -1 | 1) => {
    const el = strip.current;
    if (!el) return;
    const target = el.scrollLeft + direction * el.clientWidth;
    const tabs = [...el.children] as HTMLElement[];

    let left = target;
    if (direction === 1) {
      // The last boundary at or before the target: a tab cut in half by the
      // right edge becomes the first tab of the next page rather than being
      // skipped over.
      const landing = tabs.filter((tab) => contentLeft(el, tab) <= target).pop();
      if (landing) left = contentLeft(el, landing);
    } else {
      const landing = tabs.find((tab) => contentLeft(el, tab) >= target);
      if (landing) left = contentLeft(el, landing);
    }

    const furthest = Math.max(0, el.scrollWidth - el.clientWidth);
    // ⚠️ Set scrollLeft outright rather than `scrollTo({behavior: "smooth"})`.
    // The animated version repeatedly fires `scroll` on its way, and anything
    // that touches the strip while it is still travelling leaves it stopped
    // between two tabs — which is how a page ended up showing six pixels of
    // "new 17". A page is a jump, not a journey; Notepad++ jumps too.
    el.scrollLeft = Math.min(furthest, Math.max(0, left));
    measure();
  };

  // ── Dragging a tab onto another page ──────────────────────────────────
  //
  // Reordering is drag-and-drop, and a drop target has to be on screen to be
  // dropped on. Once the strip pages, "move this tab to the front" meant
  // dragging to an edge and watching nothing happen. Holding the tab near
  // either end of the strip now scrolls it, so the whole run of tabs comes
  // past under the pointer.

  /** How close to an end the pointer has to be, in pixels. */
  const EDGE = 56;
  /** Pixels per frame at the very edge; it eases in across the zone. */
  const EDGE_SPEED = 14;

  const edgeTimer = useRef<number | null>(null);
  const edgeSpeed = useRef(0);
  /** When the pointer was last seen over the strip, for the watchdog below. */
  const lastOver = useRef(0);

  const stopEdgeScroll = useCallback(() => {
    if (edgeTimer.current !== null) window.clearInterval(edgeTimer.current);
    edgeTimer.current = null;
    edgeSpeed.current = 0;
  }, []);

  const edgeScroll = (clientX: number) => {
    const el = strip.current;
    if (!el) return;
    const box = el.getBoundingClientRect();
    lastOver.current = Date.now();

    // Ease in, so brushing the edge nudges and parking at it travels.
    const intoLeft = box.left + EDGE - clientX;
    const intoRight = clientX - (box.right - EDGE);
    if (intoLeft > 0) {
      edgeSpeed.current = -Math.ceil((Math.min(intoLeft, EDGE) / EDGE) * EDGE_SPEED);
    } else if (intoRight > 0) {
      edgeSpeed.current = Math.ceil((Math.min(intoRight, EDGE) / EDGE) * EDGE_SPEED);
    } else {
      edgeSpeed.current = 0;
    }

    if (edgeTimer.current !== null) return;
    edgeTimer.current = window.setInterval(() => {
      const el = strip.current;
      // ⚠️ The watchdog: dragging out of the strip entirely (down into the
      // editor, or off the window) fires no further dragover here, and
      // without this the strip would keep travelling on its own.
      if (!el || Date.now() - lastOver.current > 200) {
        stopEdgeScroll();
        return;
      }
      if (edgeSpeed.current === 0) return;
      const before = el.scrollLeft;
      el.scrollLeft = before + edgeSpeed.current;
      if (el.scrollLeft === before) return; // already at that end
      measure();
    }, 16);
  };

  // A drag that ends anywhere at all must stop the strip — including one
  // dropped on another window or cancelled with Escape, neither of which
  // reaches the tab's own handlers.
  useEffect(() => {
    const stop = () => stopEdgeScroll();
    window.addEventListener("dragend", stop);
    window.addEventListener("drop", stop);
    return () => {
      window.removeEventListener("dragend", stop);
      window.removeEventListener("drop", stop);
      stopEdgeScroll();
    };
  }, [stopEdgeScroll]);

  // Keep the active tab on screen. Ctrl+Tab, a new tab and closing a tab can
  // all land on one that is currently off the end; without this the editor
  // would show a document whose tab is nowhere to be seen.
  useLayoutEffect(() => {
    const el = strip.current;
    const tab = el?.children[active] as HTMLElement | undefined;
    if (!el || !tab) return;
    const left = contentLeft(el, tab);
    if (left < el.scrollLeft) {
      el.scrollLeft = left;
    } else if (left + tab.offsetWidth > el.scrollLeft + el.clientWidth) {
      el.scrollLeft = left + tab.offsetWidth - el.clientWidth;
    }
    measure();
  }, [active, buffers, measure]);

  // The window being resized changes how many tabs fit, and so whether the
  // arrows are needed at all.
  useEffect(() => {
    const el = strip.current;
    if (!el) return;
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [measure]);

  return (
    <div className="tabbar" role="tablist">
      {leading}
      <div
        className="tabs"
        ref={strip}
        onScroll={measure}
        // On the strip rather than on each tab: the gap after the last tab is
        // part of the strip too, and that is exactly where a drag to the end
        // comes to rest.
        onDragOver={(event) => {
          event.preventDefault();
          edgeScroll(event.clientX);
        }}
        onDrop={stopEdgeScroll}
      >
        {buffers.map((buffer, index) => (
          <div
            key={buffer.id}
            role="tab"
            aria-selected={index === active}
            tabIndex={index === active ? 0 : -1}
            className={[
              "tab",
              index === active ? "is-active" : "",
              dragOver === index ? "is-dragover" : "",
            ]
              .filter(Boolean)
              .join(" ")}
            // The full path is the useful detail a truncated label hides.
            title={buffer.path ?? `${buffer.title} — not saved to a file`}
            draggable
            onDragStart={() => {
              dragFrom.current = index;
            }}
            onDragOver={(event) => {
              event.preventDefault();
              setDragOver(index);
            }}
            onDragLeave={() => setDragOver((at) => (at === index ? null : at))}
            onDrop={(event) => {
              event.preventDefault();
              stopEdgeScroll();
              const from = dragFrom.current;
              dragFrom.current = null;
              setDragOver(null);
              if (from !== null && from !== index) onReorder(from, index);
            }}
            onDragEnd={() => {
              stopEdgeScroll();
              dragFrom.current = null;
              setDragOver(null);
            }}
            onMouseDown={(event) => {
              // Middle-click closes, as in Notepad++ and every browser.
              if (event.button === 1) {
                event.preventDefault();
                onClose(buffer.id);
                return;
              }
              if (event.button === 0) onSelect(index);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onSelect(index);
              }
            }}
          >
            {buffer.dirty && <span className="dot" aria-hidden="true" />}
            <span className="label">{buffer.title}</span>
            <button
              type="button"
              className="close"
              aria-label={`Close ${buffer.title}`}
              onMouseDown={(event) => event.stopPropagation()}
              onClick={(event) => {
                event.stopPropagation();
                onClose(buffer.id);
              }}
            >
              ×
            </button>
          </div>
        ))}
      </div>
      {(hidden.left || hidden.right) && (
        <>
          <button
            type="button"
            className="tabpage"
            disabled={!hidden.left}
            aria-label="Earlier tabs"
            title="Earlier tabs"
            onClick={() => page(-1)}
          >
            ‹
          </button>
          <button
            type="button"
            className="tabpage"
            disabled={!hidden.right}
            aria-label="Later tabs"
            title="Later tabs"
            onClick={() => page(1)}
          >
            ›
          </button>
        </>
      )}
      <button type="button" className="newtab" onClick={onNew} title="New tab (Cmd/Ctrl+T)">
        +
      </button>
    </div>
  );
}
