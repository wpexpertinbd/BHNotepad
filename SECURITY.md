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
- **Scratch notes are NOT encrypted.** They are ordinary files, exactly like the ones you would
  save yourself. Do not keep passwords, keys or other secrets in them. On macOS and Linux the
  store directory is created `0700` and every file in it `0600`, so other accounts on the same
  machine cannot read them; on Windows `%APPDATA%` is already restricted to your user. That
  protects them from other local users, not from anything running as you.
- **No file access beyond what you pick.** The app only reads or writes a file you choose in an
  Open or Save dialog, one you hand it by double-clicking or dropping it on the icon, plus its
  own store. It never opens anything on its own.

## How the app is locked down

- The webview is granted only `core:default`, showing its own window, and the Open/Save dialogs
  (`app/src-tauri/capabilities/default.json`). No shell, no arbitrary filesystem scope, no HTTP.
- A strict Content-Security-Policy is set in `tauri.conf.json`: no remote scripts, styles, fonts
  or frames; everything is bundled.
- Buffer ids are validated before they are used to build a path, so a hand-edited or crafted
  `session.json` cannot make the app read or write outside its own store. This is covered by a
  test.
- A corrupt `session.json` is set aside rather than trusted or silently overwritten, and the app
  still starts.
- Opening a file checks it is a regular file and at most 64 MB first, so a double-clicked
  multi-gigabyte log — or a character device such as `/dev/zero` — cannot exhaust memory or hang
  the app. File associations make that an attacker-reachable path, which is why it is checked.
- Saving copies the destination file's existing permissions onto the temporary file before the
  rename, so re-saving a file you had deliberately made private does not widen it.

## Supported versions

Only the latest release is supported, and only the latest release is kept on the Releases page.
