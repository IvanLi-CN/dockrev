import { CardMetric } from './OverviewPageChrome'

export function HomepageNavSkeleton() {
  return (
    <div className="homepageNavSkeleton" aria-label="正在加载服务入口">
      {Array.from({ length: 3 }).map((_, groupIndex) => (
        <section key={`homepage-skeleton-group-${groupIndex}`} className="homepageDashboardGroup homepageDashboardGroupSkeleton">
          <div className="homepageDashboardGroupHeader">
            <span className="homepageSkeletonLine homepageSkeletonTitle" />
            <span className="homepageSkeletonPill" />
          </div>
          <div className="homepageDashboardStack">
            {Array.from({ length: groupIndex === 0 ? 2 : 1 }).map((__, cardIndex) => (
              <div key={`homepage-skeleton-card-${groupIndex}-${cardIndex}`} className="homepageServiceCard homepageServiceCardSkeleton">
                <div className="homepageServiceCardTop">
                  <span className="homepageServiceIcon homepageSkeletonBlock" />
                  <span className="homepageServiceCardIdentity">
                    <span className="homepageSkeletonLine" />
                    <span className="homepageSkeletonLine homepageSkeletonLineShort" />
                  </span>
                  <span className="homepageServiceDetailButton homepageSkeletonBlock" />
                </div>
                <div className="homepageServiceMetricsGrid">
                  {['CPU', 'MEM', 'RX', 'TX'].map((label) => <CardMetric key={label} value="-" label={label} />)}
                </div>
              </div>
            ))}
          </div>
        </section>
      ))}
    </div>
  )
}
