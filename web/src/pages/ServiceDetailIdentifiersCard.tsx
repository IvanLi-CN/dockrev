import type { Service, StackDetail } from '../api'
import { Mono } from '../ui'

export function ServiceDetailIdentifiersCard(props: { service: Service; stack: StackDetail }) {
  const { service, stack } = props
  return (
    <div className="card serviceDetailIdentifiersCard" data-service-detail-section-card="service-identifiers">
      <div className="serviceDetailIdentifiersHead">
        <div>
          <div className="title">服务标识</div>
          <div className="muted">镜像引用与内部标识只在概览保留，避免其余子页重复占用页头空间。</div>
        </div>
      </div>
      <div className="serviceDetailIdentifiersGrid">
        <div className="serviceDetailIdentifierItem"><div className="serviceDetailIdentifierLabel">Image Ref</div><div className="serviceDetailIdentifierValue"><Mono>{service.image.ref}</Mono></div></div>
        <div className="serviceDetailIdentifierItem"><div className="serviceDetailIdentifierLabel">Service ID</div><div className="serviceDetailIdentifierValue"><Mono>{service.id}</Mono></div></div>
        <div className="serviceDetailIdentifierItem"><div className="serviceDetailIdentifierLabel">Stack ID</div><div className="serviceDetailIdentifierValue"><Mono>{stack.id}</Mono></div></div>
      </div>
    </div>
  )
}
