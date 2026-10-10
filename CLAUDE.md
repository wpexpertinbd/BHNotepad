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
- [ ] Windows + Linux: built by CI, never run on real hardware. Benjamin to test the `.exe`.
- [ ] Not yet: open a file by double-clicking it in Finder/Explorer (no file-association or
      argv handling), multiple windows, macro recording, column mode.

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
