# Piao (飘) Default UI

This world package provides the default "Piao" themed Game UI documents for the GWTW world.

## Files
- `desktop-ui.jsonc` - Desktop game UI document with GWTW styling
- `mobile-ui.jsonc` - Mobile game UI document with GWTW styling

## Usage
These files are world-package resources and should be loaded by the world loader at runtime.
They replace the formerly hardcoded `PIAO_DEFAULT_DESKTOP_DOCUMENT` and `PIAO_DEFAULT_MOBILE_DOCUMENT` 
constants in `frontend/src/data/gameUi.ts`.

## Migration (2026-05-24)
Moved from `frontend/src/data/gameUi.ts` as part of P2 architecture refactoring.
