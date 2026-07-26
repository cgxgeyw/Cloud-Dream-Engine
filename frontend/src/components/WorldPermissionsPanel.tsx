import { useEffect, useState } from "react";

import {
  fetchWorlds,
  listWorldFeatureGrants,
  setWorldFeatureGrant,
} from "../data/apiAdapter";
import type { WorldFeatureGrantStatus } from "../data/types";

type WorldGrantGroup = {
  worldId: string;
  worldName: string;
  features: WorldFeatureGrantStatus[];
};

// 能力 id → 中文说明。新增平台能力时在这里补一行。
const FEATURE_LABELS: Record<string, string> = {
  "file.pick": "选择文件：从设备里选一个或多个文件并读取内容",
  "file.read": "读取文件：读取该世界专属目录里的文件",
  "file.write": "写入文件：在该世界专属目录里创建或覆盖文件",
  "file.share": "分享文件：经系统分享面板把文件发给其它应用",
};

function featureLabel(feature: string): string {
  return FEATURE_LABELS[feature] ?? feature;
}

/**
 * 第 12 项「世界权限」面板：按世界包列出它在 manifest 里声明的平台能力，
 * 玩家逐项允许/关闭（默认关）。世界包调用未允许的能力会得到明确报错。
 */
export function WorldPermissionsPanel() {
  const [groups, setGroups] = useState<WorldGrantGroup[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pendingKey, setPendingKey] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const worlds = await fetchWorlds();
        const loaded: WorldGrantGroup[] = [];
        for (const world of worlds) {
          const features = await listWorldFeatureGrants(world.id);
          if (features.length > 0) {
            loaded.push({ worldId: world.id, worldName: world.name, features });
          }
        }
        if (!cancelled) {
          setGroups(loaded);
        }
      } catch (loadError) {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : String(loadError));
        }
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  async function toggle(worldId: string, feature: string, granted: boolean) {
    const key = `${worldId}:${feature}`;
    setPendingKey(key);
    setError(null);
    try {
      await setWorldFeatureGrant(worldId, feature, granted);
      setGroups((current) =>
        (current ?? []).map((group) =>
          group.worldId !== worldId
            ? group
            : {
                ...group,
                features: group.features.map((status) =>
                  status.feature === feature ? { ...status, granted } : status,
                ),
              },
        ),
      );
    } catch (toggleError) {
      setError(toggleError instanceof Error ? toggleError.message : String(toggleError));
    } finally {
      setPendingKey(null);
    }
  }

  if (error && groups === null) {
    return <div className="text-error">加载世界权限失败:{error}</div>;
  }
  if (groups === null) {
    return <div className="text-muted">正在加载世界权限…</div>;
  }
  if (groups.length === 0) {
    return (
      <div className="text-muted">
        暂无声明平台能力的世界包。世界包在 manifest 里声明 platform_features 后,会出现在这里等你允许。
      </div>
    );
  }

  return (
    <div className="settings-section">
      <p className="text-muted" style={{ marginTop: 4, marginBottom: 16 }}>
        世界包声明的平台能力默认全部关闭。开启后,该世界的逻辑代码才能调用对应能力;关闭即立即生效。
      </p>
      {error ? <div className="text-error" style={{ marginBottom: 12 }}>{error}</div> : null}
      {groups.map((group) => (
        <div key={group.worldId} style={{ marginBottom: 20 }}>
          <h4 className="settings-section-title" style={{ fontSize: 15 }}>{group.worldName}</h4>
          {group.features.map((status) => {
            const key = `${group.worldId}:${status.feature}`;
            return (
              <label key={status.feature} className="field-label field-label--inline" style={{ marginTop: 8 }}>
                <span className="field-label-text">{featureLabel(status.feature)}</span>
                <div className="settings-inline-toggle">
                  <input
                    type="checkbox"
                    checked={status.granted}
                    disabled={pendingKey === key}
                    onChange={(event) => void toggle(group.worldId, status.feature, event.target.checked)}
                  />
                </div>
              </label>
            );
          })}
        </div>
      ))}
    </div>
  );
}
