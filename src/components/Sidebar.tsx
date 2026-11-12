import { FileText, Files, Image, Info, ScrollText } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { AppPageId, InfoPageId, NavId } from "../config/nav";
import { INFO_NAV_ITEMS, NAV_ITEMS } from "../config/nav";
import ThemeToggle from "./ThemeToggle";
import "./Sidebar.css";

interface SidebarProps {
  active: AppPageId;
  onChange: (id: AppPageId) => void;
}

const NAV_ICONS: Record<NavId, LucideIcon> = {
  pdf: FileText,
  office: Files,
  image: Image,
};

const INFO_ICONS: Record<InfoPageId, LucideIcon> = {
  about: Info,
  license: ScrollText,
};

export default function Sidebar({ active, onChange }: SidebarProps) {
  return (
    <aside className="sidebar">
      <div className="sidebar-brand">
        <img src="/icon.svg" alt="元数据管理" className="sidebar-logo" />
        <div className="sidebar-brand-content">
          <div className="sidebar-brand-top">
            <div className="sidebar-title">元数据管理</div>
            <ThemeToggle />
          </div>
          <div className="sidebar-subtitle">清除 · 编辑 · 脱敏</div>
        </div>
      </div>

      <nav className="sidebar-nav">
        <div className="sidebar-nav-label">功能模块</div>
        {NAV_ITEMS.map((item) => {
          const Icon = NAV_ICONS[item.id];
          return (
            <button
              key={item.id}
              type="button"
              className={`nav-item ${active === item.id ? "active" : ""} ${!item.enabled ? "disabled" : ""}`}
              disabled={!item.enabled}
              onClick={() => item.enabled && onChange(item.id)}
            >
              <span className="nav-icon">
                <Icon size={18} strokeWidth={2} aria-hidden />
              </span>
              <span className="nav-text">
                <span className="nav-label">{item.label}</span>
                {!item.enabled && <span className="nav-badge">即将推出</span>}
              </span>
            </button>
          );
        })}

        <div className="sidebar-nav-divider" />

        <div className="sidebar-nav-label">关于</div>
        {INFO_NAV_ITEMS.map((item) => {
          const Icon = INFO_ICONS[item.id];
          return (
            <button
              key={item.id}
              type="button"
              className={`nav-item ${active === item.id ? "active" : ""}`}
              onClick={() => onChange(item.id)}
            >
              <span className="nav-icon">
                <Icon size={18} strokeWidth={2} aria-hidden />
              </span>
              <span className="nav-text">
                <span className="nav-label">{item.label}</span>
              </span>
            </button>
          );
        })}
      </nav>

      <div className="sidebar-footer">拖拽文件到窗口即可添加</div>
    </aside>
  );
}
