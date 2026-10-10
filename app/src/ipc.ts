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
export interface ClosedTab {
  id: string;
  title: string;
  path: string | null;
  language: string;
  eol: string;
  has_snapshot: boolean;
}

export interface Session {
  version: number;
  buffers: Buffer[];
  active: number;
  /** Files opened or saved recently, newest first. */
  recent: string[];
  /** Tabs that were closed, newest first. */
  closed: ClosedTab[];
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
export const closeOthers = (id: string) => invoke<Session>("close_others", { id });
export const closeToLeft = (id: string) => invoke<Session>("close_to_left", { id });
export const closeToRight = (id: string) => invoke<Session>("close_to_right", { id });
export const restoreClosed = () => invoke<Opened | null>("restore_closed");
export const forgetClosed = () => invoke<Session>("forget_closed");
export const reload = (id: string) => invoke<Opened>("reload", { id });
export const rename = (id: string, path: string) => invoke<Session>("rename", { id, path });
export const saveCopy = (id: string, path: string) => invoke<void>("save_copy", { id, path });
export const reveal = (path: string) => invoke<void>("reveal", { path });
export const moveToTrash = (id: string) => invoke<Session>("move_to_trash", { id });
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

export interface Limits {
  max_open_bytes: number;
  settings_path: string;
  auto_update_check: boolean;
  version: string;
}

export interface UpdateRelease {
  version: string;
  notes: string;
  page_url: string;
  asset_name: string | null;
  asset_url: string | null;
  asset_size: number;
}

export interface UpdateCheck {
  update: UpdateRelease | null;
  current: string;
  /** Why the check failed. NEVER treat this as "up to date". */
  error: string | null;
}

/** `manual` checks always run; automatic ones respect the setting and the
 *  24-hour throttle, and return null when they were skipped. */
export const checkUpdate = (manual: boolean) =>
  invoke<UpdateCheck | null>("check_update", { manual });
export const setAutoUpdate = (enabled: boolean) =>
  invoke<void>("set_auto_update", { enabled });
export const downloadUpdate = (release: UpdateRelease) =>
  invoke<string>("download_update", { release });
export const openReleasePage = (url: string) =>
  invoke<void>("open_release_page", { url });
export const limits = () => invoke<Limits>("limits");

/**
 * Wraps a command so it cannot hang forever.
 *
 * A file too large to cross the IPC boundary left the tab sitting on
 * "Loading…" with no error and no way out — worse than a refusal. Nothing here
 * should take a minute; if it does, say so.
 */
export function withTimeout<T>(work: Promise<T>, ms: number, what: string): Promise<T> {
  return Promise.race([
    work,
    new Promise<T>((_resolve, reject) =>
      setTimeout(() => reject(new Error(`${what} did not finish within ${ms / 1000}s`)), ms),
    ),
  ]);
}

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
