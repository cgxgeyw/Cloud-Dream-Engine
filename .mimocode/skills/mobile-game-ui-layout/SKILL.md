---
name: mobile-game-ui-layout
description: Fix mobile game page UI layout issues — input box positioning, button placement, keyboard overlap, and safe-area handling across shells and CSS.
---

# Mobile Game UI Layout Fix

Guides debugging and fixing mobile game page layout in this Tauri/React project. The game page has platform-specific shells selected by `GamePageController.tsx` based on `isMobile`.

## Architecture

- **Shell selection**: `frontend/src/game/GamePageController.tsx` — renders `MobileGameShell` or `DesktopGameShell`
- **Mobile shell**: `frontend/src/game/shells/MobileGameShell.tsx`
- **Desktop shell**: `frontend/src/game/shells/DesktopGameShell.tsx`
- **Input composer**: `frontend/src/gameUiRuntime/components/InputComposer.tsx` — shared by both shells
- **Game UI runtime**: `frontend/src/gameUiRuntime/` — layout registry, component nodes, actions
- **CSS files** (check all when editing):
  - `frontend/src/index.css` — global game layout classes (`game-input-compose`, `game-chat`, `game-main`)
  - `frontend/src/styles/theme.css` — theme-level overrides, mobile media queries
  - `frontend/src/styles/mobile.css` — iOS/mobile-specific styles
- **World UI seeds**: `src-tauri/src/db/seeds/assets/*-mobile-ui.jsonc` — per-world mobile UI definitions with embedded CSS in `custom_css`

## Common Issues and Fixes

### 1. Input box not full width on mobile

The mobile input composer should occupy 100% width. Check:
- `.game-input-compose` in `index.css` — ensure `grid-template-columns: 1fr` for mobile
- `InputComposer.tsx` — verify the wrapper uses `game-input-compose--mobile` class
- Per-world `custom_css` in `*-mobile-ui.jsonc` may override — check if the world seed has conflicting rules

### 2. Buttons (voice/image/send) crowding the input box

Layout rule: text area on its own row (100% width), buttons on a second row below.
- Check `.game-input-compose` grid layout for mobile
- The `game-input-actions` or equivalent row should be below the textarea
- Buttons should be icon-sized (Image, Mic, Send from lucide-react)

### 3. Keyboard overlap hides input

On Android/iOS, soft keyboard push should shrink the visible area, not hide the input.
- Use `env(safe-area-inset-bottom, 0px)` for bottom padding
- The game layout should use `min-height: 100dvh` with `overflow: hidden` on the root
- Avoid fixed `height` on the game container — use `flex: 1` with `min-height: 0` on the chat area

### 4. Content under sidebar/back button (safe-area top)

- Top safe area: `env(safe-area-inset-top, 0px)` + real fallback offset
- Scene header text should truncate with ellipsis, not overlap the sidebar handle
- Right edge: avoid placing content under the mobile sidebar/handle area

### 5. World-specific CSS overrides breaking mobile layout

Per-world mobile UI seeds contain `custom_css` that can override base styles.
- Inspect the specific world's `*-mobile-ui.jsonc` file
- Look for `custom_css` entries that set `display`, `grid-template`, or `position` on game layout classes
- Use Node.js to parse JSONC (strip comments, then `JSON.parse`) — see AGENTS.md encoding rules

## Validation Steps

After any mobile layout fix:

1. **TypeScript check**: `cd frontend && npx tsc --noEmit`
2. **Build check**: `cd frontend && npm run build`
3. **CSS audit**: Verify no class name collisions between `index.css`, `theme.css`, and `mobile.css`
4. **World seed check**: If a world has custom CSS, verify it doesn't conflict with base mobile layout
5. **Inspect affected CSS classes directly** after edits — especially in `theme.css` mobile media queries

## Encoding Safety

When editing `*-mobile-ui.jsonc` files or any file with Chinese text:
- Use Node.js `fs.readFileSync(path, 'utf8')` for parsing/editing
- Do not use PowerShell `Set-Content` or bash pipelines for these files
- After edits, run an odd-quote scan if parsing errors appear downstream
- Use Unicode escapes for short Chinese literals inserted via scripts
