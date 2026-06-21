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
- When inserting Chinese text into Rust, TSX, JSONC, or seed assets through scripts, prefer Unicode escapes for short literals if there is any risk the shell or terminal encoding is not UTF-8 clean.
- After any scripted replacement in Rust source, immediately inspect the affected string literals. A common failure here is quote duplication or truncation, for example turning `"on-demand"` into `""on-demand""`.
- After any scripted replacement in JSONC seed files, immediately inspect nearby lines for unterminated strings. In this repo, damaged Chinese text often surfaces as a line that visually “looks fine” but is missing the closing `"`.
- If a file is already partly garbled, do not try to preserve the exact broken text. Preserve syntax and behavior first; replace damaged prose with clean Chinese or ASCII.

## Before Editing

- Inspect the exact file section first.
- If touching a file with lots of garbled text, identify syntax boundaries rather than matching the garbled prose itself.
- For large broken blocks, replace by stable function/component boundaries, not by fragile mojibake substrings.
- Keep unrelated user changes intact. This workspace may not be a git repo.
- Before editing seed assets or prompt-heavy files, run a narrow search or line dump around the target region first. Do not trust search/replace on a file you have not visually inspected.

## After Editing

Run the narrowest relevant checks:

- Frontend syntax/types: `cd frontend && npx tsc --noEmit --pretty false`
- Frontend build: `cd frontend && npm run build`
- Rust check: `cd src-tauri && cargo check`

When compiler output shows many impossible errors such as `unknown prefix`, `unterminated raw string`, JSX closing tag explosions, or unexpected tokens far below the edited area, suspect an earlier string/tag was broken by encoding damage.

Also run targeted sanity checks when relevant:

- For Rust files with string-heavy edits, inspect the edited literals directly.
- For JSONC seed files, run an odd-quote scan if parsing errors point at unrelated later lines.
- If one seed/test fix exposes another garbled file, continue cleaning the chain before assuming there is a logic regression.

## Common Failure Patterns Seen Here

- JSX closing tags became text, for example `?/div>` instead of `</div>`.
- Rust raw strings lost their terminator, for example `?#` instead of `"#`.
- Rust and TS string literals lost a closing quote inside mojibake text.
- Rust string replacements duplicated quotes around JSON payload strings, for example `""on-demand""`.
- A single broken string caused misleading downstream errors such as `prefix a is unknown` for normal literals like `"char-a"`.
- JSONC seed files failed much later than the real damage point because one Chinese string lost its closing quote and swallowed the next structural token.
- TypeScript pages restored from old snapshots could compile worse because they lacked current functionality.

## Recovery Strategy

- First fix syntax boundaries so compilers can parse the file.
- Then fix real type errors.
- Prefer replacing corrupted prompt/prose with clean, stable Chinese or ASCII text instead of trying to preserve mojibake.
- If a scripted edit touched multiple encoding-sensitive files, validate each touched file immediately instead of waiting for one global build at the end.
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
  messages, character/agent speech, player speech, plus the world-director and
  NPC chains-of-thought. Hide retry/debug cards, character creation/switch
  cards, and other management UI from the normal mobile chat flow unless the
  user explicitly asks for them.
- Chains-of-thought (world director + NPC) stream into the chat flow on both
  desktop and mobile, collapsed: the CoT always shows only the first 200
  characters with an expand/collapse toggle for the rest; NPC answer body is
  never folded. CoT uses dedicated classes (`game-cot*`); do not reuse the
  legacy `.game-agent-reasoning` (mobile seeds still hide it via `display:none`).
- Narration renders as its own message right after the corresponding NPC
  message, produced by the frontend `MessageList` from the agent message's
  `metadata.narration`. Frontend-only; no backend change, not persisted as an
  extra message.
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
