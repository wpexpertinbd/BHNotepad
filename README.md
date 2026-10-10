# BHNotepad — a notepad whose unsaved tabs survive quitting

Close it with ten unsaved scratch notes open. It will not ask you to save or discard.
Open it again and every tab is exactly where you left it.

That is the whole point. Every other editor treats an unsaved buffer as a problem to resolve
before you are allowed to quit. Notepad++ on Windows does not, and that is the one habit that
has no equivalent on macOS or Linux — so this is it.

**Free, open source (MIT), no telemetry, no account, ~3 MB.**
macOS · Windows · Linux.

---

## ✨ Why

If you keep five or ten little notes permanently open — a server IP, a snippet you are about to
paste, a half-written reply, the thing you must not forget this afternoon — then every save
prompt is noise. Those notes are not documents. They do not want filenames or folders. They want
to still be there tomorrow.

BHNotepad treats them that way:

- **Quit with unsaved tabs → no dialog.** Not "save", not "discard", not "are you sure".
- **Close a tab → no dialog.** It just closes.
- The single exception is **Close All**, which asks once, because it throws away every scratch
  note at the same time.
- **Reopen → everything back:** tab order, the tab you were on, and the caret where you left it.
- **Pull the power out → still back.** Text reaches disk as you type, not when you quit.

## 🧠 How it keeps your tabs

There is no magic and no cloud. Every tab — saved file or untitled scratch note — is written to a
small store in your own user folder a fraction of a second after you stop typing:

```
macOS    ~/Library/Application Support/BHNotepad/
Windows  %APPDATA%\BHNotepad\
Linux    ${XDG_CONFIG_HOME:-~/.config}/BHNotepad/

  session.json        which tabs are open, their order, caret positions
  buffers/<id>.txt    the text of each tab
```

Two details that matter more than they look:

- **Writes are atomic** — written to a temporary file, flushed to the disk, then renamed into
  place. A crash in the middle leaves you the previous version, never half a file.
- **Nothing waits for the quit.** Flushing at shutdown is how editors lose work, because the
  process does not always get the chance. By the time you quit, there is nothing left to write.

This is verified, not asserted: the test suite starts a child process, types into six tabs, and
kills it outright with `abort()` — no cleanup, no flush, no destructors — then reopens the store
and checks all six tabs and their text came back.

## 📦 What it does

- **Tabs** — drag to reorder, middle-click to close, `new 1`/`new 2` naming that reuses freed
  numbers the way Notepad++ does.
- **Syntax highlighting** for ~45 languages, picked automatically from the file name.
- **Find & Replace** in a proper floating dialog, not a strip glued to the window: match case,
  whole word, regular expressions, count, select all, replace all.
- **Per-tab word wrap**, line numbers, current-line highlight, bracket matching.
- **Light and dark** following the system, with no setting to forget about.
- **Open and save real files** when a note graduates into one, including by double-clicking one
  in Finder or Explorer — BHNotepad registers itself for about 70 text and source extensions.
- **Save All** walks your unsaved tabs one at a time, asking where each should go.
- **Close All** clears the board behind a single confirmation that tells you how many tabs have
  never been saved.

## 💻 Platform support

| Platform | Download | Notes |
|---|---|---|
| macOS (Apple silicon) | `.dmg` | Built and used daily on Apple silicon |
| macOS (Intel) | `.dmg` | Built in CI |
| Windows | `.exe` / `.msi` | Built in CI, **not yet run on real hardware** |
| Linux (x86_64, arm64) | `.deb` / `.rpm` | Built in CI, **not yet run on real hardware** |

Grab the latest from **[Releases](https://github.com/wpexpertinbd/BHNotepad/releases/latest)**.
Only the newest release is kept, so that link is always the right one.

### Opening it the first time on macOS

The builds are ad-hoc signed, not notarised (notarising needs a paid Apple account). macOS will
not open an unidentified app on a double-click:

1. Double-click the app → macOS offers to move it to the Trash. Click **Done**.
2. Open **System Settings › Privacy & Security**, scroll down, click **Open Anyway**.
3. It opens, and never asks again.

Right-click → Open does **not** work on current macOS versions — use Open Anyway.

### Windows

SmartScreen will warn about an unknown publisher (code-signing certificates are not free either).
Click **More info → Run anyway**. The installer is per-user and needs no administrator rights.

## ⌨️ Shortcuts

`Cmd` on macOS, `Ctrl` on Windows and Linux.

| | |
|---|---|
| New tab | `Cmd/Ctrl + T` or `+ N` |
| Open file | `Cmd/Ctrl + O` |
| Save / Save As | `Cmd/Ctrl + S` / `+ Shift + S` |
| Close tab | `Cmd/Ctrl + W` |
| Close all tabs | `Cmd/Ctrl + Shift + W` |
| Save all tabs | `Cmd/Ctrl + Alt + S` |
| Find / Replace | `Cmd/Ctrl + F` / `+ H` |
| Next / previous tab | `Ctrl + Tab` / `Ctrl + Shift + Tab` |
| Toggle word wrap | `Cmd/Ctrl + Shift + Z` |
| Undo / Redo | `Cmd/Ctrl + Z` / `+ Shift + Z` |

## 🚧 Deliberate limits

- **Notepad++ plugins do not work and never will.** They are Windows DLLs written against a Win32
  API that does not exist here. Nothing short of running Notepad++ itself will load them.
- **Not a clone.** No macro recording, column-mode editing, document map or FTP browser yet.
- **One window.** Tabs live in a single window for now.
- **Files are capped at 64 MB.** Opening is unbounded otherwise, and the text is held in memory,
  handed to the editor and snapshotted on every autosave — a cap is the honest limit rather than
  a hang. Very large files are not this editor's strength in any case.

## 🔨 Building

Needs [Rust](https://rustup.rs) and [Node 20+](https://nodejs.org).

```bash
git clone https://github.com/wpexpertinbd/BHNotepad.git
cd BHNotepad/app
npm ci
npm run tauri dev      # run it
npm run tauri build    # produce installers for this platform
```

Run the core's test suite — including the power-cut test — with:

```bash
cargo test -p bhn-core
```

## 🏗️ Architecture

```
crates/bhn-core/     Portable Rust core: the session store, crash-safe autosave,
                     language detection. No UI, no Tauri, no platform branches.
app/src-tauri/       Tauri shell: window, IPC commands, config directory.
app/src/             React + CodeMirror 6 interface.
```

The core knows nothing about the operating system — even the config directory is handed to it by
the shell. That is what keeps the three platform builds the same application instead of three
lookalikes that drift apart.

## 📄 Licence

MIT. See [LICENSE](LICENSE).

BHNotepad is an independent program. It is **not** Notepad++, not a fork of it, and contains none
of its code — Notepad++ is GPLv3 and welded to the Win32 API. "Notepad++" is Don Ho's; this
project has no affiliation with it and does not use its name or logo for its own identity.

Built on [CodeMirror 6](https://codemirror.net) (MIT) and [Tauri](https://tauri.app) (MIT/Apache-2.0).

## ☕ Support

BHNotepad is free and open-source. If it saved you time, you can **buy me a coffee** — it
genuinely helps me keep building and maintaining free tools like this. 🙏

- **bKash/Nagad** (Personal · *Send Money*): **`01710378396`**

ধন্যবাদ! / Thank you!

— BiswasHost · <https://www.biswashost.com>
