# UI Control Chain

This document summarizes how UI is controlled in this project, including framework pages outside the game and runtime UI inside the game session.

## 1. Top-Level App Shell

Entry chain:

```text
frontend/src/App.tsx
  -> Router: HashRouter in Tauri, BrowserRouter in web
  -> SettingsProvider
  -> ResponsiveLayout
  -> AppRoutes
  -> page components under frontend/src/pages
  -> ToastContainer
```

`AppRoutes` owns all framework route-to-page mapping:

```text
/                         HomePage
/new-game                 NewGamePage
/new-game/setup/:worldId  NewGameSetupPage
/saves                    SavesPage
/worlds                   WorldsPage
/worlds/new               WorldEditorPage
/worlds/:id/edit          WorldEditorPage
/worlds/:worldId/characters WorldCharactersPage
/characters/new           CharacterEditorPage
/characters/:id/edit      CharacterEditorPage
/game/:sessionId          GamePage
/debug/:sessionId         DebugPage
/settings                 SettingsPage
/mcp-tools                McpToolsPage
```

`ResponsiveLayout` decides desktop/mobile layout context and exposes `useIsMobile()`. Most framework pages receive `isMobile` from `AppRoutes`; the game page calls `useIsMobile()` directly.

## 2. Framework UI Outside The Game

Framework pages are normal React pages in `frontend/src/pages`.

Common layout components:

```text
frontend/src/components/ScreenLayout.tsx
  ScreenLayout  -> page shell, title, subtitle, toolbar, app background
  SurfacePanel  -> shared panel surface
  MenuButton    -> home/menu style button with black fill hover
  ToolbarLink   -> action button wrapper
```

Global visual language is mainly controlled by:

```text
frontend/src/styles/theme.css
```

Important CSS families:

```text
layout-*          app page shell/header/background
surface-panel     framework panel
action-btn        toolbar/form action buttons
menu-btn          home/menu buttons
card-item         world/save/character style list cards
editor-*          world/character/settings editor fields and tabs
newgame-*         new game and world selection pages
game-*            runtime game visuals and fallback component styling
game-ui-*         world-package-driven game UI nodes/components/mounts
```

If a framework page looks wrong, check in this order:

1. Page JSX in `frontend/src/pages/<Page>.tsx`.
2. Shared shell in `ScreenLayout.tsx` or `ResponsiveLayout.tsx`.
3. CSS class definitions in `theme.css`.
4. Global settings background from `SettingsContext`.

Framework UI is not controlled by world packages. It is controlled by React pages and shared CSS.

## 3. Game Page Entry

Game route chain:

```text
/game/:sessionId
  -> frontend/src/pages/GamePage.tsx
  -> frontend/src/game/GamePageController.tsx
  -> useGameSession()
  -> DesktopGameShell or MobileGameShell
  -> GameUiRenderer
```

`GamePage` only determines mobile/desktop and delegates. `GamePageController` owns the split:

```text
isMobile === true  -> MobileGameShell
isMobile === false -> DesktopGameShell
```

Shared game data and actions come from:

```text
frontend/src/game/useGameSession.ts
```

The shell should not duplicate business logic. It should assemble runtime context, actions, component renderers, and pass them to `GameUiRenderer`.

## 4. Game Runtime UI Control

Game runtime UI is controlled by the world UI document stored in each world's `ui_theme_config`.

Data shape:

```text
world.ui_theme_config
  assets
    background_source_mode
    portrait_source_mode
    runtime_image_generation_enabled
    local_background_assets
    local_scene_backgrounds
  desktop_file
  mobile_file
```

Frontend normalization/parsing:

```text
frontend/src/data/gameUi.ts
  -> barrel export

frontend/src/data/gameUi/types.ts
frontend/src/data/gameUi/defaults.ts
frontend/src/data/gameUi/parser.ts
```

Important parser functions:

```text
normalizeWorldUiEnvelope()
normalizeAssetConfig()
resolveUiFile()
parseGameUiDocument()
buildGameUiStylesheet()
defaultGameUiFile()
```

Runtime shell flow:

```text
DesktopGameShell / MobileGameShell
  -> createGameUiRuntimeContext(bag, platform)
  -> createGameUiRuntimeActions(bag, runtime, navigate)
  -> createGameUiComponentRenderers(runtime, actions)
  -> GameUiRenderer({
       document,
       mounts,
       componentRenderers
     })
```

The world document controls layout by arranging mounts, components, slots, conditions, loops, style records, and custom CSS. The framework provides available mount content and component implementations.

## 5. Game UI Renderer

Renderer file:

```text
frontend/src/components/GameUiRenderer.tsx
```

Supported document modes:

```text
schema_version: 1 -> legacy layout tree with mounts
schema_version: 2 -> component tree with mounts, slots, when, for_each
```

Renderer node types:

```text
grid
stack
absolute
mount
component
slot
when
for_each
```

The renderer does not know game business logic. It only renders a document tree and inserts provided mount/component content.

## 6. Runtime Mounts Provided By The Framework

Both desktop and mobile shells currently provide the same mount keys:

```text
header
scene
scene_focus
character_bar
narration
message_list
input_area
side_panel
floating_actions
```

Mount content is assembled in:

```text
frontend/src/game/shells/DesktopGameShell.tsx
frontend/src/game/shells/MobileGameShell.tsx
```

Mount implementations mostly come from:

```text
frontend/src/gameUiRuntime/components/PageComponents.tsx
frontend/src/gameUiRuntime/components/MessageList.tsx
frontend/src/gameUiRuntime/components/InputComposer.tsx
```

Practical meaning:

```text
World package controls where/how these mounts appear.
Framework controls what each mount can render and what actions are safe.
```

Do not add a new backend API just to move existing runtime UI around. Change the world UI document or frontend renderer/component layer instead.

## 7. Runtime Components, Bindings, And Actions

Runtime component registry:

```text
frontend/src/gameUiRuntime/registry.tsx
```

Runtime context:

```text
frontend/src/gameUiRuntime/runtimeContext.ts
```

Runtime actions:

```text
frontend/src/gameUiRuntime/actions.ts
frontend/src/gameUiRuntime/actionSchemas.ts
```

Bindings and expressions:

```text
frontend/src/gameUiRuntime/binding.ts
frontend/src/gameUiRuntime/expression.ts
```

Capabilities:

```text
frontend/src/gameUiRuntime/capabilities.ts
```

Stable design rule:

```text
World UI document may request known components/actions/capabilities.
Framework registry decides what those names mean.
Unknown components render a missing-component placeholder.
Unsupported actions/capabilities should be caught by validation/compatibility checks.
```

## 8. World Editor UI Entry

World editor file:

```text
frontend/src/pages/WorldEditorPage.tsx
```

It edits both world gameplay data and world UI config.

Relevant UI config functions in this page:

```text
normalizeUiThemeConfig()
buildUiThemeEnvelope()
renderGameUiDocumentEditor()
updateGameUiFile()
updateStructuredGameUiDocument()
replaceGameUiSchema()
```

Fields saved into world payload:

```text
ui_theme_config: buildUiThemeEnvelope(uiThemeConfig)
```

Game UI editor tools:

```text
GameUiStructureEditor
GameUiPreview
validateWorldUiBundle()
compileWorldUiDocument()
verifyWorldPackageUiCompatibility()
```

The editor parses desktop/mobile UI documents, previews them, and saves them into the world. That is the main authoring path for world-package-controlled game UI.

## 9. Backend Storage And Validation

Backend model:

```text
src-tauri/src/models/world.rs
  WorldDefinition.ui_theme_config
  WorldCreateRequest.ui_theme_config
  WorldUpdateRequest.ui_theme_config
```

Database column:

```text
src-tauri/src/db/schema.rs
  worlds.ui_theme_config_json TEXT NOT NULL DEFAULT '{}'
```

Backend game UI commands:

```text
src-tauri/src/commands/game_ui.rs
  validate_world_ui_document
  validate_world_ui_bundle
  compile_world_ui_document
  verify_world_package_ui_compatibility
```

Backend game UI service:

```text
src-tauri/src/services/game_ui.rs
```

Backend validation is for governance and compatibility. Runtime rendering still happens in the frontend.

## 10. World Package Import/Export

World package service:

```text
src-tauri/src/services/world_package.rs
```

Package manifest fields:

```text
desktop_ui_file
mobile_ui_file
assets
world
characters
```

World package import/export should preserve:

```text
world.ui_theme_config.desktop_file
world.ui_theme_config.mobile_file
world.ui_theme_config.assets
```

Upload pruning can touch UI asset references:

```text
src-tauri/src/commands/uploads.rs
  prune_world_ui_theme_config()
```

If a world package appears to lose background/UI assets, inspect `world_package.rs`, `uploads.rs`, and `ui_theme_config_json`.

## 11. Seed Worlds

Seed world UI files:

```text
src-tauri/src/db/seeds/assets/gwtw-desktop-ui.jsonc
src-tauri/src/db/seeds/assets/gwtw-mobile-ui.jsonc
src-tauri/src/db/seeds/assets/piao-desktop-ui.jsonc
src-tauri/src/db/seeds/assets/piao-mobile-ui.jsonc
```

Seed world config:

```text
src-tauri/src/db/seeds/piao_world.rs
src-tauri/src/db/seeds/feihualing_world.rs
```

Seed maps are JSON object topology, not legacy arrays. Keep labels clean UTF-8 Chinese and keep edge labels aligned with node labels.

## 12. What Controls What

Framework/out-of-game UI:

```text
React pages + ScreenLayout/ResponsiveLayout + theme.css
```

Game/in-session UI layout:

```text
world.ui_theme_config.desktop_file/mobile_file
  -> parseGameUiDocument()
  -> GameUiRenderer
```

Game/in-session component behavior:

```text
gameUiRuntime components/actions/context
```

Game/in-session data:

```text
useGameSession()
  -> backend session/world/character APIs
  -> runtimeContext
  -> mounts/components
```

Backend:

```text
stores world UI config
validates/compiles/checks compatibility
imports/exports package UI files/assets
does not render UI
```

## 13. Safe Change Guidelines

When changing framework pages:

```text
Edit frontend/src/pages/*
Edit shared layout in ScreenLayout/ResponsiveLayout only if multiple pages need it
Edit theme.css for visual language
Do not change world UI documents unless the game runtime UI should change
```

When changing game runtime layout:

```text
Prefer editing world UI documents in ui_theme_config
Use WorldEditorPage preview/governance checks
Do not hardcode page-specific runtime layout into DesktopGameShell/MobileGameShell unless adding a new stable mount/component
```

When adding a new game UI capability:

```text
1. Add or extend frontend runtime component/action/capability.
2. Register it in gameUiRuntime/registry or actions.
3. Update backend validation/compatibility if the document can reference it.
4. Update default/seed UI documents only after the framework supports it.
5. Add backend API only if the feature needs new data or persistence.
```

When fixing encoding:

```text
Always read/write UTF-8.
Do not use PowerShell output as proof of source encoding if the console is GBK.
Prefer direct UTF-8 file reads and escaped scans.
Replace unrecoverable mojibake semantically instead of trying to re-decode already-corrupted strings.
```

## 14. Quick Debug Checklist

If a framework page is ugly or broken:

```text
App route -> page component -> ScreenLayout/SurfacePanel -> theme.css class
```

If the game screen layout is wrong:

```text
session world -> ui_theme_config -> desktop_file/mobile_file -> parseGameUiDocument -> GameUiRenderer
```

If a runtime component is missing:

```text
document component name -> gameUiRuntime/registry.tsx -> component implementation
```

If an action does nothing:

```text
document action name -> gameUiRuntime/actions.ts -> actionSchemas.ts -> runtime context data
```

If world package UI differs after import/export:

```text
world_package.rs -> manifest desktop_ui_file/mobile_ui_file -> ui_theme_config_json -> asset paths
```

