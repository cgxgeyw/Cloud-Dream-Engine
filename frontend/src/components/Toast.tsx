import { useCallback, useEffect, useState } from "react";

type ToastItem = {
  id: number;
  message: string;
  variant: "success" | "error";
};

let nextId = 0;
const listeners = new Set<(item: ToastItem) => void>();

export function showToast(message: string, variant: ToastItem["variant"] = "success") {
  const item: ToastItem = { id: nextId++, message, variant };
  for (const fn of listeners) {
    fn(item);
  }
}

export function ToastContainer() {
  const [items, setItems] = useState<ToastItem[]>([]);

  const handleNew = useCallback((item: ToastItem) => {
    setItems((prev) => [...prev, item]);
    window.setTimeout(() => {
      setItems((prev) => prev.filter((current) => current.id !== item.id));
    }, 2400);
  }, []);

  useEffect(() => {
    listeners.add(handleNew);
    return () => {
      listeners.delete(handleNew);
    };
  }, [handleNew]);

  return (
    <div className="toast-stack" aria-live="polite" aria-atomic="true">
      {items.map((item) => (
        <div key={item.id} className={`toast-card toast-card--${item.variant}`}>
          <span className="toast-card-dot" aria-hidden="true" />
          <span className="toast-card-text">{item.message}</span>
        </div>
      ))}
    </div>
  );
}
