import type { NavigateFunction } from "react-router-dom";
import type { SubmitActionOptions, SwitchProposalView } from "../game/utils";
import type { GameSessionStateBag } from "../game/useGameSession";
import type { GameUiRuntimeContext } from "./runtimeContext";

type InputComposerBridge = {
  openImagePicker: () => void;
  startRecording: () => Promise<void>;
  stopRecording: () => void;
};

export type GameUiRuntimeActions = {
  clearActionError: () => void;
  setDraftValue: (value: string) => void;
  setAutoScrollEnabled: (enabled: boolean) => void;
  submitMessage: (options?: SubmitActionOptions) => Promise<void>;
  startEditingTurn: (content: string, turnIndex: number) => void;
  cancelEditingTurn: () => void;
  branchFromCurrent: () => Promise<void>;
  retryTurn: (retryToken: string) => Promise<void>;
  acceptSwitchProposal: (proposal: SwitchProposalView) => Promise<void>;
  dismissSwitchProposal: (proposalKey: string) => void;
  dismissRetryCard: (cardKey: string) => void;
  copyText: (text: string) => Promise<void>;
  switchSideTab: (tabKey: string) => void;
  navigateBack: () => void;
  navigateSettings: () => void;
  navigateDebug: () => void;
  pickImage: (files?: File[]) => void;
  removeImage: (index: number) => void;
  startRecording: () => Promise<void>;
  stopRecording: () => void;
  removeAudio: (index: number) => void;
  addAudio: (file: File) => void;
  attachInputComposerBridge: (bridge: InputComposerBridge | null) => void;
};

export function createGameUiRuntimeActions(
  bag: GameSessionStateBag,
  runtime: GameUiRuntimeContext,
  navigate: NavigateFunction,
): GameUiRuntimeActions {
  let inputComposerBridge: InputComposerBridge | null = null;

  return {
    clearActionError: bag.clearActionError,
    setDraftValue: bag.setInputValue,
    setAutoScrollEnabled: bag.setChatAutoScrollEnabled,
    submitMessage: (options = {}) => bag.handleSubmitAction(options),
    startEditingTurn: bag.startEditingTurn,
    cancelEditingTurn: bag.cancelEditingTurn,
    branchFromCurrent: bag.handleBranch,
    retryTurn: (retryToken) => bag.handleRetryFailedStep({ retry_token: retryToken }),
    acceptSwitchProposal: bag.handleAcceptSwitchProposal,
    dismissSwitchProposal: bag.dismissSwitchProposal,
    dismissRetryCard: bag.dismissDirectorRetryCard,
    copyText: bag.handleCopyMessage,
    switchSideTab: (tabKey) => {
      bag.setSideTab(tabKey);
    },
    navigateBack: () => {
      navigate(-1);
    },
    navigateSettings: () => {
      navigate("/settings");
    },
    navigateDebug: () => {
      if (!runtime.session?.id) {
        return;
      }
      navigate(`/debug/${runtime.session.id}`);
    },
    pickImage: (files) => {
      if (files && files.length > 0) {
        bag.setInputImages((previous) => [...previous, ...files]);
        return;
      }
      inputComposerBridge?.openImagePicker();
    },
    removeImage: (index) => {
      bag.setInputImages((previous) =>
        previous.filter((_, imageIndex) => imageIndex !== index),
      );
    },
    startRecording: async () => inputComposerBridge?.startRecording(),
    stopRecording: () => inputComposerBridge?.stopRecording(),
    removeAudio: (index) => {
      bag.setInputAudios((previous) =>
        previous.filter((_, audioIndex) => audioIndex !== index),
      );
    },
    addAudio: (file) => bag.setInputAudios((previous) => [...previous, file]),
    attachInputComposerBridge: (bridge) => {
      inputComposerBridge = bridge;
    },
  };
}
