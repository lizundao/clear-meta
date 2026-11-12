import { CircleCheck } from "lucide-react";
import { APP_NAME } from "../config/app";
import { LICENSE_ZH, THIRD_PARTY_HINT } from "../content/license";
import "./InfoPages.css";

export default function LicensePage() {
  return (
    <div className="info-page">
      <header className="info-header">
        <h1>许可证</h1>
        <p>软件使用条款与开源许可</p>
      </header>

      <div className="free-banner">
        <span className="free-banner-icon" aria-hidden="true">
          <CircleCheck size={14} strokeWidth={3} />
        </span>
        <div>
          <strong>开源 · MIT 许可</strong>
          <p>{APP_NAME} 按 MIT 许可使用与分发，可自由用于个人与商业场景。</p>
        </div>
      </div>

      <section className="info-card">
        <h2 className="card-section-title">软件许可协议</h2>
        <p className="license-intro">
          {APP_NAME} 由墨点（inkdot）发布，采用 MIT 开源许可。仓库根目录附有英文原版 `LICENSE` 与中文译本 `LICENSE.zh-CN.md`，以下为中文说明，便于阅读。
        </p>
        <pre className="license-text">{LICENSE_ZH}</pre>
      </section>

      <section className="info-card">
        <h2 className="card-section-title">第三方组件</h2>
        <p className="info-hint">{THIRD_PARTY_HINT}</p>
      </section>
    </div>
  );
}
