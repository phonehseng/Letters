# Privacy

Letters works on this computer. Three features can send something over
the network, each only when the person uses it. There is no telemetry:
Letters doesn't report usage, crashes, documents or anything else.

This was checked by searching the code (November 2026) for every way a
program reaches the network, as listed under "How this was checked".

## What can leave this computer, and when

| Feature | What is sent, and where | When |
|---|---|---|
| Font downloads (`core/fonts/src/download.rs`) | a request for one font file (or its preview) to the font library's address, `raw.githubusercontent.com/google/fonts`; nothing about your documents | only when you download a font, after the font library's one-time notice |
| Google Drive (`app/shell-core/src/drive/`) | the one file you chose, to your Google Drive; the sign-in happens in your browser with Google, and the sign-in is kept in the system's credential store | only when you choose Upload in Save to Google Drive and confirm what goes where; saving into the Google Drive folder on this computer sends nothing itself |
| AI Tools (`core/ai`, `app/shell-core/src/ai/`) | the text the request needs, to the server you typed in Settings | off by default. With a model on this computer nothing leaves. With an online service, before every request Letters shows what it would send and where, and sends only after Send (or after "Don’t ask again for this provider and document", which lasts while Letters runs and covers only that pair). Test Connection sends one fixed sentence, never document text |

Links in documents and "How to Get a Client ID" open in your browser;
the browser, not Letters, then visits the page.

## What never leaves

- Documents, comments, form answers and signatures.
- Recents, pinned folders, settings (`app-settings.json`, readable json
  in the app's data folder, without any key or password).
- Keys for AI Tools and the Google sign-in, which live in the system's
  credential store (Windows Credential Manager, the macOS keychain).

## How this was checked

- Crates that can make network requests, by their manifests: `ureq` is the
  only http client, used by `core/ai`, `core/fonts` and
  `app/shell-core` (Google Drive). No other http, socket or websocket
  library is a dependency of the app.
- Code search for `ureq::`, `TcpStream::connect`, `fetch(`,
  `XMLHttpRequest`, `WebSocket`, `Invoke-WebRequest` and fixed web
  addresses: the hits are the three features above, the fonts catalogue
  builder in `core/fonts/examples` (a tool run by developers, not part of
  the app), test servers on 127.0.0.1, and xml namespace names (which are
  only labels and are never fetched).
- The web view's content security policy (`app/shell/tauri.conf.json`)
  allows connections only to the app itself (`connect-src 'self' ipc:`)
  and pictures only from the app and its page tiles, so the interface
  can't reach the network either.
- Programs Letters may start: Microsoft Office and LibreOffice (to make
  pdfs, run on this computer with their own settings), the OCR engine, and
  a model server you chose (started on 127.0.0.1 only).
