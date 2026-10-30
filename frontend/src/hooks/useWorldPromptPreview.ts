import { useCallback, useEffect, useRef, useState } from "react";
import {
  fetchAttributeSchemas,
  fetchWorldOpeningPromptPreview,
  type AttributeSchemaResponse,
  type WorldOpeningPromptPreviewResponse,
} from "../data/apiAdapter";

export type UseWorldPromptPreviewParams = {
  active: boolean;
  isNew: boolean;
  worldId: string | null | undefined;
  playerCharacterId: string | null | undefined;
  directorConfig?: Record<string, unknown>;
};

export type WorldPromptPreviewState = {
  preview: WorldOpeningPromptPreviewResponse | null;
  attributeSchemas: AttributeSchemaResponse[];
  loading: boolean;
  error: string | null;
  attributeSchemaError: string | null;
  reload: () => Promise<void>;
};

export function useWorldPromptPreview({
  active,
  isNew,
  worldId,
  playerCharacterId,
  directorConfig,
}: UseWorldPromptPreviewParams): WorldPromptPreviewState {
  const [preview, setPreview] = useState<WorldOpeningPromptPreviewResponse | null>(null);
  const [attributeSchemas, setAttributeSchemas] = useState<AttributeSchemaResponse[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [attributeSchemaError, setAttributeSchemaError] = useState<string | null>(null);
  const requestIdRef = useRef(0);

  const load = useCallback(async (stableWorldId: string, stablePlayerCharacterId?: string | null) => {
    const requestId = ++requestIdRef.current;

    try {
      setLoading(true);
      setError(null);
      setAttributeSchemaError(null);
      const schemasPromise = Promise.all([
        fetchAttributeSchemas("world", stableWorldId),
        fetchAttributeSchemas("character", stableWorldId),
      ]).then(([worldSchemas, characterSchemas]) => ({
        schemas: [...worldSchemas, ...characterSchemas],
      })).catch((schemaError: unknown) => ({
        schemaError: schemaError instanceof Error ? schemaError.message : "加载属性定义失败",
      }));
      const [data, schemaResult] = await Promise.all([
        fetchWorldOpeningPromptPreview(stableWorldId, {
          playerCharacterId: stablePlayerCharacterId,
          playerInput: "\u7ee7\u7eed",
          directorConfig,
        }),
        schemasPromise,
      ]);
      if (requestId === requestIdRef.current) {
        setPreview(data);
        if ("schemas" in schemaResult) {
          setAttributeSchemas(schemaResult.schemas);
          setAttributeSchemaError(null);
        } else {
          setAttributeSchemas([]);
          setAttributeSchemaError(schemaResult.schemaError);
        }
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
  }, [directorConfig]);

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

  return { preview, attributeSchemas, loading, error, attributeSchemaError, reload };
}
