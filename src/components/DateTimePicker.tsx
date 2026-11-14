import { useEffect, useState } from "react";

/** "2026-09-14 09:50:53" ↔ datetime-local 需要的 "2026-09-14T09:50:53" */
function toInputValue(value: string): string {
  const m = value.match(/(\d{4})[-/](\d{1,2})[-/](\d{1,2})[ T](\d{1,2}):(\d{2})(?::(\d{2}))?/);
  if (!m) return "";
  const [, y, mo, d, h, mi, s] = m;
  const pad = (v: string) => v.padStart(2, "0");
  return `${y}-${pad(mo)}-${pad(d)}T${pad(h)}:${mi}:${pad(s ?? "00")}`;
}

function fromInputValue(value: string): string {
  // datetime-local 给的是 2026-09-14T09:50:53
  if (!value) return "";
  return value.replace("T", " ").slice(0, 19);
}

export function nowText(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

export interface QuickTime {
  label: string;
  value: string;
}

interface DateTimePickerProps {
  title: string;
  initial: string;
  quick: QuickTime[];
  onCancel: () => void;
  onConfirm: (value: string) => void;
}

/** 时间字段专用弹窗：原生日期时间选择器 + 常用时间快捷项。 */
export default function DateTimePicker({
  title,
  initial,
  quick,
  onCancel,
  onConfirm,
}: DateTimePickerProps) {
  const [text, setText] = useState(toInputValue(initial));

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onCancel]);

  return (
    <div className="modal-backdrop" role="presentation" onClick={onCancel}>
      <div
        className="modal-card"
        role="dialog"
        aria-modal="true"
        aria-label={title}
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="modal-title">{title}</h3>

        <input
          type="datetime-local"
          className="modal-datetime"
          step={1}
          value={text}
          onChange={(e) => setText(e.target.value)}
        />

        {quick.length > 0 && (
          <div className="modal-quick">
            {quick.map((q) => (
              <button
                key={q.label}
                type="button"
                className="btn-secondary btn-small"
                onClick={() => setText(toInputValue(q.value))}
              >
                {q.label}
                <span className="quick-value">{q.value.slice(0, 16)}</span>
              </button>
            ))}
          </div>
        )}

        <p className="modal-hint">
          会按文件格式自动转换（PDF 存 D:20260914095053+08'00'，Office 存 UTC）。
        </p>

        <div className="modal-actions">
          <button type="button" className="btn-secondary btn-small" onClick={onCancel}>
            取消
          </button>
          <button
            type="button"
            className="btn-primary btn-small"
            disabled={!text}
            onClick={() => onConfirm(fromInputValue(text))}
          >
            确定
          </button>
        </div>
      </div>
    </div>
  );
}
