// Window orchestration: tabs, the editor, the status bar and the shortcuts.
//
// ⚠️ There is deliberately no `beforeunload` handler and no "unsaved changes"
// confirmation anywhere in this file. Those are the webview's version of the
// save nag this app exists to avoid. Text reaches disk as it is typed, so
// closing a tab or quitting is never a question. See CLAUDE.md.

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  open as openDialog,
  save as saveDialog,
  confirm as confirmDialog,
} from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

import * as ipc from "./ipc";
import type { Buffer, Session } from "./ipc";
import { Editor } from "./Editor";
import { TabBar } from "./TabBar";
import { MenuBar, ACCEL, ALT, SHIFT, type Menu } from "./MenuBar";
import * as cmd from "./editorCommands";
import { FindDialog, type FindMode } from "./FindDialog";
import { SELECTABLE, labelFor } from "./languages";
import type { EditorView } from "@codemirror/view";

/** Only used for naming the Trash in a message. */
const ON_WINDOWS = navigator.platform.toLowerCase().startsWith("win");

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
  /** Set by the Editor; writes the live text through, bypassing the debounce. */
  const flushEditor = useRef<(() => Promise<void>) | null>(null);
  /** Set by the Editor; drops a tab's parked document, used by Reload. */
  const forgetState = useRef<((id: string) => void) | null>(null);

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
        // Bounded: a document too large to cross the IPC boundary used to
        // leave this tab on "Loading…" indefinitely.
        const body = await ipc.withTimeout(
          ipc.bufferText(id),
          60_000,
          "Loading that file",
        );
        setTexts((all) => (all[id] !== undefined ? all : { ...all, [id]: body }));
        // An empty result is a legitimate answer; marking the tab loaded is
        // what takes it off the loading state either way.
      } catch (error) {
        fail("Could not read that tab", error);
        // Give the tab SOMETHING so it stops saying "Loading…" forever. The
        // status bar carries the reason.
        setTexts((all) => (all[id] !== undefined ? all : { ...all, [id]: "" }));
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

  // Files opened from outside the window — double-clicked in Finder or
  // Explorer, dropped on the icon, or passed to a second launch that handed
  // them to this one. The Rust side has already opened them; this just picks
  // up the new session.
  useEffect(() => {
    // A file that could not be opened — too large, unreadable, gone. The tab
    // never appears, so the status bar is the only place this can surface.
    const failures = listen<string>("bhnotepad://open-failed", (event) => {
      setStatus(String(event.payload));
    });
    const pending = listen("bhnotepad://session-changed", () => {
      void (async () => {
        try {
          const restored = await ipc.sessionState();
          setSession(restored.session);
        } catch (error) {
          fail("Could not open that file", error);
        }
      })();
    });
    return () => {
      void pending.then((unlisten) => unlisten());
      void failures.then((unlisten) => unlisten());
    };
  }, [fail]);

  // Restore the zoom level this machine was last left at.
  useEffect(() => {
    cmd.applyFontSize(cmd.currentFontSize());
  }, []);

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
        // The core writes what is in the STORE. Without this, typing and then
        // pressing Save inside the 300ms debounce wrote the PREVIOUS text to
        // the file and cheerfully reported success.
        await flushEditor.current?.();
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

  /// Closes every tab after one confirmation.
  ///
  /// This is the only confirmation in the app, and it is here because the
  /// action is bulk and irreversible: scratch text that was never saved to a
  /// file is gone afterwards. Closing one tab, or quitting, still asks nothing.
  const handleCloseAll = useCallback(async () => {
    if (!session) return;
    const count = session.buffers.length;
    // A named file with unsaved EDITS loses those edits just as surely as an
    // untitled tab loses all of its text. Counting only path-less tabs told
    // the user "nothing will be lost" and then discarded their work.
    const untitled = session.buffers.filter((b) => !b.path).length;
    const editedFiles = session.buffers.filter((b) => b.path && b.dirty).length;
    const atRisk = untitled + editedFiles;

    const parts: string[] = [];
    if (untitled > 0) {
      parts.push(
        `${untitled} ${untitled === 1 ? "tab has" : "tabs have"} never been saved to a file`,
      );
    }
    if (editedFiles > 0) {
      parts.push(
        `${editedFiles} saved ${editedFiles === 1 ? "file has" : "files have"} unsaved changes`,
      );
    }
    const detail =
      atRisk > 0
        ? `${parts.join(", and ")}. That text will be discarded.`
        : "Every tab is saved, so nothing will be lost.";

    try {
      const go = await confirmDialog(
        `Close all ${count} ${count === 1 ? "tab" : "tabs"}?\n\n${detail}`,
        { title: "Close All", kind: atRisk > 0 ? "warning" : "info", okLabel: "Close All" },
      );
      if (!go) return;

      const updated = await ipc.closeAll();
      setSession(updated);
      setTexts({});
      note(`Closed ${count} ${count === 1 ? "tab" : "tabs"}`);
    } catch (error) {
      fail("Could not close the tabs", error);
    }
  }, [session, fail, note]);

  /// Saves every tab, asking where each unsaved one should go, one at a time.
  ///
  /// Cancelling a single tab's sheet skips that tab and moves to the next
  /// rather than abandoning the whole run — having saved four of six should
  /// not be undone by changing your mind about the fifth.
  const handleSaveAll = useCallback(async () => {
    if (!session) return;
    try {
      await flushEditor.current?.();
      let saved = 0;
      let skipped = 0;
      let latest: Session | null = null;

      // Re-read after the flush: the tab just flushed may have become dirty,
      // and this loop decides what to write from exactly that flag.
      const current = await ipc.sessionState();
      for (const buffer of current.session.buffers) {
        let target: string | undefined;
        if (!buffer.path) {
          const picked = await saveDialog({ defaultPath: buffer.title });
          if (!picked) {
            skipped += 1;
            continue;
          }
          target = picked;
        } else if (!buffer.dirty) {
          continue;
        }
        latest = await ipc.saveBuffer(buffer.id, target);
        saved += 1;
      }

      if (latest) setSession(latest);
      note(
        saved === 0
          ? skipped > 0
            ? "Nothing saved"
            : "Everything was already saved"
          : `Saved ${saved}${skipped > 0 ? `, skipped ${skipped}` : ""}`,
      );
    } catch (error) {
      fail("Could not save everything", error);
    }
  }, [session, fail, note]);

  const handleRestoreClosed = useCallback(async () => {
    try {
      const opened = await ipc.restoreClosed();
      if (!opened) {
        note("No recently closed tab to restore");
        return;
      }
      setTexts((all) => ({ ...all, [opened.id]: opened.text }));
      setSession(opened.session);
    } catch (error) {
      fail("Could not restore that tab", error);
    }
  }, [fail, note]);

  const handleReload = useCallback(async () => {
    if (!active?.path) return;
    try {
      const opened = await ipc.reload(active.id);
      setTexts((all) => ({ ...all, [opened.id]: opened.text }));
      setSession(opened.session);
      // The parked EditorState holds the old text, so it has to go or the
      // reloaded file would not actually appear.
      forgetState.current?.(opened.id);
      note(`Reloaded ${active.title}`);
    } catch (error) {
      fail("Could not reload", error);
    }
  }, [active, fail, note]);

  const handleRename = useCallback(async () => {
    if (!active?.path) return;
    try {
      const picked = await saveDialog({ defaultPath: active.path, title: "Rename" });
      if (!picked) return;
      setSession(await ipc.rename(active.id, picked));
      note("Renamed");
    } catch (error) {
      fail("Could not rename", error);
    }
  }, [active, fail, note]);

  const handleSaveCopy = useCallback(async () => {
    if (!active) return;
    try {
      await flushEditor.current?.();
      const picked = await saveDialog({ defaultPath: active.path ?? active.title });
      if (!picked) return;
      await ipc.saveCopy(active.id, picked);
      note("Copy saved");
    } catch (error) {
      fail("Could not save a copy", error);
    }
  }, [active, fail, note]);

  const handleReveal = useCallback(async () => {
    if (!active?.path) return;
    try {
      await ipc.reveal(active.path);
    } catch (error) {
      fail("Could not show it in the folder", error);
    }
  }, [active, fail]);

  const handleTrash = useCallback(async () => {
    if (!active?.path) return;
    try {
      const go = await confirmDialog(
        `Move ${active.title} to the ${ON_WINDOWS ? "Recycle Bin" : "Trash"}?`,
        { title: "Move to Trash", kind: "warning", okLabel: "Move" },
      );
      if (!go) return;
      const updated = await ipc.moveToTrash(active.id);
      setSession(updated);
      setTexts((all) => {
        const next = { ...all };
        delete next[active.id];
        return next;
      });
      note(`Moved ${active.title} to the ${ON_WINDOWS ? "Recycle Bin" : "Trash"}`);
    } catch (error) {
      fail("Could not move it", error);
    }
  }, [active, fail, note]);

  const handleBulkClose = useCallback(
    async (which: "others" | "left" | "right") => {
      if (!active) return;
      try {
        const updated =
          which === "others"
            ? await ipc.closeOthers(active.id)
            : which === "left"
              ? await ipc.closeToLeft(active.id)
              : await ipc.closeToRight(active.id);
        setSession(updated);
      } catch (error) {
        fail("Could not close those tabs", error);
      }
    },
    [active, fail],
  );

  const handleOpenRecent = useCallback(
    async (path: string) => {
      try {
        const opened = await ipc.openPath(path);
        setTexts((all) => ({ ...all, [opened.id]: opened.text }));
        setSession(opened.session);
      } catch (error) {
        fail("Could not open that file", error);
      }
    },
    [fail],
  );

  const goToLine = useCallback(() => {
    if (view) cmd.editorGoToLine(view);
  }, [view]);

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
    async (id: string, body: string) => {
      setTexts((all) => ({ ...all, [id]: body }));
      // Mark dirty on EVERY edit. Doing it only on the first one left a tab
      // that had been saved and then edited again looking clean, so Save All
      // skipped it and closing it discarded the change.
      setSession((current) =>
        current
          ? {
              ...current,
              buffers: current.buffers.map((b) =>
                b.id === id && !b.dirty ? { ...b, dirty: true } : b,
              ),
            }
          : current,
      );
      try {
        await ipc.updateText(id, body);
      } catch (error) {
        fail("Could not save your typing", error);
      }
    },
    [fail],
  );

  const handlePositionChanged = useCallback((id: string, head: number, scrollTop: number) => {
    void ipc.updatePosition(id, head, scrollTop).catch(() => {
      // Losing a caret position is not worth interrupting anyone over.
    });
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
    handleCloseAll,
    handleSaveAll,
    handleRestoreClosed,
    handleReload,
    goToLine,
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
    handleCloseAll,
    handleSaveAll,
    handleRestoreClosed,
    handleReload,
    goToLine,
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
          if (event.shiftKey) {
            void current.handleRestoreClosed();
          } else {
            void current.handleNew();
          }
          return;
        case "r":
          event.preventDefault();
          void current.handleReload();
          return;
        case "g":
          event.preventDefault();
          current.goToLine();
          return;
        case "+":
        case "=":
          event.preventDefault();
          cmd.zoomIn();
          return;
        case "-":
          event.preventDefault();
          cmd.zoomOut();
          return;
        case "0":
          event.preventDefault();
          cmd.zoomReset();
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
          // Matches Notepad++: Alt is Save As, Shift is Save All. These were
          // the other way round and that is daily muscle-memory friction.
          event.preventDefault();
          if (event.shiftKey) {
            void current.handleSaveAll();
          } else {
            void current.handleSave(event.altKey);
          }
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
          if (event.shiftKey) {
            void current.handleCloseAll();
          } else if (current.active) {
            void current.handleClose(current.active.id);
          }
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
      // Deliberately NOT `beforeunload` — that is the webview's version of the
      // save nag. `pagehide` and blur let us write without ever showing one.
      void flushEditor.current?.().catch(() => {});
      void ipc.flush().catch(() => {});
    };
    window.addEventListener("blur", onHide);
    window.addEventListener("pagehide", onHide);
    document.addEventListener("visibilitychange", onHide);
    return () => {
      window.removeEventListener("blur", onHide);
      window.removeEventListener("pagehide", onHide);
      document.removeEventListener("visibilitychange", onHide);
    };
  }, []);

  // ── Render ────────────────────────────────────────────────────────────────

  if (!session || !active) {
    return <div className="loading">Opening your tabs…</div>;
  }

  // ── Menus ─────────────────────────────────────────────────────────────
  //
  // Shortcuts match Notepad++ where it has an opinion: Save As is accel+Alt+S
  // and Save All is accel+Shift+S, which is the opposite of what this app
  // shipped with. Twenty years of muscle memory beats my first guess.
  const hasFile = Boolean(active?.path);
  const withView = (run: (view: EditorView) => void) => () => {
    if (view) run(view);
  };
  const withViewAsync = (run: (view: EditorView) => Promise<string | null>) => () => {
    if (!view) return;
    void run(view).then((problem) => problem && note(problem));
  };

  const menus: Menu[] = [
    {
      label: "File",
      items: [
        { label: "New tab", shortcut: `${ACCEL}T`, run: () => void handleNew() },
        { label: "Open…", shortcut: `${ACCEL}O`, run: () => void handleOpen() },
        {
          label: "Open Recent",
          submenu: (session?.recent ?? []).map((path) => ({
            label: path,
            run: () => void handleOpenRecent(path),
          })),
          emptyLabel: "No recent files",
        },
        {
          label: "Restore Recent Closed File",
          shortcut: `${ACCEL}${SHIFT}T`,
          separated: true,
          enabled: (session?.closed.length ?? 0) > 0,
          run: () => void handleRestoreClosed(),
        },
        {
          label: "Reload from Disk",
          shortcut: `${ACCEL}R`,
          enabled: hasFile,
          run: () => void handleReload(),
        },
        { label: "Save", shortcut: `${ACCEL}S`, separated: true, run: () => void handleSave(false) },
        { label: "Save As…", shortcut: `${ACCEL}${ALT}S`, run: () => void handleSave(true) },
        { label: "Save a Copy As…", run: () => void handleSaveCopy() },
        { label: "Save All…", shortcut: `${ACCEL}${SHIFT}S`, run: () => void handleSaveAll() },
        { label: "Rename…", separated: true, enabled: hasFile, run: () => void handleRename() },
        {
          label: ON_WINDOWS ? "Show in Explorer" : "Show in Finder",
          enabled: hasFile,
          run: () => void handleReveal(),
        },
        {
          label: ON_WINDOWS ? "Move to Recycle Bin" : "Move to Trash",
          enabled: hasFile,
          run: () => void handleTrash(),
        },
        {
          label: "Close tab",
          shortcut: `${ACCEL}W`,
          separated: true,
          run: () => active && void handleClose(active.id),
        },
        {
          label: "Close Multiple Documents",
          submenu: [
            { label: "Close All but This", run: () => void handleBulkClose("others") },
            { label: "Close to the Left", run: () => void handleBulkClose("left") },
            { label: "Close to the Right", run: () => void handleBulkClose("right") },
          ],
        },
        { label: "Close All…", shortcut: `${ACCEL}${SHIFT}W`, run: () => void handleCloseAll() },
      ],
    },
    {
      label: "Edit",
      items: [
        { label: "Undo", shortcut: `${ACCEL}Z`, run: withView(cmd.editorUndo) },
        { label: "Redo", shortcut: `${ACCEL}${SHIFT}Z`, run: withView(cmd.editorRedo) },
        { label: "Cut", shortcut: `${ACCEL}X`, separated: true, run: withViewAsync(cmd.editorCut) },
        { label: "Copy", shortcut: `${ACCEL}C`, run: withViewAsync(cmd.editorCopy) },
        { label: "Paste", shortcut: `${ACCEL}V`, run: withViewAsync(cmd.editorPaste) },
        { label: "Select All", shortcut: `${ACCEL}A`, run: withView(cmd.editorSelectAll) },
        {
          label: "Duplicate Line",
          shortcut: `${ACCEL}D`,
          separated: true,
          run: withView(cmd.editorDuplicateLine),
        },
        { label: "Delete Line", shortcut: `${ACCEL}${SHIFT}L`, run: withView(cmd.editorDeleteLine) },
        { label: "Move Line Up", shortcut: `${ALT}↑`, run: withView(cmd.editorMoveLineUp) },
        { label: "Move Line Down", shortcut: `${ALT}↓`, run: withView(cmd.editorMoveLineDown) },
        { label: "Indent", shortcut: "Tab", separated: true, run: withView(cmd.editorIndent) },
        { label: "Outdent", shortcut: `${SHIFT}Tab`, run: withView(cmd.editorOutdent) },
        {
          label: "Toggle Comment",
          shortcut: `${ACCEL}/`,
          run: withView(cmd.editorToggleComment),
        },
        { label: "UPPERCASE", separated: true, run: withView(cmd.editorUpperCase) },
        { label: "lowercase", run: withView(cmd.editorLowerCase) },
        { label: "Trim Trailing Space", run: withView(cmd.editorTrimTrailing) },
      ],
    },
    {
      label: "Search",
      items: [
        { label: "Find…", shortcut: `${ACCEL}F`, run: () => setFind("find") },
        { label: "Replace…", shortcut: `${ACCEL}H`, run: () => setFind("replace") },
        {
          label: "Go to Line…",
          shortcut: `${ACCEL}G`,
          separated: true,
          run: withView(cmd.editorGoToLine),
        },
      ],
    },
    {
      label: "View",
      items: [
        { label: "Zoom In", shortcut: `${ACCEL}+`, run: () => cmd.zoomIn() },
        { label: "Zoom Out", shortcut: `${ACCEL}-`, run: () => cmd.zoomOut() },
        { label: "Reset Zoom", shortcut: `${ACCEL}0`, run: () => cmd.zoomReset() },
        {
          label: active?.word_wrap ? "Word Wrap ✓" : "Word Wrap",
          shortcut: `${ACCEL}${SHIFT}Z`,
          separated: true,
          run: toggleWrap,
        },
        {
          label: "Forget Closed Tabs",
          separated: true,
          enabled: (session?.closed.length ?? 0) > 0,
          run: () => {
            void ipc
              .forgetClosed()
              .then((updated) => {
                setSession(updated);
                note("Closed-tab history cleared");
              })
              .catch((error) => fail("Could not clear the history", error));
          },
        },
      ],
    },
  ];



  const body = texts[active.id];

  return (
    <div className="app">
      <MenuBar menus={menus} />
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
          registerFlush={(fn) => (flushEditor.current = fn)}
          registerForget={(fn) => (forgetState.current = fn)}
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
        <span
          className={active.lossy ? "where is-warning" : "where"}
          title={
            active.lossy
              ? `${active.path} is not UTF-8; parts could not be read, so saving over it is refused — use Save As`
              : (active.path ?? "Not saved to a file yet")
          }
        >
          {active.lossy
            ? `${active.title} — not UTF-8, use Save As`
            : (active.path ?? "Not saved to a file — kept in your session")}
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
        <span className="metric">{active.eol === "crlf" ? "CRLF" : "LF"}</span>
        <span className={active.lossy ? "metric is-warning" : "metric"}>
          {active.lossy ? "UTF-8 (lossy)" : "UTF-8"}
        </span>
      </div>
    </div>
  );
}
