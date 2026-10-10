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
  save yourself. Do not keep passwords, keys or other secrets in them.
- **No file access beyond what you pick.** The app only reads or writes a file you choose in an
  Open or Save dialog, plus its own store.

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

## Supported versions

Only the latest release is supported, and only the latest release is kept on the Releases page.
