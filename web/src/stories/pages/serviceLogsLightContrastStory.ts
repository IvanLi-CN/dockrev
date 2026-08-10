import { expectStory, findButton, normalizeText, waitForCondition } from "./storyAssertions";

type Rgb = readonly [number, number, number];

function rgb(value: string, label: string): Rgb {
  const channels = value.match(/[\d.]+/g)?.map(Number) ?? [];
  expectStory(channels.length >= 3, `${label} should resolve to an RGB color, got ${value}`);
  const alpha = channels[3] ?? 1;
  expectStory(alpha >= 0.999, `${label} should be opaque, got ${value}`);
  return [channels[0]!, channels[1]!, channels[2]!];
}

function relativeLuminance([red, green, blue]: Rgb): number {
  const channel = (value: number) => {
    const normalized = value / 255;
    return normalized <= 0.04045 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue);
}

function contrastRatio(foreground: Rgb, background: Rgb): number {
  const lighter = Math.max(relativeLuminance(foreground), relativeLuminance(background));
  const darker = Math.min(relativeLuminance(foreground), relativeLuminance(background));
  return (lighter + 0.05) / (darker + 0.05);
}

function expectReadableText(foreground: HTMLElement, background: HTMLElement, label: string): void {
  const ratio = contrastRatio(
    rgb(getComputedStyle(foreground).color, `${label} foreground`),
    rgb(getComputedStyle(background).backgroundColor, `${label} background`),
  );
  expectStory(ratio >= 4.5, `${label} should meet WCAG AA contrast, got ${ratio.toFixed(2)}:1`);
}

export async function expectLightServiceLogsContrast(canvasElement: HTMLElement): Promise<void> {
  await waitForCondition(() => normalizeText(canvasElement.textContent).includes("实时日志"));
  const doc = canvasElement.ownerDocument;
  const terminal = canvasElement.querySelector<HTMLElement>(".serviceLogsTerminal");
  const terminalHead = canvasElement.querySelector<HTMLElement>(".serviceLogsTerminalHead");
  expectStory(doc.documentElement.dataset.theme === "light", "light contrast story should use light theme tokens");
  expectStory(terminal, "light contrast story should render the service logs terminal");
  expectStory(terminalHead, "light contrast story should render the terminal header");

  rgb(getComputedStyle(terminal!).backgroundColor, "light logs terminal surface");
  rgb(getComputedStyle(terminalHead!).backgroundColor, "light logs terminal header surface");
  expectReadableText(terminalHead!, terminalHead!, "light logs terminal header");

  const humanMessage = canvasElement.querySelector<HTMLElement>(".serviceLogHumanMsg");
  const timestamp = canvasElement.querySelector<HTMLElement>(".serviceLogTs");
  const format = canvasElement.querySelector<HTMLElement>(".serviceLogMetaFormat");
  expectStory(humanMessage, "light contrast story should render a human log message");
  expectStory(timestamp, "light contrast story should render a timestamp");
  expectStory(format, "light contrast story should render a structured format badge");
  expectReadableText(humanMessage!, terminal!, "light human log message");
  expectReadableText(timestamp!, terminal!, "light log timestamp");
  expectReadableText(format!, format!, "light log format badge");

  for (const level of canvasElement.querySelectorAll<HTMLElement>(".serviceLogLevel")) {
    expectReadableText(level, level, `light ${level.dataset.level ?? "unknown"} level badge`);
  }
  for (const chip of canvasElement.querySelectorAll<HTMLElement>(".serviceLogMetaChip")) {
    const key = chip.querySelector<HTMLElement>(".serviceLogMetaKey");
    const value = chip.querySelector<HTMLElement>(".serviceLogMetaValue");
    expectStory(key && value, "light metadata chip should render key and value");
    expectReadableText(key!, chip, "light metadata key");
    expectReadableText(value!, chip, "light metadata value");
  }

  findButton(canvasElement, "Raw")?.click();
  await waitForCondition(() => terminal?.getAttribute("data-service-logs-view") === "raw");
  const defaultRawMessage = Array.from(terminal?.querySelectorAll<HTMLElement>(".serviceLogRow[data-view=\"raw\"] .serviceLogMsg") ?? [])
    .find((message) => normalizeText(message.textContent).includes("serving on :8080"));
  expectStory(defaultRawMessage, "raw log view should render unstyled text");
  expectReadableText(defaultRawMessage!, terminal!, "light raw default log text");

  const ansiSegments = Array.from(terminal?.querySelectorAll<HTMLElement>(".serviceLogMsg span[style]") ?? [])
    .filter((segment) => segment.style.color.length > 0);
  expectStory(ansiSegments.length > 0, "raw log view should render ANSI-colored segments");
  for (const segment of ansiSegments) {
    expectReadableText(segment, terminal!, "light raw ANSI segment");
  }
}
