import { ArrowLeft, Home } from 'lucide-react'
import { navigate, type Route } from '../routes'

export function NotFoundView({ pathname, onHome }: { pathname: string; onHome?: () => void }) {
  return (
    <main className="notFoundView" data-visual-evidence-surface="not-found">
      <div className="notFoundViewCard">
        <p className="notFoundViewCode">404</p>
        <h1>页面不存在</h1>
        <p className="notFoundViewPath">{pathname}</p>
        <div className="notFoundViewActions">
          <button type="button" onClick={() => window.history.back()}>
            <ArrowLeft size={16} aria-hidden="true" /> 返回
          </button>
          <button type="button" onClick={onHome ?? (() => navigate({ name: 'overview' }))}>
            <Home size={16} aria-hidden="true" /> 首页
          </button>
        </div>
      </div>
    </main>
  )
}

export const notFoundRoute = (pathname: string): Route => ({ name: 'not-found', pathname })
