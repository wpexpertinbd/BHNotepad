# BHNotepad — Notepad++-style editor for macOS, Windows & Linux

**Project dir:** `/Users/benjamin/BHServe/www/notepad`
**Started:** 2026-10-10 · **For:** Benjamin (personal need first, MIT open-source release intended)
**Name chosen by Claude** — `BHNotepad`, id `com.biswashost.bhnotepad`. Say the word to rename.

---

## ⭐⭐ THE REASON THIS PROJECT EXISTS — do not regress it

Benjamin, 2026-10-10, verbatim:

> *"mostly i need I will close the apps while typing some as unsaved, it should not ask me for
> save or discard, because later I want to see that 5/10 open window for different info!
> **This is the main reason of building it.**"*
> *"if possible to close the entire apps without saving the open file/window/tab then let's go"*

**The deliverable is scratch-buffer persistence, not a text editor.** He keeps 5–10 tabs of
unsaved scratch notes permanently open, the way Notepad++ does with `new 1`, `new 2`. Every Mac
editor nags on quit; that nag is the whole reason he cannot replace Notepad++ on the Mac.

**Non-negotiable behaviour:**
- Cmd/Ctrl+Q with 10 dirty untitled tabs → **quits instantly, zero dialogs, nothing lost**.
- Closing a dirty tab → **no prompt**.
- Next launch → **every tab back**: content, tab order, active tab, caret and scroll position.
- A crash or force-quit must lose **at most a few seconds** of typing.

**⚠️ ARCHITECTURAL CONSEQUENCES — three things that must never be introduced:**
1. **No `beforeunload` handler** in the web layer. That is the webview's equivalent of the save
   nag and will produce a browser "Leave site?" dialog on quit.
2. **No "unsaved changes" confirm dialog** anywhere in the close/quit path, however well meant.
3. **Autosave must be continuous, not on-quit.** Debounced ~300 ms per keystroke batch, written
   through Rust to disk. By the time a quit happens there must be nothing left pending — relying
   on a flush during teardown is how you lose data when the webview is killed first.

---

## Stack — Tauri 2 + Rust core + React 19 + CodeMirror 6

Chosen 2026-10-10 by Benjamin, from four options, **because it is the BHUninstaller stack** and
therefore gets Windows and Linux nearly free from one UI.

| part | choice |
|---|---|
| shell | **Tauri 2** (Rust), same as BHUninstaller |
| core | **`crates/bhn-core`** — pure Rust: session store, autosave, language map. No UI, no Tauri. |
| UI | **React 19 + TypeScript + Vite 7** |
| editor | **CodeMirror 6** |
| CI | **GitHub Actions matrix** — this is how all 3 platforms are really built (see below) |

### ⭐ How all 3 platforms actually get built — GitHub Actions, NOT this MacBook
Benjamin asked how BHUninstaller shipped `.dmg` + `.exe`/`.msi` + `.deb`/`.rpm` from one Mac.
It did not: `.github/workflows/release.yml` runs a **matrix on native runners** —
`macos-latest` (aarch64 + x86_64 targets), `windows-latest`, `ubuntu-latest`, `ubuntu-24.04-arm` —
each running `npm run tauri build`. Copy that file's hard-won details:
- **`APPLE_SIGNING_IDENTITY: '-'` on macOS** — ad-hoc sign *during* bundling. Signing after the
  .dmg is made is too late, and an unsigned download reports as **"damaged"**, not merely
  unverified.
- **Windows/Linux jobs are `optional: true`** (`continue-on-error`) so a macOS release still ships.
- **Collect bundles with `find` and `gh release upload`** — `tauri-action` failed to collect the
  Windows/Linux bundles it had just built.
- **`mapfile` does not exist in macOS's bash 3.2** — use a `while IFS= read -r` loop.
- **Prune old releases only after confirming the new one has assets**, or a failed build leaves
  nothing downloadable. (Matches the standing BH rule: only the latest release stays listed.)
- Build arm64 Linux too: a Linux VM on an Apple-silicon Mac is arm64 and an amd64 `.deb` refuses
  to install on it. GitHub's arm64 runners are free for public repos.

Local toolchain verified 2026-10-10: cargo 1.97.1, rustc 1.97.1, node 22.23.1, npm 10.9.8;
rust targets already installed: `aarch64-apple-darwin`, `x86_64-pc-windows-msvc`,
`x86_64-unknown-linux-gnu`. macOS 27.0.1 arm64, Xcode 27.

### Keep the core portable
`crates/bhn-core` is plain Rust with **no `tauri` dependency** and no platform `cfg` branches in
its logic. The config directory is **injected** by the shell, never discovered by the core:
- macOS `~/Library/Application Support/BHNotepad/`
- Windows `%APPDATA%\BHNotepad\`
- Linux `${XDG_CONFIG_HOME:-~/.config}/BHNotepad/`

`<configDir>/session.json` (tab order, paths, caret, language, active tab)
+ `<configDir>/buffers/<id>.txt` (content snapshot for **every** tab, named or untitled alike).
**Snapshot writes must be atomic** (temp file + rename) or a crash mid-write truncates a scratch
note — which is exactly the data this app exists to protect.

---

## Layout (mirrors BHUninstaller)

```
notepad/
├── Cargo.toml              ← workspace: crates/bhn-core, app/src-tauri
├── crates/bhn-core/        ← ⭐ portable Rust core. No Tauri, no UI.
├── app/
│   ├── src-tauri/          ← Tauri shell: commands, window, menus, quit path
│   └── src/                ← React + CodeMirror 6
└── .github/workflows/      ← release.yml (matrix), ci.yml
```

## Status — v0.1.0 built and verified on macOS arm64, 2026-10-10

- [x] Verified Notepad++ itself cannot be ported — pure Win32, no UI abstraction
      (`Notepad_plus.rc` alone is 86 KB of Win32 dialogs; `NppDarkMode.cpp` 126 KB).
- [x] Verified how BHUninstaller builds 3 platforms (Actions matrix, not local cross-compile).
- [x] `bhn-core`: session store, atomic+fsync writes, language table, **54 tests green**.
- [x] Tauri shell + React/CodeMirror UI: tabs (drag-reorder, middle-click close), open/save,
      syntax highlighting (~45 languages, lazy-loaded chunks), line numbers, word wrap,
      light/dark, status bar.
- [x] **Find/Replace as a floating Notepad++-style dialog** (Benjamin asked: the default
      CodeMirror docked strip was rejected). Match case · whole word · regex · Count ·
      Select All · Replace All; draggable; `Cmd/Ctrl+F` and `+H`.
- [x] **The promise verified end-to-end in the real app**: typed into two tabs, `kill -9`
      (no terminate, no flush), relaunched → both tabs back with text, on the active tab.
- [x] **Size:** `.app` 3.7 MB · `.dmg` 2.0 MB.
- [x] **File associations** for ~70 extensions, on all three platforms. macOS gets documents via
      `RunEvent::Opened`; Windows and Linux via argv, with `tauri-plugin-single-instance` so a
      second launch hands its files to the running app instead of starting a rival process that
      would fight over `session.json`.
- [x] **Close All** (one confirmation — the single deliberate exception to the no-prompt rule)
      and **Save All** (asks per unsaved tab, cancelling one skips it rather than aborting the
      run), reachable from a **File menu** at the left of the tab strip and by
      `Cmd/Ctrl+Shift+W` / `Cmd/Ctrl+Alt+S`.
- [x] **Two security audits done and fixed** (mine, then Codex) — see below. 76 tests green.
- [ ] Windows + Linux: built by CI, never run on real hardware. Benjamin to test the `.exe`.
- [ ] Not yet: multiple windows, macro recording, column mode, document map.

### Verified behaviours worth not breaking
`cargo test -p bhn-core` holds the guarantees, and each was proved non-vacuous by sabotaging the
code and watching the test fail:
- `tabs_survive_the_process_being_killed_outright` — a child process types into six tabs then
  `abort()`s; the parent reopens the store and checks all six plus their dirty flags.
- `ten_dirty_scratch_tabs_all_come_back_after_a_quit`, `text_is_on_disk_before_any_quit_happens`,
  `the_first_tab_reaches_disk_before_anything_is_typed`.
- `an_unsafe_id_cannot_reach_outside_the_buffers_directory`,
  `a_corrupt_session_is_set_aside_and_the_app_still_starts`.

---

## ⚠️ Build and behaviour traps found the hard way (2026-10-10)

- **The dirty flag must be persisted on the `false → true` edge.** `update_text` writes the text
  snapshot immediately, but the flag lives in `session.json`, which is only written on structural
  changes. A tab killed before the next structural change came back looking **clean while its
  text was plainly unsaved**. Fix: `persist()` once, on the edge — not per keystroke.
- **⚠️ A "survives a kill" test is vacuous if the last edit is to an ALREADY-dirty tab.** The
  first version passed with the fix sabotaged. The test now creates a sixth tab (clean, persisted
  clean by `new_scratch`, which also *focuses* it — so `set_active` must come after), persists,
  then types into it: the only dirty edge after the last explicit persist.
- **⚠️ `--exact` matches the FULL test path.** Re-invoking the test binary as a child with the
  bare function name ran **zero tests, exited 0, and the assertion passed for the wrong reason**.
  Use `tests::<name>`.
- **Tauri's `bundle_dmg.sh` fails for mundane reasons and the real cause is only in `--verbose`:**
  (a) a missing `licenseFile` path — ours pointed at a `LICENSE` that did not exist yet;
  (b) `hdiutil: convert failed - File exists` when a `.dmg` from a previous run is still there —
  **it does not overwrite**, so clean `target/<triple>/release/bundle` between local builds.
  It can also leave the finished `.dmg` in `bundle/macos/` rather than `bundle/dmg/`, plus a
  ~29 MB `rw.*.dmg` staging image — which is why `release.yml` collects with `find` and excludes
  `rw.*.dmg`. The trailing `hdiutil internet-enable` warning is harmless noise (removed in 10.15).
- **A window with `"visible": false` in `tauri.conf.json` never appears unless something calls
  `show()`.** The app ran headless with no window until the frontend called
  `getCurrentWindow().show()` in a `finally` — in `finally` so a restore failure still produces a
  usable window. Needs `core:window:allow-show` / `allow-set-focus` in the capability file.
- **Swift/`searchKeymap` aside — `search()` is kept but `searchKeymap` is NOT installed**, so
  CodeMirror's docked panel can never open and `Cmd+F` reaches our dialog.
- **GitHub free plan storage:** docs say *"Actions artifacts and GitHub Packages storage share the
  same pooled allowance"* (500 MB). **Release assets are not in that pool** — BHUninstaller has
  38 MB of them and has never hit the limit. ⇒ **never use `actions/upload-artifact` in
  `release.yml`**; upload straight to the release, and keep the prune job.

## Deliberately OUT of scope for v1
Notepad++ plugin compatibility (Win32 DLL ABI — impossible), macro recording, column-mode editing,
document map, FTP browser, multiple windows (tabs in one window first).

---

## Prior art checked — and the Scintilla path we rejected

**Nextpad++** (`nextpad-plus-plus/nextpad-plus-plus-macos`, GPLv3, 1.4k stars, v1.1.2) — an
existing Objective-C++/Cocoa port, started March 2026 by Andrey Letov as "Notepad++ for Mac",
renamed at v1.0.6 in May 2026 after Don Ho objected to the name and chameleon logo (he is fine
with GPL ports as such). Rejected: GPLv3 would force this project's licence, it is widely reported
as AI-"vibe-coded" with 55 open issues, and Benjamin wants his own app in the BH family.
⚠️ **Never paste code from the Notepad++ tree or from Nextpad++** — both GPLv3; this project is MIT.

**Scintilla + Lexilla (the engine real Notepad++ uses) — evaluated, built, then dropped.** It
cannot be combined with Tauri: Scintilla needs a native window, Tauri renders in a webview. Kept
here because the findings cost real time and would otherwise be re-derived:
- Scintilla **5.6.7** `cocoa/Scintilla/Scintilla.xcodeproj` (target `Scintilla`) builds clean on
  Xcode 27/arm64 — **but its `MACOSX_DEPLOYMENT_TARGET` is 10.13, which Xcode 27 refuses**
  (*"supported … versions is 12.0 to 27.0.x"*). Pass `MACOSX_DEPLOYMENT_TARGET=13.0` on the
  command line; that is the only change needed.
- `cocoa/checkbuildosx.sh` is **stale** — references a `ScintillaFramework` dir that no longer
  exists (renamed to `cocoa/Scintilla` in 2021) and SDKs 10.14/10.15.
- Lexilla **5.5.4** builds **universal (arm64 + x86_64)** via `src/makefile`, but **requires
  Scintilla as a sibling at `../../scintilla`** or it refuses to start.
- ⚠️ Lexilla's makefile gives the dylib a **relative `install_name` (`../bin/liblexilla.dylib`)**,
  so it fails to load from anywhere else — link `liblexilla.a` or fix with `install_name_tool`.
- ⚠️ **Swift cannot reach the lexers at all.** The framework's `module.modulemap` has
  `exclude header "ILexer.h"` with the comment *"Swift doesn't yet interoperate with C++"*, and
  `CreateLexer()` returns a C++ `ILexer5*`. It needs an Objective-C++ shim that calls
  `CreateLexer()` and passes the pointer via `-setReferenceProperty:` (`SCI_SETILEXER`).
- Lexilla exposes **139 lexers**, and the names are not the obvious ones — enumerated from the
  built library rather than guessed: HTML is **`hypertext`**, PHP is **`phpscript`**, and **there
  is no `javascript` lexer** (Notepad++ routes JS through `cpp`). No `go`, `swift` or
  `typescript` lexer exists either.

---

## 🔒 Security audit — 2026-10-10, before first publication

Run as a deploy gate per the standing rule (build → test → **audit** → fix → deploy), adversarially
and by lens, then re-checked independently by Codex. Every fix below carries a test, and every
test was proved non-vacuous by sabotaging the fix and watching it fail.

### Fixed
1. **Store was world-readable.** `session.json` and every `buffers/*.txt` were created `0644`
   inside `0755` directories, so any other account on the machine could read scratch notes —
   which is exactly where a password or token gets pasted "just for a second". Now created
   `0600` via `OpenOptions::mode()` **at creation**, not chmod-ed afterwards (a chmod leaves a
   window where the file is world-readable), with directories forced to `0700` on every open so
   stores made by an earlier build are tightened too. Windows is unaffected — `%APPDATA%` is
   already ACL-restricted — hence the `#[cfg(unix)]`.
2. **`open_path` was unbounded and did not check the file type.** With file associations added,
   a double-clicked multi-gigabyte log, or a character device such as `/dev/zero` (which never
   ends), would be pulled into memory and then across the IPC boundary into the editor. Now
   `metadata()` is consulted first: regular files only, `MAX_OPEN_BYTES` = 64 MiB.
3. **Saving widened file permissions.** `write_user_file` writes a temp file and renames it over
   the target, and a rename replaces the destination's permissions with the temp's — so
   re-saving a file the user had deliberately made `0600` silently made it world-readable. The
   destination's existing permissions are now copied onto the temp before the rename.

4. **A late autosave could put a closed tab's text back on disk.** `update_text` wrote the
   snapshot *before* checking the tab still existed, and the UI debounces that call by 300 ms —
   so a keystroke just before a close landed just after it, recreating the file, which then sat
   there until the next launch pruned it as an orphan. Someone who closes a tab to be rid of what
   is in it should be rid of it. Now an id that is no longer in the session is ignored.
   (Found by walking the data-loss lens by hand, not by a tool.)

### Checked and found clean (worth not re-deriving)
- **`core:default` is read-only introspection.** I suspected it might include
  `allow-create-webview-window`, which would let a compromised webview open a remote page. It
  does not: it expands to `get-all-webviews`, window/app *getters*, `path`, `event`, `image`,
  `resources`, `menu`, `tray` only. The capability set is genuinely minimal; **this suspicion
  died under refutation** and should not be re-raised.
- No `dangerouslySetInnerHTML`, `innerHTML`, `eval` or `new Function` anywhere in the UI; file
  contents reach CodeMirror as text, never as markup.
- No `Command::new`, no process spawning, no HTTP client, no socket anywhere in the app. The only
  `std::process` call in the tree is `abort()` inside the power-cut test.
- Buffer ids are validated (`is_safe_id`: alphanumeric/`-`/`_`, ≤128, not `.`/`..`) before being
  used to build a path, so a crafted `session.json` cannot escape the store.

### Residual risk, accepted and documented
`save(id, path)` is an arbitrary file write and `open_path(path)` an arbitrary read, both callable
by any JS in the webview. That is unavoidable for an editor. It is contained by the strict CSP
(no remote scripts/styles/frames, everything bundled), no remote content ever being loaded, and
no HTML sink in the UI — so there is no route for foreign script to run in the first place.

---

## 🔒 Second audit — Codex, independent, 2026-10-10

Codex raised 16 surviving findings and said hold the release. **It was right about the
important ones.** Everything below was verified against the code before acting — not taken on
trust — and each fix has a test proved non-vacuous by sabotage.

### The two that mattered most
1. **🔴 A non-ASCII filename aborted the entire app.** `language::detect` did `name[1..]` to skip a
   leading dot, which slices *inside* the first character of `é.txt`, `বাংলা.txt`, `日本語.py` or
   `📄.md`. Release builds set `panic = "abort"`, so double-clicking such a file killed the
   process and took every other tab's pending keystrokes with it. **Bengali filenames crashed the
   editor** — about the worst possible bug for this user. Fixed with `rfind('.')` (ASCII, so
   always a char boundary) filtered to offset > 0. Verified live: a `বাংলা.txt` now opens fine.
2. **🔴 A broken session file deleted every note.** `Workspace::open` pruned orphan snapshots
   against the loaded session — and `load_session` returns an *empty* session when `session.json`
   is corrupt **or missing**. So the one moment the snapshots were the only surviving copy was the
   moment they were erased. **Pruning is gone entirely**, replaced by `recover_orphans`: an
   unreferenced snapshot with text in it becomes a tab again (empty ones are dropped). Closing a
   tab deletes its snapshot explicitly, so in normal operation there are no orphans at all.
   ⚠️ The old test `an_orphaned_snapshot_is_cleaned_up_on_the_next_launch` was **inverted, not
   deleted** — the new promise is now the thing protected.

### Also fixed
3. **Save wrote the previous text.** `save()` reads the *store*, and the UI debounces writes by
   300 ms — so typing then pressing Save immediately wrote the older text and reported success.
   The Editor now exposes an imperative flush that Save, Save All and window-hide call first.
4. **A steady typist could go minutes with nothing on disk.** A pure debounce resets on every
   keystroke. Added `AUTOSAVE_MAX_MS = 2000` as a ceiling regardless of typing speed.
5. **Save All skipped edits made after a save.** Dirtiness was reported only on a tab's *first*
   edit, so a saved-then-edited tab looked clean, Save All skipped it and closing it discarded
   the change. The first-edit channel is gone; every debounced write marks the tab dirty.
6. **Close All said "nothing will be lost" while discarding work** — it counted only path-less
   tabs, ignoring saved files with unsaved edits. It now counts both, and **persists the new
   session before deleting snapshots** (the reverse order left a window where a crash produced
   tabs pointing at missing files).
7. **A failed save truncated the original.** Any failure fell back to `fs::write`, so a full disk
   destroyed the existing file and then failed anyway. The fall-back is gone — the temp file is
   in the destination's own directory, so a rename can never need to cross a filesystem and it
   bought nothing.
8. **The permission fix from round one was incomplete.** The temp file was created with the umask
   default and chmod-ed *after* writing, so a private file's contents sat briefly in a 0644 temp.
   Permissions are now decided before the bytes are written.
9. **TOCTOU on open.** `metadata(path)` then `read(path)` resolves the path twice. Now one handle:
   checked via `file.metadata()` and read through `take(limit + 1)`, which also catches a file
   that grows mid-read. A path-level pre-check still rejects FIFOs before `open` can block on one.
10. **Paths were stored as given**, so `notes.txt` could reopen against a different directory and
    Save overwrite the wrong file, and `/tmp/a.txt` vs `/tmp/./a.txt` opened rival tabs with
    competing snapshots. Paths are canonicalised on open, and the single-instance callback now
    resolves forwarded arguments against **the sender's** cwd instead of discarding it.
11. **macOS could run a second instance** (`open -n`) sharing one store. Single-instance is now on
    for every desktop platform. ⚠️ `cfg(desktop)` works in Rust source but **not as a Cargo target
    predicate** — Cargo doesn't know the name and silently selects nothing, so the platforms are
    listed out in `Cargo.toml`. Verified: `open -n` yields one process, and file association still
    works cold and warm.
13. **Line endings and encoding were destroyed silently.** CodeMirror normalises CRLF to LF, so a
    one-word change to a Windows file rewrote every line. `Buffer` now carries `eol` and `lossy`:
    the file's own line endings are restored on save, and a file that could not be decoded as
    UTF-8 **refuses to be saved over** (Save As still works, so the user can rescue it). Both are
    shown in the status bar.
14. **The prune job could delete every usable release.** It ran on `always()` and needed only one
    asset, so a partial build could replace a complete release. Now `if: success()` plus a check
    that **both** macOS installers are attached, and a `concurrency: release` group.
15. **`contents: write` spanned the whole release job**, including `npm ci` and the build. Added
    `persist-credentials: false` so third-party build code finds no git credential; the upload
    authenticates explicitly through `GH_TOKEN`. CI declares `contents: read`.
16. **No directory fsync.** `sync_all` commits a file's contents but not the directory entry
    naming it, so a real power cut could lose a rename. Added after every rename, in the store and
    on user saves.

### Deliberately NOT fixed — stated, not hidden
- **#2 The IPC boundary is an arbitrary file read/write** (`open_path`, `save`) callable by any JS
  in the webview. Unavoidable for an editor; redesigning it around opaque grants is a large change
  for a threat that requires foreign script to run in the first place, and the CSP plus the total
  absence of remote content and HTML sinks is what prevents that. ⚠️ Codex was right that the
  **capability file's description was inaccurate** — it claimed dialog-only filesystem access.
  Rewritten to say plainly that it does not bound the app's own commands.
- **#12 ReDoS in Find** — the user types their own regex, locally, in their own editor; Notepad++
  is no different. Count and Replace All run on the UI thread. Documented, not fixed.
- Pinning Actions to commit SHAs: not done, same as BHUninstaller. Accepted.

### Codex findings that died under verification or refutation
Its own refutation pass killed ten more (file content executing as HTML, `../` in buffer ids, the
late-autosave resurrection already fixed in round one, tab-switch cancelling edits, lexer hangs,
invalid UTF-8 panicking Rust, store symlinks, shell injection via tag names, fork PRs getting the
write token). ⚠️ Codex also **stalled on first dispatch**, replying "I'll wait. Say go" and doing
nothing — if a review comes back empty, check for that before believing there were no findings.

---

## 📏 File size limits — MEASURED, 2026-10-10 (do not change these by guesswork)

Benjamin pushed back on the 64 MB cap: *"sometimes I need to open 1/2GB or bigger database …
I never faced any opening issue with notepad++ regarding my file size at all, in my whole life!"*
He was right that 64 MB was far too low. So the ceiling was **measured on his machine**
(24 GB, Apple silicon, SSD at ~4.9 GB/s) rather than argued about:

| size | load | memory | verdict |
|---|---|---|---|
| 64 MB | ~1 s | 369 MB | fully editable |
| 128 MB | ~1 s | 627 MB | fully editable |
| 256 MB | ~6 s | 539 MB | fully editable |
| **512 MB** | **~7 s** | **717 MB settled** | **fully editable — the default limit** |
| 768 MB | displays | 2.4 GB peak | **cannot save**: `update_text` dies with *Out of memory* |
| 1 GB | never arrives | 2.1 GB peak | tab stuck — the string never reaches the editor |

⚠️ **The wall is the IPC boundary, not the disk.** A full 512 MB snapshot write takes 0.10 s on
this SSD — storage is irrelevant. What fails is handing the document to the webview as one
JavaScript string. That is a consequence of choosing Tauri + CodeMirror over Scintilla for the
3-platform win, and no amount of tuning moves it; only a chunked/virtual document model would.

**What was changed as a result:**
1. **`max_open_bytes` is a SETTING**, not a constant — `<configDir>/settings.json`, default
   512 MB, floor 1 MB so a typo cannot make the editor useless. Being a setting also means the
   ceiling can be re-measured without rebuilding, which is how the 768 MB and 1 GB numbers above
   were obtained.
2. **An opened file is no longer copied into the store.** It used to write a second 512 MB
   snapshot for a file the user had not even edited. Now only scratch tabs and *edited*
   file-backed tabs have snapshots; a clean file-backed tab is re-read from the file itself.
   ⚠️ This has a deliberate side effect worth knowing: a clean tab shows what is on disk **now**
   after a restart, so a change made by another program is visible rather than a stale copy.
   ⚠️ And it broke `save()`, which read the snapshot and would therefore have written an EMPTY
   file over the user's data — caught by `saving_a_clean_file_tab_does_not_blank_it`, which was
   written before the bug was known. Both `save()` and `text()` now go through `contents()`.
3. **Failures are visible.** A file that is refused used to do *nothing at all* — no tab, no
   message, the window did not even come forward. There is now an `open-failed` event that puts
   the reason in the status bar and raises the window. And `bufferText` has a 60 s timeout, so a
   document that cannot cross the boundary shows an error instead of "Loading…" forever.

**If this is revisited:** 512 MB is the largest size *verified to work*, not a cautious guess.
Do not raise the default without re-running the measurement, and do not lower it without a
reason — he has a real use case at this size.
