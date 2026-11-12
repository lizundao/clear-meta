import { NAV_ITEMS, type NavId } from "../config/nav";

export function getExtension(path: string): string {
  return path.split(".").pop()?.toLowerCase() ?? "";
}

export function getModuleForPath(path: string): NavId | null {
  const ext = getExtension(path);
  for (const item of NAV_ITEMS) {
    if (item.extensions.includes(ext)) {
      return item.id;
    }
  }
  return null;
}

export interface ClassifiedDrop {
  byModule: Partial<Record<NavId, string[]>>;
  unsupported: string[];
}

export function classifyDroppedPaths(paths: string[]): ClassifiedDrop {
  const byModule: Partial<Record<NavId, string[]>> = {};
  const unsupported: string[] = [];

  for (const path of paths) {
    const moduleId = getModuleForPath(path);
    if (moduleId) {
      byModule[moduleId] = [...(byModule[moduleId] ?? []), path];
    } else {
      unsupported.push(path);
    }
  }

  return { byModule, unsupported };
}

export function getSupportedModules(
  byModule: Partial<Record<NavId, string[]>>,
): NavId[] {
  return NAV_ITEMS.map((item) => item.id).filter(
    (id) => (byModule[id]?.length ?? 0) > 0,
  );
}

export function pickPrimaryModule(
  modules: NavId[],
  byModule: Partial<Record<NavId, string[]>>,
): NavId {
  return [...modules].sort(
    (a, b) => (byModule[b]?.length ?? 0) - (byModule[a]?.length ?? 0),
  )[0];
}
