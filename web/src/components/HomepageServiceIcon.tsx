import apps from "@iconify-icons/mdi/apps";
import { Icon } from "@iconify/react";
import { useMemo, useState } from "react";

function isAbsoluteUrl(value: string): boolean {
  return /^https?:\/\//i.test(value);
}

function splitIconColor(raw: string): { name: string; color?: string } {
  const marker = raw.lastIndexOf("-#");
  if (marker < 0) return { name: raw };
  return {
    name: raw.slice(0, marker),
    color: raw.slice(marker + 1),
  };
}

function buildIconifyUrl(
  collection: string,
  name: string,
  color?: string,
): string {
  const base = `https://api.iconify.design/${collection}/${name}.svg`;
  return color ? `${base}?color=${encodeURIComponent(color)}` : base;
}

function buildSelfhStUrl(spec: string): string {
  const trimmed = spec.trim().slice(3);
  const match = trimmed.match(/^(.*)\.(svg|png|webp)$/i);
  const name = match ? match[1] : trimmed;
  const ext = match ? match[2].toLowerCase() : "png";
  return `https://cdn.jsdelivr.net/gh/selfhst/icons/${ext}/${name}.${ext}`;
}

function buildDashboardIconUrl(spec: string): string {
  const trimmed = spec.trim();
  const match = trimmed.match(/^(.*)\.(svg|png|webp)$/i);
  const name = match ? match[1] : trimmed;
  const ext = match ? match[2].toLowerCase() : "svg";
  return `https://cdn.jsdelivr.net/gh/homarr-labs/dashboard-icons/${ext}/${name}.${ext}`;
}

export function resolveHomepageIconSource(icon: string | null | undefined): {
  kind: "url" | "mdi" | "si" | "sh" | "dashboard" | "fallback";
  src?: string;
} {
  const trimmed = (icon ?? "").trim();
  if (!trimmed) return { kind: "fallback" };
  if (isAbsoluteUrl(trimmed) || trimmed.startsWith("/"))
    return { kind: "url", src: trimmed };
  if (trimmed.startsWith("mdi-")) {
    const { name, color } = splitIconColor(trimmed.slice(4));
    return { kind: "mdi", src: buildIconifyUrl("mdi", name, color) };
  }
  if (trimmed.startsWith("si-")) {
    const { name, color } = splitIconColor(trimmed.slice(3));
    return { kind: "si", src: buildIconifyUrl("simple-icons", name, color) };
  }
  if (trimmed.startsWith("sh-"))
    return { kind: "sh", src: buildSelfhStUrl(trimmed) };
  if (
    /^[^/]+\.(svg|png|webp)$/i.test(trimmed) ||
    /^[a-z0-9][a-z0-9._-]*$/i.test(trimmed)
  ) {
    return { kind: "dashboard", src: buildDashboardIconUrl(trimmed) };
  }
  return { kind: "fallback" };
}

export function HomepageServiceIcon(props: {
  icon: string | null | undefined;
  title: string;
}) {
  const source = useMemo(
    () => resolveHomepageIconSource(props.icon),
    [props.icon],
  );
  const [failedSrc, setFailedSrc] = useState<string | null>(null);
  const shouldRenderImage = Boolean(source.src) && failedSrc !== source.src;

  return (
    <span
      className="homepageServiceIcon"
      data-icon-kind={source.kind}
      title={props.title}
    >
      {shouldRenderImage ? (
        <img
          alt=""
          aria-hidden="true"
          className="homepageServiceIconImage"
          loading="lazy"
          onError={() => setFailedSrc(source.src ?? null)}
          src={source.src}
        />
      ) : (
        <Icon
          aria-hidden="true"
          className="homepageServiceIconFallback"
          icon={apps}
        />
      )}
    </span>
  );
}
