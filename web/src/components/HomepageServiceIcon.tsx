import apps from "@iconify-icons/mdi/apps";
import { Icon } from "@iconify/react";
import { useMemo, useState } from "react";

import { apiBaseUrl } from "../api";

const DEFAULT_MONOCHROME_ICON_COLOR = "#dbeafe";
const MAX_STORED_FAILED_ICONS = 80;

const failedIconSources = new Set<string>();

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

function apiIconUrl(path: string): string {
  const base = apiBaseUrl().replace(/\/$/, "");
  return `${base}${path}`;
}

function buildIconifyUrl(
  collection: string,
  name: string,
  color?: string,
): string {
  const resolvedColor = color ?? DEFAULT_MONOCHROME_ICON_COLOR;
  const params = new URLSearchParams({ color: resolvedColor });
  return apiIconUrl(
    `/api/homepage-icons/iconify/${collection}/${encodeURIComponent(name)}.svg?${params.toString()}`,
  );
}

function buildSelfhStUrl(spec: string): string {
  const trimmed = spec.trim().slice(3);
  const match = trimmed.match(/^(.*)\.(svg|png|webp)$/i);
  const name = match ? match[1] : trimmed;
  const ext = match ? match[2].toLowerCase() : "png";
  return apiIconUrl(
    `/api/homepage-icons/selfhst/${ext}/${encodeURIComponent(name)}.${ext}`,
  );
}

function buildDashboardIconUrl(spec: string): string {
  const trimmed = spec.trim();
  const match = trimmed.match(/^(.*)\.(svg|png|webp)$/i);
  const name = match ? match[1] : trimmed;
  const ext = match ? match[2].toLowerCase() : "svg";
  return apiIconUrl(
    `/api/homepage-icons/dashboard/${ext}/${encodeURIComponent(name)}.${ext}`,
  );
}

function isKnownFailedIconSource(src: string): boolean {
  return failedIconSources.has(src);
}

function rememberFailedIconSource(src: string) {
  failedIconSources.add(src);
  while (failedIconSources.size > MAX_STORED_FAILED_ICONS) {
    const oldest = failedIconSources.values().next().value;
    if (!oldest) break;
    failedIconSources.delete(oldest);
  }
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
  const shouldRenderImage =
    Boolean(source.src) &&
    failedSrc !== source.src &&
    !isKnownFailedIconSource(source.src ?? "");

  return (
    <span
      className="homepageServiceIcon"
      data-icon-src={source.src}
      data-icon-kind={source.kind}
      title={props.title}
    >
      {shouldRenderImage ? (
        <img
          alt=""
          aria-hidden="true"
          className="homepageServiceIconImage"
          decoding="async"
          fetchPriority="low"
          loading="lazy"
          onError={() => {
            if (source.src) rememberFailedIconSource(source.src);
            setFailedSrc(source.src ?? null);
          }}
          referrerPolicy="no-referrer"
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
