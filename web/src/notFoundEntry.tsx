import { createRoot } from 'react-dom/client'
import './index.css'
import './App.css'
import { NotFoundView } from './components/NotFoundView'

const pathname = window.location.pathname
const homeHref = import.meta.env.BASE_URL
createRoot(document.getElementById('root')!).render(
  <NotFoundView pathname={pathname} onHome={() => window.location.assign(homeHref)} />,
)
