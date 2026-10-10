// The tab strip.
//
// A tab's close button never asks anything — closing is immediate and the text
// is already on disk. The dot on a tab means "this text lives in the session,
// not in a file", which for a scratch note is the normal resting state, not a
// warning. See CLAUDE.md.

import { useRef, useState } from "react";

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

  return (
    <div className="tabbar" role="tablist">
      {leading}
      <div className="tabs">
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
              const from = dragFrom.current;
              dragFrom.current = null;
              setDragOver(null);
              if (from !== null && from !== index) onReorder(from, index);
            }}
            onDragEnd={() => {
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
      <button type="button" className="newtab" onClick={onNew} title="New tab (Cmd/Ctrl+T)">
        +
      </button>
    </div>
  );
}
