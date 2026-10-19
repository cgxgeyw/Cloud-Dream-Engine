import React from "react";
import { useNavigate } from "react-router-dom";
import { useGameSession } from "./useGameSession";
import { GameUiSandboxRuntime } from "../components/GameUiSandboxRuntime";

/* ============================================================
   GamePageController
   ============================================================

   Composes useGameSession with the isolated HTML world UI runtime.

   Props
   ----
   isMobile : boolean
       When true, selects the mobile world document; otherwise desktop.
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
