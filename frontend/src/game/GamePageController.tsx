import React from "react";
import { useNavigate } from "react-router-dom";
import { useGameSession } from "./useGameSession";
import { GameUiSandboxRuntime } from "../components/GameUiSandboxRuntime";

/* ============================================================
   GamePageController
   ============================================================

   Composes useGameSession (all shared state/effects/actions)
   with a platform-specific shell. The shell receives the full
   state bag and renders the appropriate 9-mount or 4-mount
   layout with all platform-specific JSX differences.

   Props
   ----
   isMobile : boolean
       When true, renders MobileGameShell; otherwise DesktopGameShell.
   ============================================================ */

export interface GamePageControllerProps {
  isMobile: boolean;
}

export const GamePageController: React.FC<GamePageControllerProps> = ({
  isMobile,
}) => {
  const navigate = useNavigate();
  const bag = useGameSession({ isMobile });

  // Redirect to home if no sessionId in URL
  React.useEffect(() => {
    if (!bag.sessionId) {
      navigate("/", { replace: true });
    }
  }, [bag.sessionId, navigate]);

  if (!bag.sessionId) {
    return <div className="game-loading">跳转中...</div>;
  }

  return <GameUiSandboxRuntime bag={bag} platform={isMobile ? "mobile" : "desktop"} />;
};

export default GamePageController;
