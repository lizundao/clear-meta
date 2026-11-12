import type { MetadataField, ProcessResult, ReportContext } from "../types";

export function fieldKey(field: MetadataField): string {
  return `${field.group}::${field.name}`;
}

export function buildCompareRows(before: MetadataField[], after: MetadataField[]) {
  const afterMap = new Map(after.map((f) => [fieldKey(f), f.value]));
  const beforeMap = new Map(before.map((f) => [fieldKey(f), f]));

  const rows: Array<{
    group: string;
    name: string;
    before?: string;
    after?: string;
    status: "removed" | "remaining" | "modified";
  }> = [];

  for (const field of before) {
    const key = fieldKey(field);
    const afterValue = afterMap.get(key);
    if (afterValue === undefined) {
      rows.push({
        group: field.group,
        name: field.name,
        before: field.value,
        status: "removed",
      });
    } else if (afterValue === field.value) {
      rows.push({
        group: field.group,
        name: field.name,
        before: field.value,
        after: afterValue,
        status: "remaining",
      });
    } else {
      rows.push({
        group: field.group,
        name: field.name,
        before: field.value,
        after: afterValue,
        status: "modified",
      });
    }
  }

  for (const field of after) {
    const key = fieldKey(field);
    if (!beforeMap.has(key)) {
      rows.push({
        group: field.group,
        name: field.name,
        after: field.value,
        status: "remaining",
      });
    }
  }

  return rows;
}

function csvEscape(value: string): string {
  if (/[",\n\r]/.test(value)) {
    return `"${value.replace(/"/g, '""')}"`;
  }
  return value;
}

function basename(path: string): string {
  return path.replace(/[/\\]+$/, "").split(/[/\\]/).pop() ?? path;
}

export function buildCsvReport(
  results: ProcessResult[],
  context: ReportContext,
): string {
  const lines: string[] = [
    `# 元数据管理 处理报告`,
    `# 模块,${csvEscape(context.module)}`,
    `# 输出方式,${context.outputMode === "copy" ? "另存到新目录" : "覆盖原文件"}`,
    `# 处理时间,${context.processedAt}`,
  ];

  if (context.outputDir) {
    lines.push(`# 输出目录,${csvEscape(context.outputDir)}`);
  }

  lines.push("");
  lines.push(
    [
      "文件名",
      "原路径",
      "输出路径",
      "状态",
      "清除前项数",
      "清除后项数",
      "已清除项数",
      "消息",
    ].join(","),
  );

  for (const result of results) {
    const removed = result.success
      ? result.before_count - (result.after_count ?? 0)
      : 0;
    lines.push(
      [
        csvEscape(basename(result.path)),
        csvEscape(result.path),
        csvEscape(result.output_path ?? ""),
        csvEscape(result.success ? "成功" : "失败"),
        String(result.before_count),
        String(result.after_count ?? ""),
        String(removed),
        csvEscape(result.message),
      ].join(","),
    );
  }

  lines.push("");
  lines.push("字段明细");
  lines.push(
    ["文件名", "分组", "字段", "清除前", "清除后", "变化"].join(","),
  );

  for (const result of results) {
    if (!result.success || !result.after_fields) continue;
    const rows = buildCompareRows(result.before_fields, result.after_fields);
    for (const row of rows) {
      const change =
        row.status === "removed"
          ? "已清除"
          : row.status === "remaining"
            ? "仍存在"
            : "已修改";
      lines.push(
        [
          csvEscape(basename(result.path)),
          csvEscape(row.group),
          csvEscape(row.name),
          csvEscape(row.before ?? ""),
          csvEscape(row.after ?? ""),
          csvEscape(change),
        ].join(","),
      );
    }
  }

  return lines.join("\n");
}

export function buildJsonReport(
  results: ProcessResult[],
  context: ReportContext,
): string {
  const payload = {
    ...context,
    summary: {
      total: results.length,
      success: results.filter((r) => r.success).length,
      failed: results.filter((r) => !r.success).length,
      removed_fields: results
        .filter((r) => r.success)
        .reduce(
          (sum, r) => sum + (r.before_count - (r.after_count ?? 0)),
          0,
        ),
    },
    results: results.map((result) => ({
      ...result,
      compare: result.after_fields
        ? buildCompareRows(result.before_fields, result.after_fields)
        : [],
    })),
  };

  return JSON.stringify(payload, null, 2);
}
