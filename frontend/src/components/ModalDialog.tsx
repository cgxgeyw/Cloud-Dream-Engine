import type { ReactNode } from "react";
import { X } from "lucide-react";

type ModalDialogProps = {
  open: boolean;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
  onClose: () => void;
  maxWidth?: number;
};

type ConfirmDialogProps = {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  confirmVariant?: "default" | "danger";
  confirmDisabled?: boolean;
  onConfirm: () => void;
  onClose: () => void;
};

export function ModalDialog({
  open,
  title,
  children,
  footer,
  onClose,
  maxWidth = 560,
}: ModalDialogProps) {
  if (!open) {
    return null;
  }

  return (
    <div className="modal-root" role="dialog" aria-modal="true" aria-label={title}>
      <button type="button" className="modal-backdrop" aria-label="关闭弹窗" onClick={onClose} />
      <div className="modal-panel" style={{ maxWidth }}>
        <div className="modal-header">
          <div className="modal-title-group">
            <h2 className="modal-title">{title}</h2>
          </div>
          <button type="button" className="modal-close-btn" aria-label="关闭弹窗" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">{children}</div>
        {footer ? <div className="modal-footer">{footer}</div> : null}
      </div>
    </div>
  );
}

export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "确认",
  cancelLabel = "取消",
  confirmVariant = "default",
  confirmDisabled = false,
  onConfirm,
  onClose,
}: ConfirmDialogProps) {
  return (
    <ModalDialog
      open={open}
      title={title}
      onClose={onClose}
      footer={
        <>
          <button type="button" className="action-btn" onClick={onClose}>
            {cancelLabel}
          </button>
          <button
            type="button"
            className={`action-btn${confirmVariant === "danger" ? " action-btn--danger" : " action-btn--accent"}`}
            disabled={confirmDisabled}
            onClick={onConfirm}
          >
            {confirmLabel}
          </button>
        </>
      }
    >
      <p className="modal-description">{description}</p>
    </ModalDialog>
  );
}
