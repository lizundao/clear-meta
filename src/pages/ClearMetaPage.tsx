import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  applyMetadata,
  isSupported,
  pickDirectoryDialog,
  pickFilesDialog,
  readMetadata,
  registerPaths,
  restoreFromBackup,
  saveReport,
  scanFolder,
} from "../api";
import MetadataPanel from "../components/MetadataPanel";
import { METADATA_PRESETS } from "../config/presets";
import type { NavItem } from "../config/nav";
import { buildCsvReport, buildJsonReport } from "../utils/report";
import {
  editStatesToRules,
  rulesToEditStates,
} from "../utils/metadataRules";
import type {
  CompareData,
  FieldEditState,
  FieldRule,
  MetadataPreview,
  OutputMode,
  PreviewMode,
  ProcessResult,
  ReportContext,
} from "../types";

const MAX_FILES = 500; // 与后端 safety::MAX_BATCH 保持一致

function basename(path: string): string {
  return path.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? path;
}

function matchesCategory(path: string, extensions: string[]): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return extensions.includes(ext);
}

/** 把后端 StructError 中文文案做二次整理，避免暴露英文原始串。 */
function friendlyMessage(error: unknown): string {
  if (typeof error === "string") {
    const first = error.split("\n")[0].trim();
    if (/path|permission|denied|no such file/i.test(first) && /:\s*\w/.test(first)) {
      return "无法访问所选文件，请重新选择或检查文件是否被占用。";
    }
    return first || "操作未完成，请重试。";
  }
  return "操作未完成，请重试。";
}

interface ClearMetaPageProps {
  item: NavItem;
  pendingPaths?: string[];
  onPendingConsumed?: () => void;
}

export default function ClearMetaPage({
  item,
  pendingPaths,
  onPendingConsumed,
}: ClearMetaPageProps) {
  const [files, setFiles] = useState<string[]>([]);
  const [results, setResults] = useState<ProcessResult[]>([]);
  const [reportContext, setReportContext] = useState<ReportContext | null>(null);
  const [outputMode, setOutputMode] = useState<OutputMode>("copy");
  const [outputDir, setOutputDir] = useState("");
  const [processing, setProcessing] = useState(false);
  const [exporting, setExporting] = useState(false);
  const [selectedFile, setSelectedFile] = useState<string | null>(null);
  const [previewMode, setPreviewMode] = useState<PreviewMode>("edit");
  const [metadataCache, setMetadataCache] = useState<
    Record<string, MetadataPreview>
  >({});
  const [compareCache, setCompareCache] = useState<Record<string, CompareData>>(
    {},
  );
  const [loadingMetadata, setLoadingMetadata] = useState(false);
  const [activeRules, setActiveRules] = useState<FieldRule[]>([]);
  const [clearAll, setClearAll] = useState(false);
  const [activePresetId, setActivePresetId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const noticeTimer = useRef<number | null>(null);

  const showNotice = useCallback((message: string) => {
    setNotice(message);
    if (noticeTimer.current) window.clearTimeout(noticeTimer.current);
    noticeTimer.current = window.setTimeout(() => setNotice(null), 5000);
  }, []);

  useEffect(
    () => () => {
      if (noticeTimer.current) window.clearTimeout(noticeTimer.current);
    },
    [],
  );

  const loadMetadata = useCallback(async (path: string) => {
    setLoadingMetadata(true);
    try {
      const [preview] = await readMetadata([path]);
      if (preview) {
        setMetadataCache((prev) => ({ ...prev, [path]: preview }));
      }
    } catch (error) {
      setMetadataCache((prev) => ({
        ...prev,
        [path]: {
          path,
          success: false,
          message: friendlyMessage(error),
          fields: [],
        },
      }));
    } finally {
      setLoadingMetadata(false);
    }
  }, []);

  const addFiles = useCallback(
    async (paths: string[]) => {
      const supported: string[] = [];
      for (const path of paths) {
        if (!matchesCategory(path, item.extensions)) continue;
        try {
          if (await isSupported(path)) supported.push(path);
        } catch {
          // 不受支持或无法识别的文件直接跳过
        }
      }
      if (supported.length === 0) {
        showNotice("没有可添加的文件，请确认格式为 " + item.extensions.map((e) => "." + e).join(" / "));
        return;
      }

      // 批量上限 + 去重
      let added = 0;
      let skipped = 0;
      setFiles((prev) => {
        const merged = [...new Set([...prev, ...supported])];
        if (merged.length > MAX_FILES) {
          skipped = merged.length - MAX_FILES;
          const trimmed = merged.slice(0, MAX_FILES);
          added = trimmed.length - prev.length;
          return trimmed;
        }
        added = merged.length - prev.length;
        return merged;
      });

      // 登记授权：后端只处理登记过的文件
      try {
        const newly = supported.slice(0, Math.max(0, MAX_FILES));
        await registerPaths(newly);
      } catch (error) {
        showNotice("文件授权失败：" + friendlyMessage(error));
        return;
      }

      setResults([]);
      setReportContext(null);
      setCompareCache({});

      setSelectedFile((current) => {
        const next = current ?? supported[0];
        if (!current && supported[0]) void loadMetadata(supported[0]);
        return next;
      });

      if (skipped > 0) {
        showNotice(`单次最多处理 ${MAX_FILES} 个文件，已添加 ${added} 个，其余 ${skipped} 个请分批添加。`);
      } else if (added > 0) {
        showNotice(`已添加 ${added} 个文件。`);
      }
    },
    [item.extensions, item.label, loadMetadata, showNotice],
  );

  useEffect(() => {
    if (!pendingPaths?.length) return;
    void (async () => {
      await addFiles(pendingPaths);
      onPendingConsumed?.();
    })();
  }, [pendingPaths, addFiles, onPendingConsumed]);

  const selectFile = useCallback(
    async (path: string) => {
      setSelectedFile(path);
      if (!metadataCache[path] && !compareCache[path]) await loadMetadata(path);
    },
    [compareCache, loadMetadata, metadataCache],
  );

  const pickFiles = async () => {
    const paths = await pickFilesDialog(item.label, item.extensions);
    if (paths) await addFiles(paths);
  };

  const pickFolder = async () => {
    const dir = await pickDirectoryDialog();
    if (!dir) return;
    try {
      const paths = await scanFolder(dir, [...item.extensions]);
      if (paths.length === 0) {
        showNotice("该文件夹中没有找到支持的文件");
        return;
      }
      await addFiles(paths);
    } catch (error) {
      showNotice("读取文件夹失败：" + friendlyMessage(error));
    }
  };

  const pickOutputDir = async () => {
    const dir = await pickDirectoryDialog();
    if (dir) setOutputDir(dir);
  };

  const selectedPreview = selectedFile ? metadataCache[selectedFile] : null;
  const selectedCompare = selectedFile ? compareCache[selectedFile] : null;
  const previewFields = selectedPreview?.success ? selectedPreview.fields : [];

  const editStates = useMemo(
    () => rulesToEditStates(previewFields, activeRules),
    [previewFields, activeRules],
  );

  const updateRulesFromStates = useCallback(
    (nextStates: Record<string, FieldEditState>) => {
      setClearAll(false);
      setActivePresetId(null);
      setActiveRules(editStatesToRules(previewFields, nextStates));
    },
    [previewFields],
  );

  const handleEditChange = useCallback(
    (key: string, patch: Partial<FieldEditState>) => {
      const next = {
        ...editStates,
        [key]: { ...editStates[key], ...patch },
      };
      updateRulesFromStates(next);
    },
    [editStates, updateRulesFromStates],
  );

  const handleEditBatchChange = useCallback(
    (next: Record<string, FieldEditState>) => updateRulesFromStates(next),
    [updateRulesFromStates],
  );

  const applyPreset = useCallback((presetId: string) => {
    const preset = METADATA_PRESETS.find((p) => p.id === presetId);
    if (!preset) return;
    setActivePresetId(presetId);
    if (preset.clearAll) {
      setClearAll(true);
      setActiveRules([]);
    } else {
      setClearAll(false);
      setActiveRules(preset.rules);
    }
    setPreviewMode("edit");
  }, []);

  const handleProcess = async () => {
    if (files.length === 0 || processing) return;
    if (outputMode === "copy" && !outputDir) {
      showNotice("请先选择输出目录");
      return;
    }
    if (outputMode === "in_place") {
      const ok = window.confirm(
        "将直接覆盖原文件。虽然会自动备份以便撤销，仍建议优先选择「另存到新目录」。确定继续吗？",
      );
      if (!ok) return;
    } else if (!clearAll && activeRules.length === 0) {
      const ok = window.confirm("当前未设置任何清除或编辑规则，是否继续？");
      if (!ok) return;
    }

    setProcessing(true);
    try {
      const res = await applyMetadata(files, {
        output_mode: outputMode,
        output_dir: outputMode === "copy" ? outputDir : undefined,
        rules: activeRules,
        clear_all: clearAll,
      });
      setResults(res);

      const nextCompare: Record<string, CompareData> = {};
      for (const result of res) {
        if (result.success && result.after_fields) {
          nextCompare[result.path] = {
            before: result.before_fields,
            after: result.after_fields,
            beforeCount: result.before_count,
            afterCount: result.after_count ?? 0,
          };
        }
      }
      setCompareCache(nextCompare);
      setReportContext({
        module: item.label,
        outputMode,
        outputDir: outputMode === "copy" ? outputDir : undefined,
        processedAt: new Date().toISOString(),
      });
      setPreviewMode("compare");

      const failed = res.filter((r) => !r.success).length;
      if (failed > 0) {
        showNotice(`处理完成：成功 ${res.length - failed} 个，失败 ${failed} 个。`);
      } else {
        showNotice(`已处理 ${res.length} 个文件。`);
      }
    } catch (error) {
      setResults([
        {
          path: "处理失败",
          success: false,
          message: friendlyMessage(error),
          before_count: 0,
          before_fields: [],
          backup_path: undefined,
        },
      ]);
      showNotice("处理未完成：" + friendlyMessage(error));
    } finally {
      setProcessing(false);
    }
  };

  const handleUndo = async (result: ProcessResult) => {
    if (!result.backup_path) {
      showNotice("此结果没有可用备份，无法撤销。");
      return;
    }
    try {
      await restoreFromBackup(result.backup_path, result.path);
      setResults((prev) => prev.filter((r) => r !== result));
      setCompareCache((prev) => {
        const next = { ...prev };
        delete next[result.path];
        return next;
      });
      showNotice(`已还原：${basename(result.path)}`);
    } catch (error) {
      showNotice("撤销失败：" + friendlyMessage(error));
    }
  };

  const exportReport = async (format: "csv" | "json") => {
    if (results.length === 0 || !reportContext) return;
    setExporting(true);
    try {
      const content =
        format === "csv"
          ? buildCsvReport(results, reportContext)
          : buildJsonReport(results, reportContext);
      const stamp = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
      const fileName = `metadata-report-${stamp}.${format}`;
      const saved = await saveReport(fileName, content);
      showNotice(`报告已保存到：${saved}`);
    } catch (error) {
      showNotice("导出报告失败：" + friendlyMessage(error));
    } finally {
      setExporting(false);
    }
  };

  const removeFile = (path: string) => {
    // 基于函数式更新的 prev 计算下一选中项，避免闭包里的旧 files。
    setFiles((prev) => {
      const remaining = prev.filter((f) => f !== path);
      setSelectedFile((cur) => (cur === path ? remaining[0] ?? null : cur));
      return remaining;
    });
    setMetadataCache((prev) => {
      const next = { ...prev };
      delete next[path];
      return next;
    });
    setCompareCache((prev) => {
      const next = { ...prev };
      delete next[path];
      return next;
    });
  };

  const clearAllFiles = () => {
    setFiles([]);
    setResults([]);
    setReportContext(null);
    setSelectedFile(null);
    setMetadataCache({});
    setCompareCache({});
    setActiveRules([]);
    setClearAll(false);
    setActivePresetId(null);
  };

  const successCount = useMemo(
    () => results.filter((item) => item.success).length,
    [results],
  );

  return (
    <div className="page">
      {notice && <div className="app-notice">{notice}</div>}
      <header className="page-header">
        <div>
          <h1>{item.label} 元数据</h1>
          <p>{item.description}</p>
        </div>
        <div className="format-tags">
          {item.extensions.map((ext) => (
            <span key={ext} className="format-tag">.{ext}</span>
          ))}
        </div>
      </header>

      <div className="page-body">
        <section className="panel panel-left">
          <div className="dropzone" onClick={() => void pickFiles()}>
            <div className="dropzone-icon">+</div>
            <div className="dropzone-title">点击选择或拖拽文件到窗口</div>
            <div className="dropzone-hint">文件会添加到当前 {item.label} 模块（最多 {MAX_FILES} 个）</div>
          </div>

          <div className="pick-row">
            <button type="button" className="btn-secondary btn-small" onClick={() => void pickFolder()}>
              选择文件夹（批量）
            </button>
          </div>

          <div className="preset-section">
            <div className="section-title">快速预设</div>
            <div className="preset-row">
              {METADATA_PRESETS.map((preset) => (
                <button
                  key={preset.id}
                  type="button"
                  className={`preset-chip ${activePresetId === preset.id ? "active" : ""}`}
                  title={preset.description}
                  onClick={() => applyPreset(preset.id)}
                >
                  {preset.label}
                </button>
              ))}
            </div>
            <p className="hint-text preset-hint">
              规则按字段匹配、应用于所有文件；未命中的字段保持原样。
            </p>
          </div>

          <div className="options compact">
            <div className="option-group">
              <label>输出方式</label>
              <div className="radio-row">
                <label className="radio-item">
                  <input
                    type="radio"
                    name={`output-${item.id}`}
                    checked={outputMode === "copy"}
                    onChange={() => setOutputMode("copy")}
                  />
                  另存到新目录
                </label>
                <label className="radio-item">
                  <input
                    type="radio"
                    name={`output-${item.id}`}
                    checked={outputMode === "in_place"}
                    onChange={() => setOutputMode("in_place")}
                  />
                  覆盖原文件（自动备份）
                </label>
              </div>
            </div>

            {outputMode === "copy" && (
              <div className="option-group">
                <label>输出目录</label>
                <div className="inline-row">
                  <button type="button" className="btn-secondary btn-small" onClick={() => void pickOutputDir()}>
                    选择目录
                  </button>
                  <span className="hint-text">{outputDir || "未选择"}</span>
                </div>
              </div>
            )}
          </div>

          <div className="file-section">
            <div className="section-title">
              文件列表
              {files.length > 0 && <span className="count-badge">{files.length}</span>}
            </div>

            {files.length > 0 ? (
              <div className="file-list">
                {files.map((path) => (
                  <div
                    key={path}
                    className={`file-item ${selectedFile === path ? "selected" : ""}`}
                    onClick={() => void selectFile(path)}
                  >
                    <div className="file-item-main">
                      <span className="file-name">{basename(path)}</span>
                      {compareCache[path] && (
                        <span className="file-badge">已处理</span>
                      )}
                    </div>
                    <button
                      type="button"
                      className="btn-ghost file-remove"
                      onClick={(e) => {
                        e.stopPropagation();
                        removeFile(path);
                      }}
                    >
                      移除
                    </button>
                  </div>
                ))}
              </div>
            ) : (
              <div className="empty-block">尚未添加文件</div>
            )}
          </div>

          <div className="action-row">
            <button
              type="button"
              className="btn-primary"
              disabled={processing || files.length === 0}
              onClick={() => void handleProcess()}
            >
              {processing ? "处理中..." : "应用更改"}
            </button>
            <button
              type="button"
              className="btn-secondary"
              disabled={files.length === 0 || processing}
              onClick={clearAllFiles}
            >
              清空
            </button>
          </div>

          {results.length > 0 && (
            <section className="results compact">
              <div className="section-title row-title">
                <span>
                  处理结果
                  <span className="count-badge">{successCount}/{results.length}</span>
                </span>
                <div className="inline-row">
                  <button
                    type="button"
                    className="btn-secondary btn-small"
                    disabled={exporting}
                    onClick={() => void exportReport("csv")}
                  >
                    导出 CSV
                  </button>
                  <button
                    type="button"
                    className="btn-secondary btn-small"
                    disabled={exporting}
                    onClick={() => void exportReport("json")}
                  >
                    导出 JSON
                  </button>
                </div>
              </div>
              {results.map((result) => (
                <div
                  key={`${result.path}-${result.message}`}
                  className={`result-item ${result.success ? "success" : "error"}`}
                >
                  <div className="result-path">{basename(result.path)}</div>
                  <div>{result.message}</div>
                  {result.success && result.after_count !== undefined && (
                    <div className="result-meta">
                      {result.before_count} 项 → {result.after_count} 项
                    </div>
                  )}
                  {result.success && result.backup_path && (
                    <button
                      type="button"
                      className="btn-ghost result-undo"
                      onClick={() => void handleUndo(result)}
                    >
                      撤销（还原原文件）
                    </button>
                  )}
                </div>
              ))}
            </section>
          )}
        </section>

        <section className="panel panel-right">
          {!selectedFile ? (
            <>
              <div className="section-title">元数据编辑</div>
              <div className="empty-block tall">选择左侧文件查看并编辑元数据</div>
            </>
          ) : (
            <MetadataPanel
              fileName={basename(selectedFile)}
              loading={loadingMetadata}
              preview={selectedPreview?.success ? selectedPreview.fields : null}
              previewError={
                selectedPreview && !selectedPreview.success
                  ? selectedPreview.message
                  : undefined
              }
              compare={selectedCompare}
              mode={previewMode}
              onModeChange={setPreviewMode}
              onRefresh={selectedCompare ? undefined : () => void loadMetadata(selectedFile)}
              refreshing={loadingMetadata}
              editStates={editStates}
              onEditChange={handleEditChange}
              onEditBatchChange={handleEditBatchChange}
            />
          )}
        </section>
      </div>
    </div>
  );
}
