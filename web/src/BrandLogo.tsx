import { brandLogoDarkUrl, brandLogoLightUrl } from './publicAssetUrls'

export function BrandLogo() {
  return (
    <span className="brandLogoThemeSwitch" role="img" aria-label="Dockrev">
      <img className="brandLogo brandLogoForDark" src={brandLogoDarkUrl} alt="" aria-hidden="true" />
      <img className="brandLogo brandLogoForLight" src={brandLogoLightUrl} alt="" aria-hidden="true" />
    </span>
  )
}
