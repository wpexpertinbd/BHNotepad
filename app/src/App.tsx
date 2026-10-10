// Window orchestration: tabs, the editor, the status bar and the shortcuts.
//
// ⚠️ There is deliberately no `beforeunload` handler and no "unsaved changes"
// confirmation anywhere in this file. Those are the webview's version of the
// save nag this app exists to avoid. Text reaches disk as it is typed, so
// closing a tab or quitting is never a question. See CLAUDE.md.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { open as openDialog, save as saveDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";

import * as ipc from "./ipc";
import type { Buffer, Session } from "./ipc";
import { Editor } from "./Editor";
import { TabBar } from "./TabBar";
import { FindDialog, type FindMode } from "./FindDialog";
import { SELECTABLE, labelFor } from "./languages";
import type { EditorView } from "@codemirror/view";

interface CaretInfo {
  line: number;
  col: number;
  selected: number;
}

export function App() {
  const [session, setSession] = useState<Session | null>(null);
  const [texts, setTexts] = useState<Record<string, string>>({});
  const [status, setStatus] = useState<string>("");
  const [caret, setCaret] = useState<CaretInfo>({ line: 1, col: 1, selected: 0 });
  const [find, setFind] = useState<FindMode | null>(null);
  const [view, setView] = useState<EditorView | null>(null);

  const active: Buffer | null = useMemo(() => {
    if (!session || session.buffers.length === 0) return null;
    return session.buffers[session.active] ?? session.buffers[0] ?? null;
  }, [session]);

  const openIds = useMemo(() => session?.buffers.map((b) => b.id) ?? [], [session]);

  const note = useCallback((message: string) => {
    setStatus(message);
    window.setTimeout(() => setStatus((shown) => (shown === message ? "" : shown)), 4000);
  }, []);

  const fail = useCallback(
    (what: string, error: unknown) => {
      const detail = error instanceof Error ? error.message : String(error);
      // Shown rather than swallowed: a failure to read or write is exactly
      // what the user needs to know about immediately.
      setStatus(`${what}: ${detail}`);
      console.error(what, error);
    },
    [],
  );

  /** Loads a tab's text if it is not already in hand. */
  const ensureText = useCallback(
    async (id: string) => {
      if (texts[id] !== undefined) return;
      try {
        const body = await ipc.bufferText(id);
        setTexts((all) => (all[id] !== undefined ? all : { ...all, [id]: body }));
      } catch (error) {
        fail("Could not read that tab", error);
      }
    },
    [texts, fail],
  );

  // Restore the previous session.
  useEffect(() => {
    void (async () => {
      try {
        const restored = await ipc.sessionState();
        setSession(restored.session);
        if (restored.note) {
          // Sticky, not a timed toast: if tabs did not come back, that is the
          // one thing about this app the user must not miss.
          setStatus(restored.note);
        }
        const { buffers, active: index } = restored.session;
        const current = buffers[index] ?? buffers[0];
        if (current) {
          const body = await ipc.bufferText(current.id);
          setTexts({ [current.id]: body });
        }
      } catch (error) {
        fail("Could not restore your tabs", error);
      } finally {
        // The window is created hidden so the first paint is the restored
        // session rather than a white rectangle. Showing it here, in a
        // `finally`, means a failure to restore still produces a usable
        // window instead of an app with no UI at all.
        try {
          await getCurrentWindow().show();
          await getCurrentWindow().setFocus();
        } catch (error) {
          console.error("BHNotepad: could not show the window", error);
        }
      }
    })();
  }, [fail]);

  useEffect(() => {
    if (active) void ensureText(active.id);
  }, [active, ensureText]);

  // Keep the window title showing the active tab, as Notepad++ does.
  useEffect(() => {
    document.title = active ? `${active.title} — BHNotepad` : "BHNotepad";
  }, [active]);

  // ── Actions ───────────────────────────────────────────────────────────────

  const handleNew = useCallback(async () => {
    try {
      const opened = await ipc.newTab();
      setTexts((all) => ({ ...all, [opened.id]: opened.text }));
      setSession(opened.session);
    } catch (error) {
      fail("Could not open a new tab", error);
    }
  }, [fail]);

  const handleOpen = useCallback(async () => {
    try {
      const picked = await openDialog({ multiple: true });
      if (!picked) return;
      const paths = Array.isArray(picked) ? picked : [picked];
      let latest: Session | null = null;
      for (const path of paths) {
        const opened = await ipc.openPath(path);
        setTexts((all) => ({ ...all, [opened.id]: opened.text }));
        latest = opened.session;
      }
      if (latest) setSession(latest);
    } catch (error) {
      fail("Could not open that file", error);
    }
  }, [fail]);

  const handleSave = useCallback(
    async (asNew: boolean) => {
      if (!active) return;
      try {
        let target: string | undefined;
        if (asNew || !active.path) {
          const picked = await saveDialog({
            defaultPath: active.path ?? active.title,
          });
          if (!picked) return;
          target = picked;
        }
        const updated = await ipc.saveBuffer(active.id, target);
        setSession(updated);
        note(`Saved ${updated.buffers.find((b) => b.id === active.id)?.title ?? ""}`);
      } catch (error) {
        fail("Could not save", error);
      }
    },
    [active, fail, note],
  );

  const handleClose = useCallback(
    async (id: string) => {
      try {
        const updated = await ipc.closeTab(id);
        setSession(updated);
        setTexts((all) => {
          const next = { ...all };
          delete next[id];
          return next;
        });
      } catch (error) {
        fail("Could not close that tab", error);
      }
    },
    [fail],
  );

  const handleSelect = useCallback(
    (index: number) => {
      setSession((current) => (current ? { ...current, active: index } : current));
      void ipc.setActive(index).catch((error) => fail("Could not switch tabs", error));
    },
    [fail],
  );

  const handleReorder = useCallback(
    async (from: number, to: number) => {
      try {
        setSession(await ipc.reorder(from, to));
      } catch (error) {
        fail("Could not move that tab", error);
      }
    },
    [fail],
  );

  const handleTextChanged = useCallback(
    (id: string, body: string) => {
      setTexts((all) => ({ ...all, [id]: body }));
      void ipc.updateText(id, body).catch((error) => fail("Could not save your typing", error));
    },
    [fail],
  );

  const handlePositionChanged = useCallback((id: string, head: number, scrollTop: number) => {
    void ipc.updatePosition(id, head, scrollTop).catch(() => {
      // Losing a caret position is not worth interrupting anyone over.
    });
  }, []);

  const handleFirstEdit = useCallback((id: string) => {
    setSession((current) =>
      current
        ? {
            ...current,
            buffers: current.buffers.map((b) => (b.id === id ? { ...b, dirty: true } : b)),
          }
        : current,
    );
  }, []);

  const toggleWrap = useCallback(() => {
    if (!active) return;
    const wrap = !active.word_wrap;
    setSession((current) =>
      current
        ? {
            ...current,
            buffers: current.buffers.map((b) =>
              b.id === active.id ? { ...b, word_wrap: wrap } : b,
            ),
          }
        : current,
    );
    void ipc.setWordWrap(active.id, wrap).catch((error) => fail("Could not set wrapping", error));
  }, [active, fail]);

  const changeLanguage = useCallback(
    (language: string) => {
      if (!active) return;
      setSession((current) =>
        current
          ? {
              ...current,
              buffers: current.buffers.map((b) =>
                b.id === active.id ? { ...b, language } : b,
              ),
            }
          : current,
      );
      void ipc.setLanguage(active.id, language).catch((error) =>
        fail("Could not set the language", error),
      );
    },
    [active, fail],
  );

  const cycleTab = useCallback(
    (step: number) => {
      if (!session || session.buffers.length < 2) return;
      const count = session.buffers.length;
      handleSelect((session.active + step + count) % count);
    },
    [session, handleSelect],
  );

  // ── Shortcuts ─────────────────────────────────────────────────────────────

  // Kept in a ref so the listener is installed once and never sees stale
  // handlers; re-binding on every render would drop keystrokes mid-press.
  const actions = useRef({
    handleNew,
    handleOpen,
    handleSave,
    handleClose,
    cycleTab,
    toggleWrap,
    active,
    setFind,
  });
  actions.current = {
    handleNew,
    handleOpen,
    handleSave,
    handleClose,
    cycleTab,
    toggleWrap,
    active,
    setFind,
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const accel = event.metaKey || event.ctrlKey;
      if (!accel) return;
      const key = event.key.toLowerCase();
      const current = actions.current;

      switch (key) {
        case "t":
          event.preventDefault();
          void current.handleNew();
          return;
        case "n":
          // Notepad++ uses Ctrl+N for a new document; honour both.
          event.preventDefault();
          void current.handleNew();
          return;
        case "o":
          event.preventDefault();
          void current.handleOpen();
          return;
        case "s":
          event.preventDefault();
          void current.handleSave(event.shiftKey);
          return;
        case "f":
          // Opens our floating dialog. CodeMirror's own panel is never
          // installed, so nothing else answers this.
          event.preventDefault();
          current.setFind("find");
          return;
        case "h":
          event.preventDefault();
          current.setFind("replace");
          return;
        case "w":
          // Closes the tab, not the window. Without preventDefault the
          // webview or the app menu would take this first.
          event.preventDefault();
          if (current.active) void current.handleClose(current.active.id);
          return;
        case "z":
          // Wrapping is a view setting in Notepad++ (Ctrl+Alt+Z there, but
          // Alt is awkward in a webview, so plain accel+Shift+Z is used).
          if (event.shiftKey) {
            event.preventDefault();
            current.toggleWrap();
          }
          return;
        case "pageup":
          event.preventDefault();
          current.cycleTab(-1);
          return;
        case "pagedown":
          event.preventDefault();
          current.cycleTab(1);
          return;
        default:
          return;
      }
    };

    // Capture, so the editor's own keymap does not consume these first.
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  // Ctrl+Tab is not an accel combination, so it is handled separately.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Tab" && event.ctrlKey) {
        event.preventDefault();
        actions.current.cycleTab(event.shiftKey ? -1 : 1);
      }
    };
    window.addEventListener("keydown", onKeyDown, true);
    return () => window.removeEventListener("keydown", onKeyDown, true);
  }, []);

  // Flush caret positions when the window loses focus or is hidden, which is
  // the last reliable moment before a quit.
  useEffect(() => {
    const onHide = () => {
      void ipc.flush().catch(() => {});
    };
    window.addEventListener("blur", onHide);
    document.addEventListener("visibilitychange", onHide);
    return () => {
      window.removeEventListener("blur", onHide);
      document.removeEventListener("visibilitychange", onHide);
    };
  }, []);

  // ── Render ────────────────────────────────────────────────────────────────

  if (!session || !active) {
    return <div className="loading">Opening your tabs…</div>;
  }

  const body = texts[active.id];

  return (
    <div className="app">
      <TabBar
        buffers={session.buffers}
        active={session.active}
        onSelect={handleSelect}
        onClose={(id) => void handleClose(id)}
        onNew={() => void handleNew()}
        onReorder={(from, to) => void handleReorder(from, to)}
      />

      {body === undefined ? (
        <div className="loading">Loading…</div>
      ) : (
        <Editor
          buffer={active}
          text={body}
          openIds={openIds}
          onTextChanged={handleTextChanged}
          onPositionChanged={handlePositionChanged}
          onFirstEdit={handleFirstEdit}
          onCaretInfo={setCaret}
          onViewReady={setView}
        />
      )}

      {find && (
        <FindDialog
          view={view}
          mode={find}
          onModeChange={setFind}
          onClose={() => {
            setFind(null);
            view?.focus();
          }}
        />
      )}

      <div className="statusbar">
        <span className="where" title={active.path ?? "Not saved to a file yet"}>
          {active.path ?? "Not saved to a file — kept in your session"}
        </span>
        <span className="spacer" />
        {status && <span className="note">{status}</span>}
        <span className="metric">
          Ln {caret.line}, Col {caret.col}
          {caret.selected > 0 && ` · ${caret.selected} selected`}
        </span>
        <button
          type="button"
          className={active.word_wrap ? "toggle is-on" : "toggle"}
          onClick={toggleWrap}
          title="Word wrap (Cmd/Ctrl+Shift+Z)"
        >
          Wrap
        </button>
        <select
          className="language"
          value={SELECTABLE.some((l) => l.id === active.language) ? active.language : "plain"}
          onChange={(event) => changeLanguage(event.target.value)}
          title={`Syntax: ${labelFor(active.language)}`}
        >
          {SELECTABLE.map((language) => (
            <option key={language.id} value={language.id}>
              {language.label}
            </option>
          ))}
        </select>
        <span className="metric">UTF-8</span>
      </div>
    </div>
  );
}
