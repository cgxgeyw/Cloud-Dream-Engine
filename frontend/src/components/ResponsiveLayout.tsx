import { type ReactNode, useEffect, useState } from "react";
import { MobileNav } from "./MobileNav";
import { isAndroidRuntime } from "../data/apiAdapter";

const MOBILE_BREAKPOINT = 768;

export function useIsMobile() {
  const [isMobile, setIsMobile] = useState(() => {
    if (isAndroidRuntime()) return true;
    return typeof window !== "undefined" && window.innerWidth < MOBILE_BREAKPOINT;
  });

  useEffect(() => {
    // Android 运行时始终为移动端
    if (isAndroidRuntime()) {
      setIsMobile(true);
      return;
    }

    const mq = window.matchMedia(`(max-width: ${MOBILE_BREAKPOINT - 1}px)`);
    setIsMobile(mq.matches);

    const handler = (e: MediaQueryListEvent) => setIsMobile(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);

  return isMobile;
}

interface ResponsiveLayoutProps {
  children: ReactNode;
  showHeader?: boolean;
  headerRight?: ReactNode;
  headerLeft?: ReactNode;
  transparentHeader?: boolean;
  isGame?: boolean;
}

export function ResponsiveLayout({ children }: ResponsiveLayoutProps) {
  const isMobile = useIsMobile();

  if (isMobile) {
    return <MobileNav>{children}</MobileNav>;
  }

  return <>{children}</>;
}

export function MobileOnly({ children }: { children: ReactNode }) {
  const isMobile = useIsMobile();
  if (!isMobile) return null;
  return <>{children}</>;
}

export function DesktopOnly({ children }: { children: ReactNode }) {
  const isMobile = useIsMobile();
  if (isMobile) return null;
  return <>{children}</>;
}
