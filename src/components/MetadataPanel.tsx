import MetadataEditor from "./MetadataEditor";
import type { CompareData, FieldEditState, MetadataField, PreviewMode } from "../types";
import { buildCompareRows, FS_GROUP } from "../utils/report";

interface MetadataPanelProps {
  fileName: string;
  loading: boolean;
  preview?: MetadataField[] | null;
  previewError?: string;
  compare?: CompareData | null;
  mode: PreviewMode;
  onModeChange: (mode: PreviewMode) => void;
  onRefresh?: () => void;
  refreshing?: boolean;
  editStates?: Record<string, FieldEditState>;
  onEditChange?: (key: string, patch: Partial<FieldEditState>) => void;
  onEditBatchChange?: (next: Record<string, FieldEditState>) => void;
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

function FieldTable({ fields }: { fields: MetadataField[] }) {
  const grouped = groupFields(fields);
  if (grouped.length === 0) {
    return <div className="empty-block">未检测到元数据</div>;
  }

  return (
    <div className="metadata-groups">
      {grouped.map(([group, items]) => (
        <div key={group} className="metadata-group">
          <h3>
            {group}
            {group === FS_GROUP && (
              <span className="fs-note">
                来自文件系统，即资源管理器右键属性里的时间；不属于文档内容，清除时不会被改动
              </span>
            )}
          </h3>
          <table className="metadata-table">
            <tbody>
              {items.map((field) => (
                <tr key={field.key}>
                  <th>{field.name}</th>
                  <td>
                    {field.value}
                    {field.raw && (
                      <span className="field-raw" title="文件里实际存的值">
                        原文 {field.raw}
                      </span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      ))}
    </div>
  );
}

export default function MetadataPanel({
  fileName,
  loading,
  preview,
  previewError,
  compare,
  mode,
  onModeChange,
  onRefresh,
  refreshing,
  editStates,
  onEditChange,
  onEditBatchChange,
}: MetadataPanelProps) {
  const showCompare = !!compare;
  const showEdit = !showCompare && !!preview && preview.length > 0;
  // 统计与对比只针对文档元数据；文件属性（资源管理器时间）仅展示，不参与计数。
  const beforeMetaCount = compare
    ? compare.before.filter((f) => f.group !== FS_GROUP).length
    : 0;
  const afterMetaCount = compare
    ? compare.after.filter((f) => f.group !== FS_GROUP).length
    : 0;

  return (
    <>
      <div className="section-title row-title">
        <span>
          {showCompare ? "元数据对比" : showEdit ? "元数据编辑" : "元数据预览"}
        </span>
        {!showCompare && onRefresh && (
          <button
            type="button"
            className="btn-secondary btn-small"
            disabled={refreshing}
            onClick={onRefresh}
          >
            {refreshing ? "读取中..." : "刷新"}
          </button>
        )}
      </div>

      <div className="preview-file-name">{fileName}</div>

      {showCompare && (
        <>
          <div className="compare-stats">
            <span className="stat-chip before">处理前 {beforeMetaCount} 项</span>
            <span className="stat-arrow">→</span>
            <span className="stat-chip after">处理后 {afterMetaCount} 项</span>
            <span className="stat-chip removed">
              已变更 {beforeMetaCount - afterMetaCount} 项
            </span>
          </div>

          <div className="preview-tabs">
            {(["compare", "before", "after"] as PreviewMode[]).map((tab) => (
              <button
                key={tab}
                type="button"
                className={`preview-tab ${mode === tab ? "active" : ""}`}
                onClick={() => onModeChange(tab)}
              >
                {tab === "compare" ? "对比" : tab === "before" ? "处理前" : "处理后"}
              </button>
            ))}
          </div>
        </>
      )}

      {showEdit && (
        <div className="preview-tabs">
          <button
            type="button"
            className={`preview-tab ${mode === "edit" ? "active" : ""}`}
            onClick={() => onModeChange("edit")}
          >
            编辑
          </button>
          <button
            type="button"
            className={`preview-tab ${mode === "before" ? "active" : ""}`}
            onClick={() => onModeChange("before")}
          >
            只读预览
          </button>
        </div>
      )}

      {loading && !preview && !compare && (
        <div className="empty-block">正在读取元数据...</div>
      )}

      {previewError && !compare && (
        <div className="result-item error">{previewError}</div>
      )}

      {!loading && !compare && preview && preview.length === 0 && !previewError && (
        <div className="empty-block">未检测到元数据</div>
      )}

      {showEdit && mode === "edit" && editStates && onEditChange && onEditBatchChange && (
        <MetadataEditor
          fields={preview}
          states={editStates}
          onChange={onEditChange}
          onBatchChange={onEditBatchChange}
        />
      )}

      {!compare && preview && preview.length > 0 && mode === "before" && (
        <FieldTable fields={preview} />
      )}

      {compare && mode === "before" && <FieldTable fields={compare.before} />}

      {compare && mode === "after" && (
        afterMetaCount > 0 ? (
          <FieldTable fields={compare.after} />
        ) : (
          <div className="empty-block success-block">元数据已全部清除</div>
        )
      )}

      {compare && mode === "compare" && (
        <div className="compare-table-wrap">
          <CompareTable
            before={compare.before.filter((f) => f.group !== FS_GROUP)}
            after={compare.after.filter((f) => f.group !== FS_GROUP)}
          />
        </div>
      )}
    </>
  );
}

function CompareTable({
  before,
  after,
}: {
  before: MetadataField[];
  after: MetadataField[];
}) {
  const rows = buildCompareRows(before, after);

  if (rows.length === 0) {
    return <div className="empty-block">无元数据变化</div>;
  }

  return (
    <table className="metadata-table compare-table">
      <thead>
        <tr>
          <th>分组</th>
          <th>字段</th>
          <th>处理前</th>
          <th>处理后</th>
          <th>状态</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr key={`${row.group}-${row.name}`} className={`compare-row-${row.status}`}>
            <td>{row.group}</td>
            <td>{row.name}</td>
            <td>{row.before ?? "—"}</td>
            <td>{row.after ?? "—"}</td>
            <td>
              <span className={`compare-badge ${row.status}`}>
                {row.status === "removed"
                  ? "已清除"
                  : row.status === "remaining"
                    ? "仍存在"
                    : "已修改"}
              </span>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
