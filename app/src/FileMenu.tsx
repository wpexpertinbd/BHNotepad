// The File menu, as a button at the left of the tab strip.
//
// The window has no custom native menu bar, so the bulk actions — Save All,
// Close All — need somewhere discoverable to live. Shortcuts alone would mean
// nobody finds them.

import { useEffect, useRef, useState } from "react";

export interface FileMenuActions {
  onNew: () => void;
  onOpen: () => void;
  onSave: () => void;
  onSaveAs: () => void;
  onSaveAll: () => void;
  onClose: () => void;
  onCloseAll: () => void;
}

/** `Cmd` reads better than `Ctrl` on a Mac, and vice versa. */
const ACCEL = navigator.platform.toLowerCase().includes("mac") ? "⌘" : "Ctrl+";

interface Item {
  label: string;
  shortcut: string;
  run: keyof FileMenuActions;
  separated?: boolean;
}

const ITEMS: Item[] = [
  { label: "New tab", shortcut: `${ACCEL}T`, run: "onNew" },
  { label: "Open…", shortcut: `${ACCEL}O`, run: "onOpen" },
  { label: "Save", shortcut: `${ACCEL}S`, run: "onSave", separated: true },
  { label: "Save As…", shortcut: `${ACCEL}⇧S`, run: "onSaveAs" },
  { label: "Save All…", shortcut: `${ACCEL}⌥S`, run: "onSaveAll" },
  { label: "Close tab", shortcut: `${ACCEL}W`, run: "onClose", separated: true },
  { label: "Close All…", shortcut: `${ACCEL}⇧W`, run: "onCloseAll" },
];

export function FileMenu(actions: FileMenuActions) {
  const [open, setOpen] = useState(false);
  const host = useRef<HTMLDivElement | null>(null);

  // Close on a click elsewhere or on Escape, the way a menu is expected to.
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!host.current?.contains(event.target as Node)) setOpen(false);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  return (
    <div className="filemenu" ref={host}>
      <button
        type="button"
        className={open ? "filemenu-button is-on" : "filemenu-button"}
        aria-haspopup="menu"
        aria-expanded={open}
        title="File"
        onClick={() => setOpen((shown) => !shown)}
      >
        File
      </button>

      {open && (
        <div className="filemenu-popup" role="menu">
          {ITEMS.map((item) => (
            <button
              key={item.label}
              type="button"
              role="menuitem"
              className={item.separated ? "is-separated" : undefined}
              onClick={() => {
                setOpen(false);
                actions[item.run]();
              }}
            >
              <span>{item.label}</span>
              <span className="shortcut">{item.shortcut}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
