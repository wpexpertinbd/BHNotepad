// Typed wrappers over the Rust commands in `app/src-tauri/src/lib.rs`.
//
// Every mutating command returns the whole session, so the UI never has to
// guess what changed and can simply re-render from the truth the core holds.

import { invoke } from "@tauri-apps/api/core";

/** One open tab. Mirrors `bhn_core::session::Buffer`. */
export interface Buffer {
  id: string;
  title: string;
  path: string | null;
  language: string;
  dirty: boolean;
  caret: number;
  scroll_top: number;
  word_wrap: boolean;
  /** `"crlf"` or `"lf"` — the line ending the file arrived with. */
  eol: string;
  /** The file was not valid UTF-8 and characters were replaced, so saving
   *  over the original is refused. */
  lossy: boolean;
}

/** Mirrors `bhn_core::session::Session`. */
export interface Session {
  version: number;
  buffers: Buffer[];
  active: number;
}

export interface Opened {
  session: Session;
  id: string;
  text: string;
}

export interface Restored {
  session: Session;
  /** Why the previous session could not be fully restored, if it could not.
   *  Sent on the first call only. */
  note: string | null;
}

export const sessionState = () => invoke<Restored>("session_state");
export const bufferText = (id: string) => invoke<string>("buffer_text", { id });
export const newTab = () => invoke<Opened>("new_tab");
export const openPath = (path: string) => invoke<Opened>("open_path", { path });
export const closeTab = (id: string) => invoke<Session>("close_tab", { id });
export const closeAll = () => invoke<Session>("close_all");
export const unsavedTabs = () => invoke<Buffer[]>("unsaved_tabs");
export const saveBuffer = (id: string, path?: string) =>
  invoke<Session>("save", { id, path: path ?? null });
export const setActive = (index: number) => invoke<void>("set_active", { index });
export const reorder = (from: number, to: number) =>
  invoke<Session>("reorder", { from, to });
export const setWordWrap = (id: string, wrap: boolean) =>
  invoke<void>("set_word_wrap", { id, wrap });
export const setLanguage = (id: string, language: string) =>
  invoke<void>("set_language", { id, language });
export const detectLanguage = (path: string) => invoke<string>("detect_language", { path });
export const flush = () => invoke<void>("flush");

/**
 * Writes a tab's text through to disk.
 *
 * This is what makes quitting safe, so it must be called as the user types
 * (debounced), never deferred to a close or quit handler. See CLAUDE.md.
 */
export const updateText = (id: string, text: string) =>
  invoke<void>("update_text", { id, text });

export const updatePosition = (id: string, caret: number, scrollTop: number) =>
  invoke<void>("update_position", { id, caret, scroll_top: scrollTop });
