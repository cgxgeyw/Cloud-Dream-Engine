# Agent Notes

## Encoding Safety

This project has many files that already contain mojibake text from earlier encoding damage. Treat every TSX, Rust, JSONC, and seed file with Chinese text as encoding-sensitive.

Rules:

- Do not use PowerShell `Set-Content` to rewrite source files that contain Chinese text or existing mojibake.
- Do not do broad text rewrites through shell pipelines when the file has mixed Chinese/mojibake content.
- Prefer `apply_patch` for manual edits.
- If a bounded script edit is necessary, use Node `fs.readFileSync(path, "utf8")` and `fs.writeFileSync(path, text, "utf8")`, and keep the replacement range narrow.
- Never “repair” mojibake globally unless the user explicitly asks for an encoding cleanup pass.
- Avoid replacing entire large TSX/Rust files from old backups unless the user explicitly approves losing current functionality.

## Before Editing

- Inspect the exact file section first.
- If touching a file with lots of garbled text, identify syntax boundaries rather than matching the garbled prose itself.
- For large broken blocks, replace by stable function/component boundaries, not by fragile mojibake substrings.
- Keep unrelated user changes intact. This workspace may not be a git repo.

## After Editing

Run the narrowest relevant checks:

- Frontend syntax/types: `cd frontend && npx tsc --noEmit --pretty false`
- Frontend build: `cd frontend && npm run build`
- Rust check: `cd src-tauri && cargo check`

When compiler output shows many impossible errors such as `unknown prefix`, `unterminated raw string`, JSX closing tag explosions, or unexpected tokens far below the edited area, suspect an earlier string/tag was broken by encoding damage.

## Common Failure Patterns Seen Here

- JSX closing tags became text, for example `?/div>` instead of `</div>`.
- Rust raw strings lost their terminator, for example `?#` instead of `"#`.
- Rust and TS string literals lost a closing quote inside mojibake text.
- A single broken string caused misleading downstream errors such as `prefix a is unknown` for normal literals like `"char-a"`.
- TypeScript pages restored from old snapshots could compile worse because they lacked current functionality.

## Recovery Strategy

- First fix syntax boundaries so compilers can parse the file.
- Then fix real type errors.
- Prefer replacing corrupted prompt/prose with clean, stable Chinese or ASCII text instead of trying to preserve mojibake.
- Use odd-quote scans to locate likely broken lines, then verify manually:

```powershell
@'
const fs = require('fs');
for (const path of process.argv.slice(2)) {
  const lines = fs.readFileSync(path, 'utf8').split(/\r?\n/);
  console.log('---', path);
  for (let i = 0; i < lines.length; i++) {
    let dq = 0, bt = 0;
    for (let j = 0; j < lines[i].length; j++) {
      if (lines[i][j] === '"' && lines[i][j - 1] !== '\\') dq++;
      if (lines[i][j] === '`' && lines[i][j - 1] !== '\\') bt++;
    }
    if (dq % 2 || bt % 2) console.log(i + 1, lines[i]);
  }
}
'@ | node - frontend/src/pages/WorldEditorPage.tsx
```

## Mobile World UI Rules

These rules apply when editing mobile world UI documents such as
`src-tauri/src/db/seeds/assets/*-mobile-ui.jsonc` and the shared runtime
components under `frontend/src/gameUiRuntime/components/`.

- Android/mobile pages must reserve top safe space. Do not let headers or
  handles sit flush against the top edge; use `env(safe-area-inset-top, 0px)`
  plus a real fallback offset.
- All visible mobile components must avoid the status/sidebar handle area on
  the right. Header title/location text should truncate with ellipsis instead
  of running underneath the handle.
- Mobile custom fields belong in the side/status drawer. On mobile, the side
  panel is the status drawer; do not render custom tabs as inline content above
  or inside the chat column.
- Mobile chat should stay focused on narrative content: narration/system
  messages, character/agent speech, and player speech. Hide director traces,
  retry/debug cards, reasoning blocks, and other management UI from the normal
  mobile chat flow unless the user explicitly asks for them.
- Do not put the global copy button in the mobile title/header. Message actions
  belong under each message.
- Under character/agent messages, show small copy and branch buttons.
- Under player messages, show small edit and resend buttons. Do not add an
  extra player-message copy button unless explicitly requested.
- Mobile input composers should use a two-row layout: the text area occupies
  100% of the chat width on its own row, and image, voice, and send buttons sit
  together on one row below it.
- Image/voice buttons should be icon buttons. Send can be a short localized
  text label. Avoid mixing English labels into Chinese mobile UI.
- If adding Chinese defaults in TS/TSX through scripts, prefer Unicode escapes
  or `apply_patch`; PowerShell stdin has repeatedly damaged Chinese text into
  question marks in this repo.
