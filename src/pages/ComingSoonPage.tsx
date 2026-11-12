import type { NavItem } from "../config/nav";

interface ComingSoonPageProps {
  item: NavItem;
}

export default function ComingSoonPage({ item }: ComingSoonPageProps) {
  return (
    <div className="coming-soon">
      <div className="coming-soon-icon">{item.label}</div>
      <h2>{item.label} 元数据清理</h2>
      <p>{item.description}</p>
      <div className="coming-soon-badge">即将推出</div>
      <p className="coming-soon-hint">
        计划支持：{item.extensions.map((e) => `.${e}`).join("、")}
      </p>
    </div>
  );
}
