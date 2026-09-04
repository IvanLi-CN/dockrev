import { ArrowLeft, Home } from 'lucide-react'
import { BrandLogo } from '../BrandLogo'

export function NotFoundView({ pathname, onHome }: { pathname: string; onHome: () => void }) {
  return (
    <main className="notFoundView" data-visual-evidence-surface="not-found">
      <header className="notFoundViewHeader">
        <BrandLogo />
        <p className="notFoundViewHeaderContext">DOCUMENT NOT FOUND</p>
        <span className="notFoundViewStatus">HTTP 404</span>
      </header>
      <section className="notFoundViewSystemState" aria-label="文档状态">
        <div>
          <span>STATUS</span>
          <strong>404</strong>
        </div>
        <div>
          <span>TYPE</span>
          <strong>DOCUMENT</strong>
        </div>
        <div>
          <span>CACHE</span>
          <strong>NO-STORE</strong>
        </div>
      </section>
      <section className="notFoundViewContent" aria-labelledby="not-found-title">
        <aside className="notFoundViewSignalRail" aria-hidden="true">
          <p className="notFoundViewCode">404</p>
          <span className="notFoundViewSignalLabel">DOCUMENT</span>
          <span className="notFoundViewSignalTarget" />
          <span className="notFoundViewSignalGrid" />
        </aside>
        <div className="notFoundViewBody">
          <p className="notFoundViewEyebrow">REQUEST NOT FOUND</p>
          <h1 id="not-found-title">页面未找到</h1>
          <p className="notFoundViewDescription">此地址不属于 Dockrev 的已知页面。</p>
          <div className="notFoundViewPath">
            <span>请求地址</span>
            <span className="notFoundViewPathTrace" aria-hidden="true"><i /><i /></span>
            <code>{pathname}</code>
          </div>
          <div className="notFoundViewActions">
            <button className="btn btnGhost" type="button" onClick={() => window.history.back()}>
              <ArrowLeft size={16} aria-hidden="true" /> 返回
            </button>
            <button className="btn btnPrimary" type="button" onClick={onHome}>
              <Home size={16} aria-hidden="true" /> 首页
            </button>
          </div>
        </div>
      </section>
    </main>
  )
}
