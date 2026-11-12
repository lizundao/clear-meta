export type NavId = "pdf" | "office" | "image";
export type InfoPageId = "about" | "license";
export type AppPageId = NavId | InfoPageId;

export interface NavItem {
  id: NavId;
  label: string;
  description: string;
  extensions: string[];
  enabled: boolean;
}

export interface InfoNavItem {
  id: InfoPageId;
  label: string;
}

export const INFO_NAV_ITEMS: InfoNavItem[] = [
  { id: "about", label: "关于" },
  { id: "license", label: "许可证" },
];

export const NAV_ITEMS: NavItem[] = [
  {
    id: "pdf",
    label: "PDF",
    description: "查看、选择性清除或编辑 PDF 文档元数据",
    extensions: ["pdf"],
    enabled: true,
  },
  {
    id: "office",
    label: "Office",
    description: "管理 Word、Excel、PowerPoint 文档属性，适合投标脱敏",
    extensions: ["docx", "xlsx", "pptx", "docm", "xlsm", "pptm"],
    enabled: true,
  },
  {
    id: "image",
    label: "图片",
    description: "选择性清除 EXIF、GPS、XMP 等图片元数据",
    extensions: ["jpg", "jpeg", "png", "webp"],
    enabled: true,
  },
];

export function getNavItem(id: NavId): NavItem {
  return NAV_ITEMS.find((item) => item.id === id) ?? NAV_ITEMS[0];
}

export function isNavId(id: AppPageId): id is NavId {
  return id === "pdf" || id === "office" || id === "image";
}

export function formatExtensions(item: NavItem): string {
  return item.extensions.map((ext) => `.${ext}`).join("、");
}
