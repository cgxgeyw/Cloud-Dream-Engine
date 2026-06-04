import { useIsMobile } from "../components/ResponsiveLayout";
import { GamePageController } from "../game";

export function GamePage() {
  const isMobile = useIsMobile();

  return <GamePageController isMobile={isMobile} />;
}
