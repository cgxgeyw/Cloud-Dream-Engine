import { useCallback } from "react";
import { useSearchParams } from "react-router-dom";

export type SectionParamControls<TId extends string> = {
  activeSection: TId | null;
  openSection: (next: TId) => void;
  closeSection: () => void;
};

// 移动端「打开某项设置」由 URL 的 ?section= 驱动。
// 返回固定回到列表（清除参数），不依赖浏览器历史，避免从外部入口进入时退到别的页面。
export function useSectionParam<TId extends string>(
  validIds: readonly TId[],
): SectionParamControls<TId> {
  const [searchParams, setSearchParams] = useSearchParams();

  const rawSection = searchParams.get("section");
  const activeSection: TId | null =
    rawSection && (validIds as readonly string[]).includes(rawSection) ? (rawSection as TId) : null;

  const openSection = useCallback(
    (next: TId) => {
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("section", next);
          return params;
        },
        { replace: false },
      );
    },
    [setSearchParams],
  );

  const closeSection = useCallback(() => {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.delete("section");
        return params;
      },
      { replace: true },
    );
  }, [setSearchParams]);

  return { activeSection, openSection, closeSection };
}
