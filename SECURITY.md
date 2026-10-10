# Security Policy

## Reporting a vulnerability

Please report security issues privately rather than opening a public issue:

- GitHub **Security → Report a vulnerability** on this repository, or
- email **benjamin.biswas@gmail.com** with `BHNotepad security` in the subject.

Include what you found, how to reproduce it, and the version you tested. You will get an
acknowledgement within a few days. Please give a reasonable window for a fix before disclosing
publicly.

## What BHNotepad does with your data

Nothing leaves your machine.

- **No network access at all.** The app makes no HTTP requests, has no telemetry, no analytics,
  no crash reporting and no update check.
- **Your notes stay in your user folder** — `~/Library/Application Support/BHNotepad` on macOS,
  `%APPDATA%\BHNotepad` on Windows, `${XDG_CONFIG_HOME:-~/.config}/BHNotepad` on Linux — in plain
  text, readable by you and by anything else running as you.
- **A closed tab's text is kept so it can be restored.** Closing a tab moves its snapshot to a
  `closed/` folder beside your session rather than deleting it, so Restore Recent Closed File can
  bring it back. The last ten are kept and older ones are deleted properly. If you closed a tab
  to be rid of what was in it, use **View → Forget Closed Tabs**, which removes the history and
  every snapshot in it.
- **Scratch notes are NOT encrypted.** They are ordinary files, exactly like the ones you would
  save yourself. Do not keep passwords, keys or other secrets in them. On macOS and Linux the
  store directory is created `0700` and every file in it `0600`, so other accounts on the same
  machine cannot read them; on Windows `%APPDATA%` is already restricted to your user. That
  protects them from other local users, not from anything running as you.
- **No file access beyond what you pick.** The app only reads or writes a file you choose in an
  Open or Save dialog, one you hand it by double-clicking or dropping it on the icon, plus its
  own store. It never opens anything on its own.

## How the app is locked down

- The capability file (`app/src-tauri/capabilities/default.json`) grants only `core:default`
  (read-only introspection), showing and focusing this window, and the native Open/Save/confirm
  sheets. No shell plugin, no HTTP plugin, no filesystem plugin.
- **Being straight about what that does not cover:** BHNotepad's own commands — `open_path` and
  `save` — read and write through `std::fs` in Rust and are callable by any script running in the
  window. An editor cannot avoid that. What prevents it being reachable is that no remote content
  is ever loaded, the CSP forbids remote scripts and `eval`, and nothing in the interface renders
  anything as HTML, so there is no way for foreign script to run in the first place. If you find
  one, that is the report worth sending.
- A strict Content-Security-Policy is set in `tauri.conf.json`: no remote scripts, styles, fonts
  or frames; everything is bundled.
- Buffer ids are validated before they are used to build a path, so a hand-edited or crafted
  `session.json` cannot make the app read or write outside its own store. This is covered by a
  test.
- A corrupt `session.json` is set aside rather than trusted or silently overwritten, and the app
  still starts.
- Opening a file checks it is a regular file and within the configured size limit (512 MB by
  default, `max_open_bytes` in `settings.json`) first, so a double-clicked
  multi-gigabyte log — or a character device such as `/dev/zero` — cannot exhaust memory or hang
  the app. File associations make that an attacker-reachable path, which is why it is checked.
- Saving creates its temporary file with the destination's permissions from the outset, so a
  private file's contents are never briefly readable by others, and the rename cannot widen it.
- A save that fails leaves the original untouched. There is no fall-back direct write, because
  the case that makes the temporary file fail — a full disk — is exactly the case where falling
  back would truncate the file you already had.
- A file that is not valid UTF-8 is opened read-only in effect: the bytes that could not be
  decoded were replaced, so saving over the original is refused and Save As is offered instead.
- Line endings are preserved. A Windows file is written back with CRLF rather than being
  silently converted.

## Supported versions

Only the latest release is supported, and only the latest release is kept on the Releases page.
