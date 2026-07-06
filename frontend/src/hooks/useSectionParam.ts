import { useCallback } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";

// 是否存在「应用内」的历史可回退。
//
// react-router 在 history.state 上维护一个自增的 idx：首屏/直接深链为 0，
// 应用内每次 push 后递增。我们据此判断 navigate(-1) 是否会落在应用内的上一页，
// 还是会离开应用。这是 react-router 的实现细节而非公开契约，因此只在此处集中
// 依赖它；若将来该字段变化，只需改这一个函数。idx 缺失时按「无历史」处理，
// 让调用方回退到清除参数 / 跳转父级，而不是盲目 navigate(-1) 离开应用。
export function hasInAppHistory(): boolean {
  if (typeof window === "undefined") {
    return false;
  }
  const state = window.history.state as { idx?: unknown } | null;
  return !!state && typeof state.idx === "number" && state.idx > 0;
}

export type SectionParamControls<TId extends string> = {
  activeSection: TId | null;
  openSection: (next: TId) => void;
  closeSection: () => void;
};

// 移动端「打开某项设置」由 URL 的 ?section= 驱动，使其成为一条真实历史记录：
// 点返回按钮与侧滑/系统返回都只是历史回退，落点一致（都回到列表）。
//
// 打开：push 一条带 ?section= 的历史；关闭：有应用内历史时走历史回退
// （navigate(-1)），与侧滑/系统返回一致；无历史（首屏深链/刷新进入）时直接
// 清除 ?section= 参数回到列表，避免 navigate(-1) 离开应用。
export function useSectionParam<TId extends string>(
  validIds: readonly TId[],
): SectionParamControls<TId> {
  const navigate = useNavigate();
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
    if (hasInAppHistory()) {
      navigate(-1);
      return;
    }
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.delete("section");
        return params;
      },
      { replace: true },
    );
  }, [navigate, setSearchParams]);

  return { activeSection, openSection, closeSection };
}
