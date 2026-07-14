import { useEffect, useId, useRef, type ReactNode } from "react";
import { X } from "lucide-react";

const FOCUSABLE_SELECTOR = [
  "a[href]",
  "button:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "textarea:not([disabled])",
  "[contenteditable='true']",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

function getFocusableElements(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (element) => !element.hidden && element.getAttribute("aria-hidden") !== "true",
  );
}

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
  const titleId = useId();
  const panelRef = useRef<HTMLDivElement | null>(null);
  const previouslyFocusedRef = useRef<HTMLElement | null>(null);
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    if (!open) {
      return;
    }

    previouslyFocusedRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;
    const panel = panelRef.current;
    const initialFocus = panel
      ? panel.querySelector<HTMLElement>("[autofocus]")
        ?? getFocusableElements(panel)[0]
        ?? panel
      : null;
    initialFocus?.focus({ preventScroll: true });

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") {
        event.preventDefault();
        onCloseRef.current();
        return;
      }
      if (event.key !== "Tab" || !panel) {
        return;
      }

      const focusableElements = getFocusableElements(panel);
      if (focusableElements.length === 0) {
        event.preventDefault();
        panel.focus({ preventScroll: true });
        return;
      }

      const first = focusableElements[0];
      const last = focusableElements[focusableElements.length - 1];
      const activeElement = document.activeElement;
      if (event.shiftKey && (activeElement === first || !panel.contains(activeElement))) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && (activeElement === last || !panel.contains(activeElement))) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      const previouslyFocused = previouslyFocusedRef.current;
      if (previouslyFocused?.isConnected) {
        previouslyFocused.focus({ preventScroll: true });
      }
      previouslyFocusedRef.current = null;
    };
  }, [open]);

  if (!open) {
    return null;
  }

  return (
    <div className="modal-root" role="dialog" aria-modal="true" aria-labelledby={titleId}>
      <button type="button" className="modal-backdrop" aria-label="关闭弹窗" onClick={onClose} />
      <div ref={panelRef} className="modal-panel" style={{ maxWidth }} tabIndex={-1}>
        <div className="modal-header">
          <div className="modal-title-group">
            <h2 id={titleId} className="modal-title">{title}</h2>
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
