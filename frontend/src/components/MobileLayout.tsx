import { type ReactNode } from "react";
import { useSettings } from "../data/SettingsContext";
import { ChevronRight, Inbox } from "lucide-react";

// 类型定义
interface MobileLayoutProps {
  children: ReactNode;
  showHeader?: boolean;
  headerRight?: ReactNode;
  headerLeft?: ReactNode;
  transparentHeader?: boolean;
  isGame?: boolean;
}

// 主布局组件 - 使用 MobileNav 的侧边栏
export function MobileLayout({
  children,
  showHeader = true,
  headerRight,
  headerLeft,
  transparentHeader = false,
  isGame = false,
}: MobileLayoutProps) {
  const { backgroundUrl, backgroundIsVideo } = useSettings();

  // 处理背景样式
  const containerStyle = backgroundUrl && !backgroundIsVideo && !isGame
    ? {
        backgroundImage: `linear-gradient(180deg, rgba(242, 242, 247, 0.95), rgba(242, 242, 247, 0.98)), url("${backgroundUrl}")`,
        backgroundSize: "cover",
        backgroundPosition: "center",
      }
    : undefined;

  return (
    <div 
      className={isGame ? "ios-game" : ""}
      style={containerStyle}
    >
      {/* 移动端头部 - 只显示右侧按钮，左侧菜单按钮由 MobileNav 提供 */}
      {showHeader && !isGame && (
        <header className={`ios-nav ${transparentHeader ? "ios-nav--transparent" : ""}`} style={{ display: 'none' }}>
          {/* MobileNav 已提供导航栏，这里隐藏 */}
        </header>
      )}
      
      <div className={isGame ? "" : "ios-content"}>
        {children}
      </div>
    </div>
  );
}

// ChevronRightIcon 组件
function ChevronRightIcon() {
  return <ChevronRight size={14} />;
}

// 移动端卡片组件
export function MobileCard({
  children,
  onClick,
  icon,
  title,
  subtitle,
}: {
  children?: ReactNode;
  onClick?: () => void;
  icon?: ReactNode;
  title?: string;
  subtitle?: string;
}) {
  return (
    <div 
      className={`ios-card ${onClick ? "ios-card--pressable" : ""}`}
      onClick={onClick}
    >
      {(icon || title) && (
        <div className="ios-card-header">
          {icon && <div className="ios-card-icon">{icon}</div>}
          {title && (
            <div style={{ flex: 1 }}>
              <div style={{ fontSize: "17px", fontWeight: "600", color: "var(--ios-text)" }}>{title}</div>
              {subtitle && <div style={{ fontSize: "14px", color: "var(--ios-text-secondary)", marginTop: "4px" }}>{subtitle}</div>}
            </div>
          )}
        </div>
      )}
      {children}
    </div>
  );
}

// 移动端列表组件
export function MobileList({
  children,
}: {
  children: ReactNode;
}) {
  return <div className="ios-list">{children}</div>;
}

export function MobileListItem({
  icon,
  title,
  subtitle,
  onClick,
  arrow = true,
}: {
  icon?: ReactNode;
  title: string;
  subtitle?: string;
  onClick?: () => void;
  arrow?: boolean;
}) {
  return (
    <button type="button" className="ios-list-item" onClick={onClick}>
      {icon && <div className="ios-list-item-icon">{icon}</div>}
      <div className="ios-list-item-content">
        <div className="ios-list-item-title">{title}</div>
        {subtitle && <div className="ios-list-item-subtitle">{subtitle}</div>}
      </div>
      {arrow && <span className="ios-list-item-arrow"><ChevronRightIcon /></span>}
    </button>
  );
}

// 移动端按钮组件
export function MobileButton({
  children,
  onClick,
  variant = "primary",
  size = "default",
  block = false,
  disabled = false,
}: {
  children: ReactNode;
  onClick?: () => void;
  variant?: "primary" | "secondary" | "destructive";
  size?: "default" | "lg";
  block?: boolean;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className={`ios-btn ios-btn--${variant} ${block ? "ios-btn--block" : ""}`}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

// 空状态组件
export function MobileEmpty({
  icon,
  title,
  description,
}: {
  icon?: ReactNode;
  title: string;
  description?: string;
}) {
  return (
    <div className="ios-empty">
      <div className="ios-empty-icon">{icon || <Inbox size={40} />}</div>
      <h3 className="ios-empty-title">{title}</h3>
      {description && <p className="ios-empty-text">{description}</p>}
    </div>
  );
}

// 加载状态组件
export function MobileLoading() {
  return (
    <div className="ios-loading">
      <div className="ios-spinner" />
    </div>
  );
}

// 分段控制器
export function MobileSegments({
  options,
  activeValue,
  onChange,
}: {
  options: { value: string; label: string }[];
  activeValue: string;
  onChange: (value: string) => void;
}) {
  return (
    <div className="ios-segment">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className={`ios-segment-btn ${activeValue === option.value ? "ios-segment-btn--active" : ""}`}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

// 浮动操作按钮
export function MobileFab({
  onClick,
  children,
}: {
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className="ios-fab"
      onClick={onClick}
    >
      {children}
    </button>
  );
}
