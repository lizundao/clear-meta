import type { FieldRule } from "../types";

export interface MetadataPreset {
  id: string;
  label: string;
  description: string;
  rules: FieldRule[];
  clearAll?: boolean;
}

export const METADATA_PRESETS: MetadataPreset[] = [
  {
    id: "tender",
    label: "投标脱敏",
    description: "清除作者、修改者、应用信息等，避免标书被关联为同一人",
    rules: [
      { key: "core:creator", action: "remove" },
      { key: "core:lastModifiedBy", action: "remove" },
      { key: "core:revision", action: "remove" },
      { key: "app:Application", action: "remove" },
      { key: "app:AppVersion", action: "remove" },
      { key: "app:Company", action: "remove" },
      { key: "app:Manager", action: "remove" },
      { key: "app:Template", action: "remove" },
      { key: "custom:*", action: "remove" },
      { key: "office:customXml", action: "remove" },
      { key: "info:Author", action: "remove" },
      { key: "info:Creator", action: "remove" },
      { key: "info:Producer", action: "remove" },
      { key: "info:CreationDate", action: "remove" },
      { key: "info:ModDate", action: "remove" },
      { key: "xmp:creator", action: "remove" },
      { key: "xmp:CreatorTool", action: "remove" },
      { key: "xmp:Producer", action: "remove" },
      { key: "xmp:CreateDate", action: "remove" },
      { key: "xmp:ModifyDate", action: "remove" },
      { key: "xmp:MetadataDate", action: "remove" },
    ],
  },
  {
    id: "privacy",
    label: "隐私清除",
    description: "清除个人身份、GPS 与软件痕迹",
    rules: [
      { key: "core:creator", action: "remove" },
      { key: "core:lastModifiedBy", action: "remove" },
      { key: "app:Application", action: "remove" },
      { key: "app:Company", action: "remove" },
      { key: "info:Author", action: "remove" },
      { key: "xmp:creator", action: "remove" },
      { key: "gps:*", action: "remove" },
      { key: "exif:Artist", action: "remove" },
      { key: "exif:Software", action: "remove" },
      { key: "png:*", action: "remove" },
    ],
  },
  {
    id: "clear_all",
    label: "全部清除",
    description: "移除所有可编辑元数据",
    rules: [],
    clearAll: true,
  },
];
