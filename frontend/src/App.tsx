import { lazy, Suspense } from "react";
import { BrowserRouter, HashRouter, Route, Routes } from "react-router-dom";
import { ResponsiveLayout } from "./components/ResponsiveLayout";
import { SettingsProvider } from "./data/SettingsContext";
import { isTauriEnvironment } from "./data/apiAdapter";
import { ToastContainer } from "./components/Toast";

const HomePage = lazy(() => import("./pages/HomePage").then((module) => ({ default: module.HomePage })));
const NewGamePage = lazy(() => import("./pages/NewGamePage").then((module) => ({ default: module.NewGamePage })));
const NewGameSetupPage = lazy(() => import("./pages/NewGamePage").then((module) => ({ default: module.NewGameSetupPage })));
const SavesPage = lazy(() => import("./pages/SavesPage").then((module) => ({ default: module.SavesPage })));
const WorldsPage = lazy(() => import("./pages/WorldsPage").then((module) => ({ default: module.WorldsPage })));
const WorldEditorPage = lazy(() => import("./pages/WorldEditorPage").then((module) => ({ default: module.WorldEditorPage })));
const WorldCharactersPage = lazy(() => import("./pages/WorldCharactersPage").then((module) => ({ default: module.WorldCharactersPage })));
const CharacterEditorPage = lazy(() => import("./pages/CharacterEditorPage").then((module) => ({ default: module.CharacterEditorPage })));
const GamePage = lazy(() => import("./pages/GamePage").then((module) => ({ default: module.GamePage })));
const DebugPage = lazy(() => import("./pages/DebugPage").then((module) => ({ default: module.DebugPage })));
const SettingsPage = lazy(() => import("./pages/SettingsPage").then((module) => ({ default: module.SettingsPage })));
const McpToolsPage = lazy(() => import("./pages/McpToolsPage").then((module) => ({ default: module.McpToolsPage })));

function AppRoutes() {
  return (
    <Suspense fallback={<div className="app-route-loading" />}>
      <Routes>
        <Route path="/" element={<HomePage />} />
        <Route path="/new-game" element={<NewGamePage />} />
        <Route path="/new-game/setup/:worldId" element={<NewGameSetupPage />} />
        <Route path="/saves" element={<SavesPage />} />
        <Route path="/worlds" element={<WorldsPage />} />
        <Route path="/worlds/new" element={<WorldEditorPage />} />
        <Route path="/worlds/:id/edit" element={<WorldEditorPage />} />
        <Route path="/worlds/:worldId/characters" element={<WorldCharactersPage />} />
        <Route path="/characters/new" element={<CharacterEditorPage />} />
        <Route path="/characters/:id/edit" element={<CharacterEditorPage />} />
        <Route path="/game/:sessionId" element={<GamePage />} />
        <Route path="/debug/:sessionId" element={<DebugPage />} />
        <Route path="/settings" element={<SettingsPage />} />
        <Route path="/mcp-tools" element={<McpToolsPage />} />
        <Route path="*" element={<HomePage />} />
      </Routes>
    </Suspense>
  );
}

export default function App() {
  const Router = isTauriEnvironment() ? HashRouter : BrowserRouter;

  return (
    <Router>
      <SettingsProvider>
        <ResponsiveLayout>
          <AppRoutes />
        </ResponsiveLayout>
        <ToastContainer />
      </SettingsProvider>
    </Router>
  );
}
