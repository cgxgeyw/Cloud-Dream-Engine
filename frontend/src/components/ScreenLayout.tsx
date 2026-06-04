import type { CSSProperties, ReactNode } from "react";
import { useSettings } from "../data/SettingsContext";
import { ChevronRight } from "lucide-react";

type ScreenLayoutProps = {
  title: string;
  subtitle?: string;
  toolbar?: ReactNode;
  children: ReactNode;
  maxWidth?: number;
  compactHeader?: boolean;
};

type SurfacePanelProps = {
  children: ReactNode;
  style?: CSSProperties;
  className?: string;
};

type MenuButtonProps = {
  title: string;
  description?: string;
  icon?: ReactNode;
  primary?: boolean;
  onClick?: () => void;
};

export function ScreenLayout({
  title,
  subtitle,
  toolbar,
  children,
  maxWidth = 1240,
  compactHeader = false,
}: ScreenLayoutProps) {
  const { backgroundUrl, backgroundIsVideo } = useSettings();

  return (
    <div
      className="layout-root"
      style={
        backgroundUrl && !backgroundIsVideo
          ? {
              backgroundImage: `url(${backgroundUrl})`,
              backgroundSize: "cover",
              backgroundPosition: "center",
              backgroundRepeat: "no-repeat",
              backgroundAttachment: "fixed",
            }
          : undefined
      }
    >
      {backgroundUrl && backgroundIsVideo ? (
        <video
          className="layout-bg-video"
          src={backgroundUrl}
          autoPlay
          loop
          muted
          playsInline
        />
      ) : null}

      <div
        className={`layout-inner${compactHeader ? " layout-inner--compact" : ""}${backgroundUrl ? " layout-inner--has-bg" : ""}`}
        style={{ maxWidth }}
      >
        <header className={`layout-header${compactHeader ? " layout-header--compact" : ""}${backgroundUrl ? " layout-header--has-bg" : ""}`}>
          <div className="layout-title-shell">
            <div className="grid grid--gap-xs">
              <div className="layout-brand">云朵梦境</div>
              <h1 className={`layout-title${compactHeader ? " layout-title--compact" : ""}`}>{title}</h1>
              {subtitle ? <p className="layout-subtitle">{subtitle}</p> : null}
            </div>
          </div>

          {toolbar ? (
            <div className="layout-toolbar-shell">
              <div className="layout-toolbar">{toolbar}</div>
            </div>
          ) : null}
        </header>

        {children}
      </div>
    </div>
  );
}

export function SurfacePanel({ children, style, className = "" }: SurfacePanelProps) {
  return (
    <div
      className={`surface-panel${className ? ` ${className}` : ""}`}
      style={style}
    >
      {children}
    </div>
  );
}

export function MenuButton({ title, description, icon, primary = false, onClick }: MenuButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`menu-btn${primary ? " menu-btn--primary" : ""}`}
    >
      <div className="menu-btn-icon">{icon}</div>

      <div className="menu-btn-text">
        <strong className="menu-btn-title">{title}</strong>
        {description ? <span className="menu-btn-desc">{description}</span> : null}
      </div>

      <div className="menu-btn-arrow"><ChevronRight size={16} /></div>
    </button>
  );
}

export function ToolbarLink({
  children,
  href,
  primary = false,
}: {
  children: ReactNode;
  href?: string;
  primary?: boolean;
}) {
  const className = `action-btn${primary ? " action-btn--primary" : ""}`;

  if (href) {
    return (
      <a href={href} className={className}>
        {children}
      </a>
    );
  }

  return <span className={className}>{children}</span>;
}
