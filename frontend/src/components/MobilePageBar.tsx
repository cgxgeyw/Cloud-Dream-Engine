import type { ReactNode } from "react";
import { ArrowLeft } from "lucide-react";
import { useLocation, useNavigate } from "react-router-dom";
import { resolveParentPath } from "../lib/routeBack";

type MobilePageBarProps = {
  title: string;
  actions?: ReactNode;
  onBack?: () => void;
};

export function MobilePageBar({ title, actions, onBack }: MobilePageBarProps) {
  const navigate = useNavigate();
  const location = useLocation();

  const handleBack = () => {
    if (onBack) {
      onBack();
      return;
    }
    navigate(resolveParentPath(location.pathname, location.search));
  };

  return (
    <div className="mobile-page-topbar">
      <button type="button" className="mobile-page-back" onClick={handleBack} aria-label="返回">
        <ArrowLeft size={18} />
      </button>
      <div className="mobile-editor-heading">
        <strong className="mobile-editor-title">{title}</strong>
      </div>
      {actions ?? <span className="mobile-editor-more-spacer" aria-hidden="true" />}
    </div>
  );
}
