import { useCallback, useEffect, useRef, useState } from "react";
import {
  fetchWorldOpeningPromptPreview,
  type WorldOpeningPromptPreviewResponse,
} from "../data/apiAdapter";

export type UseWorldPromptPreviewParams = {
  active: boolean;
  isNew: boolean;
  worldId: string | null | undefined;
  playerCharacterId: string | null | undefined;
};

export type WorldPromptPreviewState = {
  preview: WorldOpeningPromptPreviewResponse | null;
  loading: boolean;
  error: string | null;
  reload: () => Promise<void>;
};

export function useWorldPromptPreview({
  active,
  isNew,
  worldId,
  playerCharacterId,
}: UseWorldPromptPreviewParams): WorldPromptPreviewState {
  const [preview, setPreview] = useState<WorldOpeningPromptPreviewResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const requestIdRef = useRef(0);

  const load = useCallback(async (stableWorldId: string, stablePlayerCharacterId?: string | null) => {
    const requestId = ++requestIdRef.current;

    try {
      setLoading(true);
      setError(null);
      const data = await fetchWorldOpeningPromptPreview(stableWorldId, {
        playerCharacterId: stablePlayerCharacterId,
        playerInput: "\u7ee7\u7eed",
      });
      if (requestId === requestIdRef.current) {
        setPreview(data);
      }
    } catch (previewError) {
      if (requestId === requestIdRef.current) {
        setError(previewError instanceof Error ? previewError.message : "加载 prompt 预览失败");
      }
    } finally {
      if (requestId === requestIdRef.current) {
        setLoading(false);
      }
    }
  }, []);

  const reload = useCallback(async () => {
    if (isNew || !worldId) {
      requestIdRef.current += 1;
      setLoading(false);
      setError("新世界保存后才能生成 prompt 预览。");
      return;
    }

    await load(worldId, playerCharacterId);
  }, [isNew, load, playerCharacterId, worldId]);

  useEffect(() => {
    if (!active || isNew || !worldId) {
      requestIdRef.current += 1;
    } else {
      void load(worldId, playerCharacterId);
    }

    return () => {
      requestIdRef.current += 1;
    };
  }, [active, isNew, load, playerCharacterId, worldId]);

  return { preview, loading, error, reload };
}
