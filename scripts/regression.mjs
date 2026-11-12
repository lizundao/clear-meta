/**
 * 前端纯函数回归：用 esbuild 把 TS 模块打包成临时 ESM，再用 Node 跑断言。
 * 覆盖字段规则/通配匹配、处理报告对比与 CSV 转义、预设完整性、文案合规扫描。
 */
import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { readFileSync as readSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { build } from "esbuild";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const tempDir = await mkdtemp(path.join(tmpdir(), "clear-meta-regression-"));
const outFile = path.join(tempDir, "bundle.mjs");

const entry = `
export * as rulesLib from "./src/utils/metadataRules.ts";
export * as reportLib from "./src/utils/report.ts";
export { METADATA_PRESETS } from "./src/config/presets.ts";
`;

let passed = 0;
function check(name, fn) {
  fn();
  passed += 1;
  console.log(`  ok  ${name}`);
}

function field(key, group, name, value, editable = true) {
  return { key, group, name, value, editable };
}

try {
  await build({
    stdin: {
      contents: entry,
      resolveDir: root,
      loader: "ts",
      sourcefile: "regression-entry.ts",
    },
    bundle: true,
    platform: "node",
    format: "esm",
    outfile: outFile,
    logLevel: "silent",
  });

  const m = await import(pathToFileURL(outFile).href);
  const { rulesLib, reportLib, METADATA_PRESETS } = m;

  console.log("字段规则与通配匹配:");
  check("编辑状态转换为规则：keep 不生成规则", () => {
    const fields = [field("core:creator", "文档属性", "作者", "张三")];
    const rules = rulesLib.editStatesToRules(fields, {
      "core:creator": { action: "keep", value: "张三" },
    });
    assert.equal(rules.length, 0);
  });

  check("remove / set 正确生成规则", () => {
    const fields = [
      field("core:creator", "文档属性", "作者", "张三"),
      field("core:title", "文档属性", "标题", "旧"),
    ];
    const rules = rulesLib.editStatesToRules(fields, {
      "core:creator": { action: "remove", value: "张三" },
      "core:title": { action: "set", value: "新标题" },
    });
    assert.equal(rules.length, 2);
    assert.deepEqual(rules[1], { key: "core:title", action: "set", value: "新标题" });
  });

  check("只读字段不会被编辑规则覆盖", () => {
    const fields = [field("doc:pageCount", "文档信息", "页数", "3", false)];
    const rules = rulesLib.editStatesToRules(fields, {
      "doc:pageCount": { action: "remove", value: "3" },
    });
    assert.equal(rules.length, 0);
  });

  check("通配前缀规则 key:* 命中并回填编辑态", () => {
    const fields = [
      field("custom:A", "自定义属性", "A", "1"),
      field("custom:B", "自定义属性", "B", "2"),
      field("core:creator", "文档属性", "作者", "张三"),
    ];
    const states = rulesLib.rulesToEditStates(fields, [
      { key: "custom:*", action: "remove" },
    ]);
    assert.equal(states["custom:A"].action, "remove");
    assert.equal(states["custom:B"].action, "remove");
    assert.equal(states["core:creator"].action, "keep");
  });

  check("规则回填保留原值，set 用规则值", () => {
    const fields = [field("core:title", "文档属性", "标题", "旧")];
    const states = rulesLib.rulesToEditStates(fields, [
      { key: "core:title", action: "set", value: "新" },
    ]);
    assert.equal(states["core:title"].value, "新");
  });

  console.log("报告对比与导出:");
  check("对比行区分 removed / modified / remaining", () => {
    const before = [
      field("core:creator", "文档属性", "作者", "张三"),
      field("core:title", "文档属性", "标题", "旧"),
      field("core:subject", "文档属性", "主题", "保留"),
    ];
    const after = [
      field("core:title", "文档属性", "标题", "新"),
      field("core:subject", "文档属性", "主题", "保留"),
    ];
    const rows = reportLib.buildCompareRows(before, after);
    const byName = Object.fromEntries(rows.map((r) => [r.name, r.status]));
    assert.equal(byName["作者"], "removed");
    assert.equal(byName["标题"], "modified");
    assert.equal(byName["主题"], "remaining");
  });

  check("after 中新增字段也被计入", () => {
    const rows = reportLib.buildCompareRows(
      [],
      [field("xmp:title", "XMP", "标题", "新")],
    );
    assert.equal(rows.length, 1);
    assert.equal(rows[0].status, "remaining");
  });

  check("CSV 正确转义逗号、引号与换行", () => {
    const ctx = {
      module: "PDF",
      outputMode: "copy",
      processedAt: "2026-11-12T00:00:00Z",
    };
    const results = [
      {
        path: "C:\\a,b\\报告\"1\".pdf",
        success: true,
        message: "已清除 2 项\n第二行",
        before_count: 2,
        after_count: 0,
        before_fields: [field("core:creator", "文档属性", "作者", "张三")],
        after_fields: [],
      },
    ];
    const csv = reportLib.buildCsvReport(results, ctx);
    assert.ok(csv.includes('"C:\\a,b\\报告""1"".pdf"'));
    assert.ok(csv.includes('"已清除 2 项\n第二行"'));
  });

  check("JSON 报告汇总成功/失败/清除项数", () => {
    const ctx = { module: "图片", outputMode: "in_place", processedAt: "t" };
    const results = [
      {
        path: "a.png", success: true, message: "ok", before_count: 3, after_count: 1,
        before_fields: [field("gps:GPSLatitude", "GPS", "纬度", "39.9")],
        after_fields: [],
      },
      { path: "b.png", success: false, message: "损坏", before_count: 0, before_fields: [] },
    ];
    const json = JSON.parse(reportLib.buildJsonReport(results, ctx));
    assert.equal(json.summary.total, 2);
    assert.equal(json.summary.success, 1);
    assert.equal(json.summary.failed, 1);
    assert.equal(json.summary.removed_fields, 2);
  });

  console.log("预设完整性:");
  check("三个预设均存在且脱敏/隐私含关键规则", () => {
    const ids = METADATA_PRESETS.map((p) => p.id);
    assert.deepEqual(ids, ["tender", "privacy", "clear_all"]);
    const privacy = METADATA_PRESETS.find((p) => p.id === "privacy");
    assert.ok(privacy.rules.some((r) => r.key === "gps:*"));
    const clearAll = METADATA_PRESETS.find((p) => p.id === "clear_all");
    assert.equal(clearAll.clearAll, true);
  });

  check("投标脱敏覆盖作者与自定义属性", () => {
    const tender = METADATA_PRESETS.find((p) => p.id === "tender");
    const keys = tender.rules.map((r) => r.key);
    assert.ok(keys.includes("core:creator"));
    assert.ok(keys.includes("core:lastModifiedBy"));
    assert.ok(keys.includes("custom:*"));
    assert.ok(keys.includes("office:customXml"));
  });

  console.log("文案合规扫描:");
  check("全仓源码与文档无违规/绝对化措辞", () => {
    const banned = /永久免费|完全免费|免费提供|对齐\s*PDF|复用\s*PDF|按\s*PDF\s*口径/g;
    const files = [
      "README.md",
      "docs/产品介绍.md",
      "src/pages/AboutPage.tsx",
      "src/pages/LicensePage.tsx",
      "src/content/license.ts",
      "src/pages/ClearMetaPage.tsx",
    ];
    for (const rel of files) {
      const text = readSync(path.join(root, rel), "utf8");
      const hit = text.match(banned);
      assert.equal(hit, null, `${rel} 命中违规词: ${hit?.join(",")}`);
    }
  });

  check("根目录含英文 MIT LICENSE 与中文译本", () => {
    const en = readSync(path.join(root, "LICENSE"), "utf8");
    const zh = readSync(path.join(root, "LICENSE.zh-CN.md"), "utf8");
    assert.ok(en.includes("MIT License"));
    assert.ok(en.includes("Permission is hereby granted"));
    assert.ok(zh.includes("MIT 开源许可"));
  });
} finally {
  await rm(tempDir, { recursive: true, force: true });
}

console.log(`\n全部通过：${passed} 项`);
