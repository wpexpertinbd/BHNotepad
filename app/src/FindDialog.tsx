// Find / Replace as a floating, draggable dialog — Notepad++'s shape rather
// than CodeMirror's docked strip.
//
// CodeMirror's own search panel is deliberately left unopened (its keymap is
// not installed in Editor.tsx); this component drives the same search engine
// through its commands, so behaviour stays correct while the presentation is
// ours.

import { useCallback, useEffect, useRef, useState } from "react";
import { EditorView } from "@codemirror/view";
import {
  SearchQuery,
  findNext,
  findPrevious,
  replaceAll,
  replaceNext,
  selectMatches,
  setSearchQuery,
} from "@codemirror/search";

export type FindMode = "find" | "replace";

interface Props {
  view: EditorView | null;
  mode: FindMode;
  onModeChange: (mode: FindMode) => void;
  onClose: () => void;
}

interface Options {
  caseSensitive: boolean;
  wholeWord: boolean;
  regexp: boolean;
}

export function FindDialog({ view, mode, onModeChange, onClose }: Props) {
  const [search, setSearch] = useState("");
  const [replace, setReplace] = useState("");
  const [options, setOptions] = useState<Options>({
    caseSensitive: false,
    wholeWord: false,
    regexp: false,
  });
  const [message, setMessage] = useState("");
  const [position, setPosition] = useState<{ x: number; y: number } | null>(null);

  const searchField = useRef<HTMLInputElement | null>(null);
  const dialog = useRef<HTMLDivElement | null>(null);
  const drag = useRef<{ dx: number; dy: number } | null>(null);

  /** Builds the query and makes it the editor's current one. */
  const applyQuery = useCallback((): SearchQuery | null => {
    if (!view || !search) return null;
    const query = new SearchQuery({
      search,
      replace,
      caseSensitive: options.caseSensitive,
      wholeWord: options.wholeWord,
      regexp: options.regexp,
      // `literal: false` would treat \n and \t in the box as escapes. Notepad++
      // keeps those for its "Extended" mode, which this dialog does not offer,
      // so the text is taken exactly as typed.
      literal: true,
    });
    view.dispatch({ effects: setSearchQuery.of(query) });
    return query;
  }, [view, search, replace, options]);

  // Keep the editor's highlighting in step with what is typed, so matches light
  // up as you go rather than only after pressing a button.
  useEffect(() => {
    if (!view || !search) return;
    const timer = window.setTimeout(() => {
      applyQuery();
    }, 120);
    return () => window.clearTimeout(timer);
  }, [view, search, options, applyQuery]);

  useEffect(() => {
    searchField.current?.focus();
    searchField.current?.select();
  }, [mode]);

  /** Counts every match, for the Count button. */
  const countMatches = useCallback((): number => {
    const query = applyQuery();
    if (!view || !query || !query.valid) return 0;
    let total = 0;
    const cursor = query.getCursor(view.state);
    while (!cursor.next().done) total += 1;
    return total;
  }, [view, applyQuery]);

  const run = useCallback(
    (command: (view: EditorView) => boolean, nothingFound: string) => {
      const query = applyQuery();
      if (!view) return;
      if (!query || !query.valid) {
        setMessage(options.regexp && search ? "That is not a valid regular expression" : "");
        return;
      }
      const moved = command(view);
      setMessage(moved ? "" : nothingFound);
      // The editor keeps the selection, but focus stays here so the next
      // Enter press searches again — the way Notepad++'s dialog behaves.
      searchField.current?.focus();
    },
    [view, applyQuery, options.regexp, search],
  );

  // Dragging by the title bar.
  useEffect(() => {
    const onMove = (event: MouseEvent) => {
      if (!drag.current) return;
      const width = dialog.current?.offsetWidth ?? 0;
      const height = dialog.current?.offsetHeight ?? 0;
      // Clamped so the dialog can never be dragged somewhere it cannot be
      // grabbed again.
      setPosition({
        x: Math.min(Math.max(0, event.clientX - drag.current.dx), window.innerWidth - width),
        y: Math.min(Math.max(0, event.clientY - drag.current.dy), window.innerHeight - height),
      });
    };
    const onUp = () => {
      drag.current = null;
    };
    window.addEventListener("mousemove", onMove);
    window.addEventListener("mouseup", onUp);
    return () => {
      window.removeEventListener("mousemove", onMove);
      window.removeEventListener("mouseup", onUp);
    };
  }, []);

  const toggle = (key: keyof Options) => () =>
    setOptions((current) => ({ ...current, [key]: !current[key] }));

  return (
    <div
      className="finddialog"
      ref={dialog}
      role="dialog"
      aria-label={mode === "replace" ? "Replace" : "Find"}
      style={position ? { left: position.x, top: position.y, right: "auto" } : undefined}
      onKeyDown={(event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose();
        }
      }}
    >
      <div
        className="finddialog-bar"
        onMouseDown={(event) => {
          const box = dialog.current?.getBoundingClientRect();
          if (!box) return;
          drag.current = { dx: event.clientX - box.left, dy: event.clientY - box.top };
          setPosition({ x: box.left, y: box.top });
        }}
      >
        <div className="finddialog-tabs">
          <button
            type="button"
            className={mode === "find" ? "is-on" : ""}
            onClick={() => onModeChange("find")}
          >
            Find
          </button>
          <button
            type="button"
            className={mode === "replace" ? "is-on" : ""}
            onClick={() => onModeChange("replace")}
          >
            Replace
          </button>
        </div>
        <button type="button" className="finddialog-close" aria-label="Close" onClick={onClose}>
          ×
        </button>
      </div>

      <div className="finddialog-body">
        <label className="row">
          <span>Find what</span>
          <input
            ref={searchField}
            type="text"
            value={search}
            spellCheck={false}
            autoComplete="off"
            onChange={(event) => {
              setSearch(event.target.value);
              setMessage("");
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                run(event.shiftKey ? findPrevious : findNext, "No matches");
              }
            }}
          />
        </label>

        {mode === "replace" && (
          <label className="row">
            <span>Replace with</span>
            <input
              type="text"
              value={replace}
              spellCheck={false}
              autoComplete="off"
              onChange={(event) => setReplace(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  run(replaceNext, "No matches");
                }
              }}
            />
          </label>
        )}

        <div className="finddialog-options">
          <label>
            <input
              type="checkbox"
              checked={options.caseSensitive}
              onChange={toggle("caseSensitive")}
            />
            Match case
          </label>
          <label>
            <input type="checkbox" checked={options.wholeWord} onChange={toggle("wholeWord")} />
            Whole word only
          </label>
          <label>
            <input type="checkbox" checked={options.regexp} onChange={toggle("regexp")} />
            Regular expression
          </label>
        </div>

        <div className="finddialog-actions">
          <button
            type="button"
            className="primary"
            onClick={() => run(findNext, "No matches")}
            disabled={!search}
          >
            Find Next
          </button>
          <button
            type="button"
            onClick={() => run(findPrevious, "No matches")}
            disabled={!search}
          >
            Find Previous
          </button>
          <button
            type="button"
            onClick={() => {
              const total = countMatches();
              setMessage(total === 1 ? "1 match" : `${total} matches`);
            }}
            disabled={!search}
          >
            Count
          </button>
          <button
            type="button"
            onClick={() => run(selectMatches, "No matches")}
            disabled={!search}
          >
            Select All
          </button>
          {mode === "replace" && (
            <>
              <button type="button" onClick={() => run(replaceNext, "No matches")} disabled={!search}>
                Replace
              </button>
              <button
                type="button"
                onClick={() => {
                  const total = countMatches();
                  run(replaceAll, "No matches");
                  setMessage(total === 1 ? "Replaced 1 match" : `Replaced ${total} matches`);
                }}
                disabled={!search}
              >
                Replace All
              </button>
            </>
          )}
        </div>

        {message && <div className="finddialog-message">{message}</div>}
      </div>
    </div>
  );
}
