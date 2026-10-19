import type { CSSProperties } from "react";
import type { GameUiDocument, GameUiPlatform } from "../data/gameUi";

export type PreviewMessage = {
  role: string;
  content: string;
  speaker?: string | null;
};

export type PreviewStatusTab = {
  key: string;
  label: string;
  content: string;
};

export type GameUiPreviewProps = {
  platform: GameUiPlatform;
  document: GameUiDocument;
  stylesheet?: string;
  scopeId?: string;
  rootStyle?: CSSProperties;
  worldName: string;
  location: string;
  timeLabel: string;
  playerName: string;
  visibleCharacters: string[];
  focusSpeaker?: string;
  focusContent?: string;
  portraitSrc?: string;
  narration?: string;
  messages: PreviewMessage[];
  statusTabs?: PreviewStatusTab[];
  parseError?: string | null;
  usedFallback?: boolean;
  showStyleDebug?: boolean;
};
