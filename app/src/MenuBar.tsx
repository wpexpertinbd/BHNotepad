// The menu bar: File, Edit, Search, View.
//
// Laid out like Notepad++'s — a row of menus above the tab strip — because
// that is the shape the muscle memory expects. Once one menu is open, moving
// the pointer across the others switches between them without clicking again.

import { useEffect, useRef, useState } from "react";

export interface MenuItem {
  label: string;
  shortcut?: string;
  run?: () => void;
  /** Greyed out and unclickable when false. */
  enabled?: boolean;
  /** Draws a divider above this item. */
  separated?: boolean;
  /** A nested list, used for Recent Files and Close Multiple Documents. */
  submenu?: MenuItem[];
  /** Shown in place of an empty submenu. */
  emptyLabel?: string;
}

export interface Menu {
  label: string;
  items: MenuItem[];
}

/** `Cmd` reads better on a Mac, `Ctrl` everywhere else. */
export const ON_MAC = navigator.platform.toLowerCase().includes("mac");
export const ACCEL = ON_MAC ? "⌘" : "Ctrl+";
export const ALT = ON_MAC ? "⌥" : "Alt+";
export const SHIFT = ON_MAC ? "⇧" : "Shift+";

export function MenuBar({ menus }: { menus: Menu[] }) {
  const [open, setOpen] = useState<string | null>(null);
  const [submenu, setSubmenu] = useState<string | null>(null);
  const host = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onPointerDown = (event: MouseEvent) => {
      if (!host.current?.contains(event.target as Node)) setOpen(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(null);
    };
    window.addEventListener("mousedown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("mousedown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  useEffect(() => setSubmenu(null), [open]);

  const renderItems = (items: MenuItem[], depth: number) =>
    items.map((item, index) => {
      const key = `${depth}:${index}:${item.label}`;
      const disabled = item.enabled === false;

      if (item.submenu) {
        const entries = item.submenu;
        return (
          <div
            key={key}
            className={item.separated ? "menu-row has-submenu is-separated" : "menu-row has-submenu"}
            onMouseEnter={() => setSubmenu(key)}
          >
            <button type="button" disabled={disabled} className="menu-item">
              <span>{item.label}</span>
              <span className="chev">›</span>
            </button>
            {submenu === key && (
              <div className="menu-popup menu-sub">
                {entries.length === 0 ? (
                  <div className="menu-empty">{item.emptyLabel ?? "Nothing yet"}</div>
                ) : (
                  renderItems(entries, depth + 1)
                )}
              </div>
            )}
          </div>
        );
      }

      return (
        <button
          key={key}
          type="button"
          role="menuitem"
          disabled={disabled}
          className={item.separated ? "menu-item is-separated" : "menu-item"}
          onMouseEnter={() => setSubmenu(null)}
          onClick={() => {
            if (disabled) return;
            setOpen(null);
            item.run?.();
          }}
        >
          <span className="label">{item.label}</span>
          {item.shortcut && <span className="shortcut">{item.shortcut}</span>}
        </button>
      );
    });

  return (
    <div className="menubar" ref={host} role="menubar">
      {menus.map((menu) => (
        <div className="menu" key={menu.label}>
          <button
            type="button"
            className={open === menu.label ? "menu-title is-on" : "menu-title"}
            aria-haspopup="menu"
            aria-expanded={open === menu.label}
            onClick={() => setOpen((shown) => (shown === menu.label ? null : menu.label))}
            // Once something is open, sliding across the bar switches menus,
            // the way a real menu bar behaves.
            onMouseEnter={() => setOpen((shown) => (shown ? menu.label : shown))}
          >
            {menu.label}
          </button>
          {open === menu.label && (
            <div className="menu-popup" role="menu">
              {renderItems(menu.items, 0)}
            </div>
          )}
        </div>
      ))}
    </div>
  );
}
