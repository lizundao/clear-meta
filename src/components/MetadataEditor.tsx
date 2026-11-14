import { useState } from "react";
import { Pencil, X } from "lucide-react";
import type { FieldAction, FieldEditState, MetadataField } from "../types";
import { defaultEditState } from "../utils/metadataRules";
import DateTimePicker, { nowText } from "./DateTimePicker";

interface MetadataEditorProps {
  fields: MetadataField[];
  states: Record<string, FieldEditState>;
  onChange: (key: string, patch: Partial<FieldEditState>) => void;
  onBatchChange: (next: Record<string, FieldEditState>) => void;
}

// 时间字段写回时必须用对应格式，编辑框给出格式提示，避免把展示文本写进文件。
const DATE_KEYS = new Set([
  "info:CreationDate",
  "info:ModDate",
  "xmp:CreateDate",
  "xmp:ModifyDate",
  "xmp:MetadataDate",
  "core:created",
  "core:modified",
]);

const FS_GROUP = "文件属性";

function isDateField(field: MetadataField) {
  return DATE_KEYS.has(field.key);
}

function groupFields(fields: MetadataField[]) {
  const groups = new Map<string, MetadataField[]>();
  for (const field of fields) {
    const list = groups.get(field.group) ?? [];
    list.push(field);
    groups.set(field.group, list);
  }
  return Array.from(groups.entries());
}

function FieldActionMark({ action }: { action: FieldAction }) {
  if (action === "remove") {
    return (
      <span className="field-mark field-mark-remove" title="将清除">
        <X size={14} strokeWidth={2.5} aria-hidden />
      </span>
    );
  }

  if (action === "set") {
    return (
      <span className="field-mark field-mark-edit" title="将编辑">
        <Pencil size={12} strokeWidth={2.5} aria-hidden />
      </span>
    );
  }

  return null;
}

export default function MetadataEditor({
  fields,
  states,
  onChange,
  onBatchChange,
}: MetadataEditorProps) {
  // 正在选择时间的字段 key（弹窗只对时间字段出现）
  const [pickerKey, setPickerKey] = useState<string | null>(null);
  const grouped = groupFields(fields);

  if (grouped.length === 0) {
    return <div className="empty-block">未检测到元数据</div>;
  }

  const setGroupAction = (group: string, action: FieldAction) => {
    const next = { ...states };
    for (const field of fields) {
      if (field.group !== group || field.editable === false) continue;
      next[field.key] = {
        action,
        value: states[field.key]?.value ?? field.value,
      };
    }
    onBatchChange(next);
  };

  return (
    <div className="metadata-editor">
      {grouped.map(([group, items]) => (
        <div key={group} className="metadata-group">
          <div className="metadata-group-header">
            <h3>{group}</h3>
            <div className="group-actions">
              <button
                type="button"
                className="btn-ghost btn-tiny"
                onClick={() => setGroupAction(group, "keep")}
              >
                全保留
              </button>
              <button
                type="button"
                className="btn-ghost btn-tiny"
                onClick={() => setGroupAction(group, "remove")}
              >
                全清除
              </button>
            </div>
          </div>
          <table className="metadata-table edit-table">
            <thead>
              <tr>
                <th>字段</th>
                <th>当前值</th>
                <th>操作</th>
                <th>新值</th>
              </tr>
            </thead>
            <tbody>
              {items.map((field) => {
                const state = states[field.key] ?? defaultEditState(field);
                const readonly = field.editable === false;

                return (
                  <tr
                    key={field.key}
                    className={`edit-row edit-row-${state.action}${readonly ? " readonly" : ""}`}
                  >
                    <td>
                      <span className="field-name-cell">
                        <span>{field.name}</span>
                        {!readonly && <FieldActionMark action={state.action} />}
                      </span>
                    </td>
                    <td className="mono-cell">
                      {field.value}
                      {field.raw && <span className="field-raw">原文 {field.raw}</span>}
                    </td>
                    <td>
                      {readonly ? (
                        <span className="readonly-tag">只读</span>
                      ) : (
                        <select
                          className="action-select"
                          value={state.action}
                          onChange={(e) =>
                            onChange(field.key, {
                              action: e.target.value as FieldAction,
                            })
                          }
                        >
                          <option value="keep">保留</option>
                          <option value="remove">清除</option>
                          <option value="set">编辑</option>
                        </select>
                      )}
                    </td>
                    <td>
                      {readonly || state.action !== "set" ? (
                        <span className="muted-text">—</span>
                      ) : isDateField(field) ? (
                        <button
                          type="button"
                          className="date-pick"
                          onClick={() => setPickerKey(field.key)}
                        >
                          <span className="date-pick-value">
                            {state.value || "选择时间"}
                          </span>
                          <span className="date-pick-action">选择</span>
                        </button>
                      ) : (
                        <input
                          type="text"
                          className="edit-input"
                          value={state.value}
                          onChange={(e) =>
                            onChange(field.key, { value: e.target.value })
                          }
                          placeholder="输入新值"
                        />
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      ))}

      {(() => {
        const target = fields.find((f) => f.key === pickerKey);
        if (!target) return null;
        const fsTime = (name: string) =>
          fields.find((f) => f.group === FS_GROUP && f.name === name)?.value ?? "";
        const quick = [
          { label: "用属性·修改时间", value: fsTime("修改时间") },
          { label: "用属性·创建时间", value: fsTime("创建时间") },
          { label: "当前时间", value: nowText() },
        ].filter((q) => q.value);
        return (
          <DateTimePicker
            title={`选择${target.name}`}
            initial={states[target.key]?.value ?? target.value}
            quick={quick}
            onCancel={() => setPickerKey(null)}
            onConfirm={(value) => {
              onChange(target.key, { action: "set", value });
              setPickerKey(null);
            }}
          />
        );
      })()}
    </div>
  );
}
