import { CircleCheck } from "lucide-react";
import {
  APP_AUTHOR,
  APP_BRAND,
  APP_DESCRIPTION,
  APP_HOMEPAGE,
  APP_NAME,
  APP_TAGLINE,
  APP_VERSION,
} from "../config/app";
import { openExternalUrl } from "../utils/openUrl";
import "./InfoPages.css";

interface AboutPageProps {
  onOpenLicense: () => void;
}

export default function AboutPage({ onOpenLicense }: AboutPageProps) {
  return (
    <div className="info-page">
      <header className="info-header">
        <h1>关于</h1>
        <p>版本信息、作者与使用引导</p>
      </header>

      <div className="free-banner">
        <span className="free-banner-icon" aria-hidden="true">
          <CircleCheck size={14} strokeWidth={3} />
        </span>
        <div>
          <strong>离线运行，数据留在本地</strong>
          <p>无需账号、无需联网，文件与处理记录都保存在本机。由 inkdot 提供。</p>
        </div>
      </div>

      <section className="info-card">
        <h2 className="card-section-title">ⓘ 关于</h2>
        <div className="about-brand">
          <img
            src="/inkdot.png"
            alt="墨点 inkdot 网站 Logo"
            className="about-logo"
            width={56}
            height={56}
          />
          <div>
            <div className="about-brand-name">墨点 inkdot</div>
            <div className="about-brand-tagline">{APP_TAGLINE}</div>
          </div>
        </div>

        <dl className="meta-table">
          <div className="meta-row">
            <dt>应用名称</dt>
            <dd>
              {APP_NAME}
              <span className="badge-free">开源</span>
            </dd>
          </div>
          <div className="meta-row">
            <dt>版本</dt>
            <dd>{APP_VERSION}</dd>
          </div>
          <div className="meta-row">
            <dt>品牌</dt>
            <dd>{APP_BRAND}</dd>
          </div>
          <div className="meta-row">
            <dt>作者</dt>
            <dd>{APP_AUTHOR}</dd>
          </div>
          <div className="meta-row">
            <dt>网址</dt>
            <dd>
              <button
                type="button"
                className="ext-link"
                onClick={() => void openExternalUrl(APP_HOMEPAGE)}
              >
                inkdot.cn
              </button>
            </dd>
          </div>
          <div className="meta-row">
            <dt>许可</dt>
            <dd>
              MIT 开源许可，详见{" "}
              <button type="button" className="link-btn" onClick={onOpenLicense}>
                「许可证」
              </button>
            </dd>
          </div>
          <div className="meta-row">
            <dt>定位</dt>
            <dd>{APP_DESCRIPTION}</dd>
          </div>
        </dl>
      </section>
    </div>
  );
}
