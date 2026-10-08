/** 小问号提示：把字段说明折叠进标题后面的圆形按钮里，hover / 点击展开气泡。 */

import { useEffect, useRef, useState } from "react";

type HelpTipProps = {
  /** 说明文案（同时作为无障碍标签）。 */
  text: string;
};

export function HelpTip({ text }: HelpTipProps) {
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLSpanElement>(null);

  useEffect(() => {
    if (!open) return;

    const handleOutside = (event: MouseEvent | TouchEvent) => {
      if (!wrapRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };

    document.addEventListener("mousedown", handleOutside);
    document.addEventListener("touchstart", handleOutside);
    document.addEventListener("keydown", handleKey);
    return () => {
      document.removeEventListener("mousedown", handleOutside);
      document.removeEventListener("touchstart", handleOutside);
      document.removeEventListener("keydown", handleKey);
    };
  }, [open]);

  return (
    <span className="help-tip" ref={wrapRef}>
      <button
        type="button"
        className="help-tip__btn"
        aria-label={text}
        aria-expanded={open}
        // 在 label 内部时必须阻止默认行为，否则会聚焦输入框 / 误触开关。
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          setOpen((current) => !current);
        }}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
      >
        ?
      </button>
      {open ? (
        <span role="tooltip" className="help-tip__bubble">
          {text}
        </span>
      ) : null}
    </span>
  );
}
