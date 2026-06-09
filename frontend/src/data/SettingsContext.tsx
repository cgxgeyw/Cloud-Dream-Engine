import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { assetUrl, fetchSettings, updateSettings, type SettingsResponse } from "./apiAdapter";

type SettingsContextValue = {
  settings: SettingsResponse | null;
  loading: boolean;
  /** 从服务端重新加载设置 */
  refresh: () => Promise<void>;
  /** 更新设置并同步刷新上下文 */
  save: (patch: SettingsResponse) => Promise<SettingsResponse>;
  /** 解析后的背景资源地址，可直接用于 img/src/css；不存在时为空串 */
  backgroundUrl: string;
  /** 当前背景是否为视频 */
  backgroundIsVideo: boolean;
};

const SettingsContext = createContext<SettingsContextValue | null>(null);

export function SettingsProvider({ children }: { children: ReactNode }) {
  const [settings, setSettings] = useState<SettingsResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const mountedRef = useRef(false);

  const refresh = useCallback(async () => {
    try {
      const data = await fetchSettings();
      if (mountedRef.current) {
        setSettings(data);
      }
    } catch {
      // 静默处理：首次启动时设置可能尚未创建，或后端暂时不可达
    } finally {
      if (mountedRef.current) {
        setLoading(false);
      }
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void refresh();
    return () => {
      mountedRef.current = false;
    };
  }, [refresh]);

  const save = useCallback(
    async (patch: SettingsResponse) => {
      const updated = await updateSettings(patch);
      setSettings(updated);
      return updated;
    },
    [],
  );

  // 统一通过 assetUrl 解析背景资源地址
  const bgStrategy = settings?.home_background_strategy?.trim() ?? "";
  const backgroundUrl = bgStrategy && bgStrategy !== "static" ? assetUrl(bgStrategy) : "";
  const backgroundIsVideo = Boolean(backgroundUrl) && /\.(mp4|webm|ogg|mov)$/i.test(bgStrategy);

  return (
    <SettingsContext.Provider value={{ settings, loading, refresh, save, backgroundUrl, backgroundIsVideo }}>
      {children}
    </SettingsContext.Provider>
  );
}

export function useSettings() {
  const ctx = useContext(SettingsContext);
  if (!ctx) {
    return {
      settings: null,
      loading: false,
      refresh: async () => {},
      save: async () => { throw new Error("SettingsProvider not available"); },
      backgroundUrl: "",
      backgroundIsVideo: false,
    } as SettingsContextValue;
  }
  return ctx;
}
