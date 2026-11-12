import { useCallback, useEffect, useState } from "react";
import Sidebar from "./components/Sidebar";
import {
  getNavItem,
  isNavId,
  formatExtensions,
  type AppPageId,
  type NavId,
} from "./config/nav";
import { useTauriFileDrop } from "./hooks/useTauriFileDrop";
import AboutPage from "./pages/AboutPage";
import ClearMetaPage from "./pages/ClearMetaPage";
import ComingSoonPage from "./pages/ComingSoonPage";
import LicensePage from "./pages/LicensePage";
import {
  classifyDroppedPaths,
  getSupportedModules,
  pickPrimaryModule,
} from "./utils/fileCategory";
import "./App.css";

export default function App() {
  const [active, setActive] = useState<AppPageId>("pdf");
  const [dragActive, setDragActive] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [pendingByModule, setPendingByModule] = useState<
    Partial<Record<NavId, string[]>>
  >({});

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 4500);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const enqueuePaths = useCallback((moduleId: NavId, paths: string[]) => {
    setPendingByModule((prev) => ({
      ...prev,
      [moduleId]: [...new Set([...(prev[moduleId] ?? []), ...paths])],
    }));
  }, []);

  const clearPending = useCallback((moduleId: NavId) => {
    setPendingByModule((prev) => {
      if (!prev[moduleId]) return prev;
      const next = { ...prev };
      delete next[moduleId];
      return next;
    });
  }, []);

  const handleGlobalDrop = useCallback(
    (paths: string[]) => {
      const { byModule, unsupported } = classifyDroppedPaths(paths);
      const modules = getSupportedModules(byModule);
      const notices: string[] = [];

      if (modules.length === 0) {
        if (isNavId(active)) {
          const item = getNavItem(active);
          alert(
            `不支持的文件格式。当前为 ${item.label} 模块，请拖入 ${formatExtensions(item)} 文件`,
          );
        } else {
          alert("不支持的文件格式，请拖入 PDF、Office 或图片文件");
        }
        return;
      }

      if (unsupported.length > 0) {
        notices.push(`${unsupported.length} 个不支持的文件已跳过`);
      }

      if (modules.length === 1) {
        const target = modules[0];
        enqueuePaths(target, byModule[target]!);

        if (isNavId(active) && active !== target) {
          setActive(target);
          notices.unshift(`已自动切换到 ${getNavItem(target).label} 模块`);
        } else if (!isNavId(active)) {
          setActive(target);
          notices.unshift(`已切换到 ${getNavItem(target).label} 模块`);
        }

        if (notices.length > 0) setNotice(notices.join("；"));
        return;
      }

      for (const moduleId of modules) {
        enqueuePaths(moduleId, byModule[moduleId]!);
      }

      const primary = pickPrimaryModule(modules, byModule);
      const moduleLabels = modules.map((id) => getNavItem(id).label).join("、");

      if (!isNavId(active) || active !== primary) {
        setActive(primary);
        notices.unshift(
          `拖入 ${moduleLabels} 多种格式，已切换到 ${getNavItem(primary).label} 模块`,
        );
      } else {
        notices.unshift(`${moduleLabels} 文件已添加到各自模块`);
      }

      setNotice(notices.join("；"));
    },
    [active, enqueuePaths],
  );

  useTauriFileDrop(handleGlobalDrop, setDragActive);

  let content;
  if (active === "about") {
    content = <AboutPage onOpenLicense={() => setActive("license")} />;
  } else if (active === "license") {
    content = <LicensePage />;
  } else {
    const item = getNavItem(active);
    content = item.enabled ? (
      <ClearMetaPage
        key={item.id}
        item={item}
        pendingPaths={pendingByModule[item.id]}
        onPendingConsumed={() => clearPending(item.id)}
      />
    ) : (
      <ComingSoonPage item={item} />
    );
  }

  const activeNavItem = isNavId(active) ? getNavItem(active) : null;

  return (
    <div className="shell">
      <Sidebar active={active} onChange={setActive} />
      <main className="main">
        {notice && <div className="app-notice">{notice}</div>}
        {dragActive && (
          <div className="drag-overlay app-drag-overlay">
            <div className="drag-overlay-inner">
              <div className="drag-overlay-title">松开鼠标添加文件</div>
              <div className="drag-overlay-hint">
                {activeNavItem
                  ? `当前 ${activeNavItem.label} 模块（${formatExtensions(activeNavItem)}）；拖入其他格式将自动切换`
                  : "支持 PDF、Office、图片，将自动识别并切换到对应模块"}
              </div>
            </div>
          </div>
        )}
        {content}
      </main>
    </div>
  );
}
