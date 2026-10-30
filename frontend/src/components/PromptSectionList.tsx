import { useEffect, useRef, useState, type PointerEvent } from "react";
import { createPortal } from "react-dom";
import { motion, useMotionValue, useReducedMotion } from "framer-motion";
import { Menu } from "lucide-react";
import "./PromptSectionList.css";

type PromptModule = Record<string, unknown>;
type DragSession = {
  id: string;
  label: string;
  pointerId: number;
  startX: number;
  startY: number;
  offsetX: number;
  offsetY: number;
  width: number;
  listHeight: number;
  slots: { id: string; center: number; top: number; bottom: number }[];
  originalOrder: string[];
  previewOrder: string[];
  moved: boolean;
  valid: boolean;
};

function moveSection(order: string[], id: string, targetIndex: number) {
  const movable = order.filter((key) => key !== id);
  movable.splice(targetIndex, 0, id);
  return movable;
}

export function PromptSectionList({ sections, modules, onReorder, requestTools }: {
  sections: [string, string][];
  modules: Map<string, PromptModule>;
  onReorder: (ids: string[]) => void;
  requestTools?: unknown;
}) {
  const listRef = useRef<HTMLDivElement>(null);
  const dragRef = useRef<DragSession | null>(null);
  const [drag, setDrag] = useState<DragSession | null>(null);
  const [previewOrder, setPreviewOrder] = useState<string[] | null>(null);
  const [announcement, setAnnouncement] = useState("");
  const x = useMotionValue(0);
  const y = useMotionValue(0);
  const reducedMotion = useReducedMotion();
  const labels = new Map(sections);
  const order = previewOrder ?? sections.map(([id]) => id);

  function cancelDrag() {
    dragRef.current = null;
    setDrag(null);
    setPreviewOrder(null);
  }

  useEffect(() => {
    if (!drag) return;
    const previousCursor = document.body.style.cursor;
    document.body.style.cursor = "grabbing";
    const cancel = (event: KeyboardEvent) => {
      if (event.key === "Escape") cancelDrag();
    };
    window.addEventListener("keydown", cancel);
    window.addEventListener("blur", cancelDrag);
    return () => {
      document.body.style.cursor = previousCursor;
      window.removeEventListener("keydown", cancel);
      window.removeEventListener("blur", cancelDrag);
    };
  }, [drag]);

  function startDrag(event: PointerEvent<HTMLButtonElement>, id: string, label: string) {
    if (!event.isPrimary || event.button !== 0 || !listRef.current) return;
    event.preventDefault();
    event.stopPropagation();
    const list = listRef.current;
    const header = event.currentTarget.closest("summary")!.getBoundingClientRect();
    const listBounds = list.getBoundingClientRect();
    const originalOrder = sections.map(([key]) => key);
    dragRef.current = {
      id, label, pointerId: event.pointerId,
      startX: event.clientX, startY: event.clientY,
      offsetX: event.clientX - header.left, offsetY: event.clientY - header.top,
      width: header.width, listHeight: listBounds.height,
      slots: Array.from(list.querySelectorAll<HTMLElement>("[data-prompt-section-id]")).map((row) => {
        const bounds = row.getBoundingClientRect();
        return {
          id: row.dataset.promptSectionId!,
          center: bounds.top - listBounds.top + bounds.height / 2,
          top: bounds.top - listBounds.top,
          bottom: bounds.bottom - listBounds.top,
        };
      }),
      originalOrder, previewOrder: originalOrder, moved: false, valid: true,
    };
    // Capture on the stable list, so reordering rows cannot interrupt the gesture.
    list.setPointerCapture(event.pointerId);
  }

  function updateDrag(event: PointerEvent<HTMLDivElement>) {
    const session = dragRef.current;
    const list = listRef.current;
    if (!session || session.pointerId !== event.pointerId || !list) return;
    if (!session.moved && Math.hypot(event.clientX - session.startX, event.clientY - session.startY) < 4) return;
    if (!session.moved) {
      session.moved = true;
      setDrag(session);
    }
    x.set(event.clientX - session.offsetX);
    const rect = list.getBoundingClientRect();
    const localY = event.clientY - rect.top;
    const overlayLeft = Math.max(rect.left, Math.min(event.clientX - session.offsetX, rect.right - session.width));
    const overlayTop = Math.max(4, Math.min(event.clientY - session.offsetY, window.innerHeight - 48));
    x.set(overlayLeft);
    y.set(overlayTop);
    session.valid = event.clientX >= rect.left && event.clientX <= rect.right
      && localY >= 0 && localY <= session.listHeight;
    if (!session.valid) return;
    const slots = session.slots;
    const targetIndex = slots.reduce((best, slot, index) =>
      Math.abs(localY - slot.center) < Math.abs(localY - slots[best].center) ? index : best, 0);
    const next = moveSection(session.originalOrder, session.id, targetIndex);
    if (next.some((id, index) => id !== session.previewOrder[index])) {
      session.previewOrder = next;
      setPreviewOrder(next);
    }
  }

  function finishDrag(event: PointerEvent<HTMLDivElement>) {
    updateDrag(event);
    const session = dragRef.current;
    if (!session || session.pointerId !== event.pointerId) return;
    if (session.moved && session.valid) {
      onReorder(session.previewOrder);
      setAnnouncement(`${session.label}已移至第 ${session.previewOrder.indexOf(session.id) + 1} 项`);
    }
    cancelDrag();
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  }

  return (
    <>
    <div ref={listRef} className="prompt-section-list"
      onPointerMove={updateDrag} onPointerUp={finishDrag}
      onPointerCancel={cancelDrag} onLostPointerCapture={cancelDrag}>
      {order.map((id) => {
        const label = labels.get(id) ?? id;
        const module = modules.get(id);
        const placeholder = drag?.id === id;
        return (
          <motion.div key={id} layout="position"
            transition={{ layout: { duration: reducedMotion ? 0 : 0.18, ease: [0.22, 1, 0.36, 1] } }}
            className={`prompt-section-row${placeholder ? " prompt-section-row--placeholder" : ""}`}
            data-prompt-section-id={id}>
            {placeholder ? <div className="prompt-section-drop-label">拖动中 · 放置到第 {order.indexOf(id) + 1} 项</div> : null}
            <details className="prompt-section-details" style={{ opacity: module?.sent === false && module?.preview_only !== true ? 0.6 : 1 }}>
              <summary className="prompt-section-summary">
                  <button type="button" className="prompt-preview-grip" aria-label={`拖动排序：${label}`}
                    title="拖动排序，也可用上下方向键调整"
                    onClick={(event) => { event.preventDefault(); event.stopPropagation(); }}
                    onPointerDown={(event) => startDrag(event, id, label)}
                    onKeyDown={(event) => {
                      if (event.key !== "ArrowUp" && event.key !== "ArrowDown") return;
                      event.preventDefault();
                      const nextIndex = order.indexOf(id) + (event.key === "ArrowUp" ? -1 : 1);
                      if (nextIndex < 0 || nextIndex >= order.length) return;
                      const next = moveSection(order, id, nextIndex);
                      onReorder(next);
                      setAnnouncement(`${label}已移至第 ${next.indexOf(id) + 1} 项`);
                    }}><Menu size={17} /></button>
                <span className="prompt-preview-title" style={{ flex: 1 }}>{label}</span>
                <span className="text-muted prompt-preview-title" style={{ fontSize: 12 }}>
                  {module?.preview_only === true ? String(module.preview_label ?? "默认值预览") : module?.sent === false ? "当前未发送" : ""}
                </span>
              </summary>
              <div className="prompt-preview-content" style={{ padding: "0 12px 12px 38px" }}>
                <div className="text-muted prompt-preview-text" style={{ marginBottom: 6, fontSize: 12 }}>{String(module?.source ?? "运行时生成")}</div>
                <pre className="prompt-preview-text" style={{ margin: 0, whiteSpace: "pre-wrap", overflowWrap: "anywhere", fontSize: 12, lineHeight: 1.5 }}>{module ? (typeof module.content === "string" ? module.content : JSON.stringify(module.content ?? "", null, 2)) : "当前预览没有生成此区块。"}</pre>
              </div>
            </details>
          </motion.div>
        );
      })}
      <span className="prompt-section-announcement" role="status" aria-live="polite">{announcement}</span>
      {drag ? createPortal(
        <motion.div className="prompt-section-drag-preview" aria-hidden="true" style={{ x, y, width: drag.width }}>
          <Menu size={17} />
          <span>{drag.label}</span>
          <span className="prompt-section-drag-hint">放置到第 {drag.previewOrder.indexOf(drag.id) + 1} 项</span>
        </motion.div>, document.body,
      ) : null}
    </div>
      <details className="prompt-section-details" style={{ marginTop: 10, borderStyle: "dashed" }}>
        <summary className="prompt-section-summary">
          <span className="text-muted" aria-hidden="true">↳</span>
          <span className="prompt-preview-title" style={{ flex: 1 }}>tools 字段</span>
          <span className="text-muted prompt-preview-title" style={{ fontSize: 12 }}>请求字段 · 不属于 messages</span>
        </summary>
        <div className="prompt-preview-content" style={{ padding: "0 12px 12px 38px" }}>
          <div className="text-muted prompt-preview-text" style={{ marginBottom: 6, fontSize: 12 }}>发送给模型请求的原生工具定义，固定显示在预览底部。</div>
          <pre className="prompt-preview-text" style={{ margin: 0, whiteSpace: "pre-wrap", overflowWrap: "anywhere", fontSize: 12, lineHeight: 1.5 }}>
            {requestTools === undefined ? "当前预览未提供 tools 字段。" : JSON.stringify(requestTools ?? [], null, 2)}
          </pre>
        </div>
      </details>
    </>
  );
}
