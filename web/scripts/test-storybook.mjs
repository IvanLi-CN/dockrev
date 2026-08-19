import { access, readFile } from "node:fs/promises";
import http from "node:http";
import net from "node:net";
import path from "node:path";
import { fileURLToPath } from "node:url";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_OUTDIR = path.resolve(SCRIPT_DIR, "../storybook-static");
const DEFAULT_PORT = 50887;

function parsePort(value, fallback) {
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : fallback;
}

async function findAvailablePort(preferredPort) {
  return await new Promise((resolve, reject) => {
    const probe = net.createServer();
    let retriedWithRandomPort = false;

    const handleError = (error) => {
      if (
        !retriedWithRandomPort &&
        preferredPort !== 0 &&
        error &&
        typeof error === "object" &&
        error.code === "EADDRINUSE"
      ) {
        retriedWithRandomPort = true;
        probe.listen(0, "127.0.0.1");
        return;
      }
      reject(error);
    };

    probe.on("error", handleError);
    probe.listen(preferredPort, "127.0.0.1", () => {
      const address = probe.address();
      const port =
        typeof address === "object" && address ? address.port : preferredPort;
      probe.close((closeError) => {
        if (closeError) reject(closeError);
        else resolve(port);
      });
    });
  });
}

function parseArgs(argv) {
  const out = { url: null, passthrough: [] };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--url") {
      out.url = argv[i + 1] ?? null;
      i++;
      continue;
    }
    out.passthrough.push(a);
  }
  return out;
}

function selectSmokeShard(storyIds) {
  const raw = process.env.DOCKREV_TEST_STORYBOOK_SHARD;
  if (!raw) return storyIds;

  const match = /^(\d+)\/(\d+)$/.exec(raw);
  if (!match) {
    throw new Error(
      "DOCKREV_TEST_STORYBOOK_SHARD must use the one-based index/total form, for example 1/4.",
    );
  }
  const index = Number(match[1]);
  const total = Number(match[2]);
  if (!Number.isInteger(index) || !Number.isInteger(total) || index < 1 || index > total) {
    throw new Error(
      "DOCKREV_TEST_STORYBOOK_SHARD must use a one-based index within its total, for example 1/4.",
    );
  }

  return storyIds.filter((_, storyIndex) => storyIndex % total === index - 1);
}

function contentType(filePath) {
  const ext = path.extname(filePath).toLowerCase();
  if (ext === ".html") return "text/html; charset=utf-8";
  if (ext === ".js" || ext === ".mjs") return "text/javascript; charset=utf-8";
  if (ext === ".css") return "text/css; charset=utf-8";
  if (ext === ".json") return "application/json; charset=utf-8";
  if (ext === ".svg") return "image/svg+xml";
  if (ext === ".png") return "image/png";
  if (ext === ".jpg" || ext === ".jpeg") return "image/jpeg";
  if (ext === ".woff") return "font/woff";
  if (ext === ".woff2") return "font/woff2";
  return "application/octet-stream";
}

async function waitForHttpOk(url, timeoutMs = 60_000) {
  const startedAt = Date.now();
  while (Date.now() - startedAt < timeoutMs) {
    try {
      const resp = await fetch(url, { method: "GET" });
      if (resp.ok) return;
    } catch {
      // ignore until timeout
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

async function ensureStaticBuild() {
  try {
    await access(path.join(DEFAULT_OUTDIR, "index.html"));
    await access(path.join(DEFAULT_OUTDIR, "iframe.html"));
    return;
  } catch {
    console.error(
      "Missing storybook-static build. Run: bun run build-storybook",
    );
    process.exit(1);
  }
}

function startStaticServer({ port }) {
  const sockets = new Set();
  const server = http.createServer(async (req, res) => {
    const reqUrl = new URL(
      req.url ?? "/",
      `http://${req.headers.host ?? "127.0.0.1"}`,
    );
    const pathname = reqUrl.pathname === "/" ? "/index.html" : reqUrl.pathname;
    const filePath = path.resolve(DEFAULT_OUTDIR, `.${pathname}`);
    if (!filePath.startsWith(DEFAULT_OUTDIR)) {
      res.statusCode = 403;
      res.end("Forbidden");
      return;
    }

    try {
      const body = await readFile(filePath);
      res.statusCode = 200;
      res.setHeader("Content-Type", contentType(filePath));
      res.end(body);
    } catch {
      res.statusCode = 404;
      res.end("Not found");
    }
  });

  server.on("connection", (socket) => {
    sockets.add(socket);
    socket.on("close", () => sockets.delete(socket));
  });

  const listen = () =>
    new Promise((resolve, reject) => {
      const onError = (err) => {
        server.off("error", onError);
        reject(err);
      };
      server.on("error", onError);
      server.listen(port, "127.0.0.1", () => {
        server.off("error", onError);
        resolve();
      });
    });

  const cleanup = () =>
    new Promise((resolve) => {
      for (const s of sockets) s.destroy();
      server.close(() => resolve());
    });

  return { listen, cleanup };
}

async function getStoryIds(baseUrl) {
  const base = normalizeBaseUrl(baseUrl);
  const resp = await fetch(new URL("index.json", base));
  if (!resp.ok) {
    throw new Error(
      `Failed to fetch Storybook index.json: ${resp.status} ${resp.statusText}`,
    );
  }
  const json = await resp.json();
  const entries = (json && typeof json === "object" && json.entries) || {};
  if (!entries || typeof entries !== "object") return [];
  return Object.values(entries)
    .filter(
      (e) =>
        e &&
        typeof e === "object" &&
        e.type === "story" &&
        typeof e.id === "string",
    )
    .map((e) => e.id);
}

function normalizeBaseUrl(input) {
  const url = new URL(input);
  url.search = "";
  url.hash = "";
  if (
    url.pathname.endsWith("/iframe.html") ||
    url.pathname.endsWith("/index.html")
  ) {
    url.pathname = url.pathname.replace(/[^/]+$/, "");
  }
  if (!url.pathname.endsWith("/")) url.pathname += "/";
  return url.toString();
}

function approxEqual(a, b, tolerancePx = 1) {
  return Math.abs(a - b) <= tolerancePx;
}

async function requireBoundingBox(locator, label) {
  const box = await locator.boundingBox();
  if (!box) throw new Error(`Missing bounding box: ${label}`);
  return box;
}

function getModal(page) {
  return page.locator('[role="alertdialog"]:visible, [role="dialog"]:visible').first();
}

async function assertServiceLogsLightContrast({ baseUrl, browser }) {
  const page = await browser.newPage();
  try {
    const base = normalizeBaseUrl(baseUrl);
    const url = new URL("iframe.html", base);
    url.searchParams.set("id", "pages-servicedetailpage--logs-section-light-contrast");
    url.searchParams.set("viewMode", "story");
    await page.goto(url.toString(), { waitUntil: "domcontentloaded" });
    await page.locator(".serviceLogsTerminal").waitFor({ timeout: 10_000 });
    await page.getByRole("button", { name: "Human" }).click();
    await page.locator(".serviceLogHumanMsg").first().waitFor({ timeout: 10_000 });

    const assertPairs = async (mode) => {
      await page.evaluate((expectedMode) => {
        const parseRgb = (value, label) => {
          const channels = value.match(/[\d.]+/g)?.map(Number) ?? [];
          if (channels.length < 3) {
            throw new Error(`${label} should resolve to an RGB color, got ${value}`);
          }
          if ((channels[3] ?? 1) < 0.999) {
            throw new Error(`${label} should be opaque, got ${value}`);
          }
          return channels;
        };
        const relativeLuminance = ([red, green, blue]) => {
          const channel = (value) => {
            const normalized = value / 255;
            return normalized <= 0.04045
              ? normalized / 12.92
              : ((normalized + 0.055) / 1.055) ** 2.4;
          };
          return 0.2126 * channel(red) + 0.7152 * channel(green) + 0.0722 * channel(blue);
        };
        const expectReadableText = (foreground, background, label) => {
          const fg = parseRgb(getComputedStyle(foreground).color, `${label} foreground`);
          const bg = parseRgb(getComputedStyle(background).backgroundColor, `${label} background`);
          const ratio =
            (Math.max(relativeLuminance(fg), relativeLuminance(bg)) + 0.05) /
            (Math.min(relativeLuminance(fg), relativeLuminance(bg)) + 0.05);
          if (ratio < 4.5) {
            throw new Error(`${label} should meet WCAG AA contrast, got ${ratio.toFixed(2)}:1`);
          }
        };
        const terminal = document.querySelector(".serviceLogsTerminal");
        const terminalHead = document.querySelector(".serviceLogsTerminalHead");
        if (!terminal || !terminalHead) throw new Error("Light service logs terminal is missing.");
        if (document.documentElement.dataset.theme !== "light") {
          throw new Error("Light service logs story did not apply light theme tokens.");
        }
        parseRgb(getComputedStyle(terminal).backgroundColor, "light logs terminal surface");
        parseRgb(getComputedStyle(terminalHead).backgroundColor, "light logs terminal header surface");
        expectReadableText(terminalHead, terminalHead, "light logs terminal header");
        if (expectedMode === "human") {
          const humanMessage = document.querySelector(".serviceLogHumanMsg");
          const timestamp = document.querySelector(".serviceLogTs");
          const format = document.querySelector(".serviceLogMetaFormat");
          if (!humanMessage || !timestamp || !format) {
            throw new Error("Light Human log fixtures are incomplete.");
          }
          expectReadableText(humanMessage, terminal, "light human log message");
          expectReadableText(timestamp, terminal, "light log timestamp");
          expectReadableText(format, format, "light log format badge");
          for (const level of document.querySelectorAll(".serviceLogLevel")) {
            expectReadableText(level, level, `light ${level.dataset.level ?? "unknown"} level badge`);
          }
          for (const chip of document.querySelectorAll(".serviceLogMetaChip")) {
            const key = chip.querySelector(".serviceLogMetaKey");
            const value = chip.querySelector(".serviceLogMetaValue");
            if (!key || !value) throw new Error("Light metadata chip is incomplete.");
            expectReadableText(key, chip, "light metadata key");
            expectReadableText(value, chip, "light metadata value");
          }
          return;
        }
        const ansiSegments = Array.from(
          document.querySelectorAll(".serviceLogMsg span[style]"),
        ).filter((segment) => segment.style.color.length > 0);
        if (ansiSegments.length === 0) {
          throw new Error("Raw log view did not render ANSI-colored segments.");
        }
        for (const segment of ansiSegments) {
          expectReadableText(segment, terminal, "light raw ANSI segment");
        }
      }, mode);
    };

    await assertPairs("human");
    await page.getByRole("button", { name: "Raw" }).click();
    await page.locator('.serviceLogRow[data-view="raw"]').first().waitFor({ timeout: 10_000 });
    await assertPairs("raw");
  } finally {
    await page.close().catch(() => {});
  }
}

async function assertServiceLogsTimestampLayout({ baseUrl, browser, label, storyId, viewport }) {
  const page = await browser.newPage();
  try {
    await page.setViewportSize(viewport);
    const base = normalizeBaseUrl(baseUrl);
    const url = new URL("iframe.html", base);
    url.searchParams.set("id", storyId);
    url.searchParams.set("viewMode", "story");
    await page.goto(url.toString(), { waitUntil: "domcontentloaded" });
    await page.locator(".serviceLogsTerminal").waitFor({ timeout: 10_000 });
    await page.locator(".serviceLogTsTime").first().waitFor({ timeout: 10_000 });

    await page.getByRole("button", { name: "Human" }).click();
    await page.locator('.serviceLogRow[data-view="human"] .serviceLogTsTime').first().waitFor({ timeout: 10_000 });

    const assertLayout = async (mode) => {
      const layout = await page.evaluate(() => {
        const terminal = document.querySelector(".serviceLogsTerminal");
        const headerTime = document.querySelector(".serviceLogsTerminalHead > span");
        const timestamp = document.querySelector(`.serviceLogRow[data-view="${document.querySelector('.serviceLogsTerminal')?.getAttribute('data-service-logs-view')}"] .serviceLogTs`);
        const time = timestamp?.querySelector(".serviceLogTsTime");
        const date = timestamp?.querySelector(".serviceLogTsDate");
        if (!(terminal instanceof HTMLElement && headerTime instanceof HTMLElement && timestamp instanceof HTMLElement && time instanceof HTMLElement && date instanceof HTMLElement)) {
          return null;
        }
        const headerRect = headerTime.getBoundingClientRect();
        const timestampRect = timestamp.getBoundingClientRect();
        const timeRect = time.getBoundingClientRect();
        const dateRect = date.getBoundingClientRect();
        return {
          dateAfterTime: Boolean(time.compareDocumentPosition(date) & Node.DOCUMENT_POSITION_FOLLOWING),
          timeColumn: getComputedStyle(headerTime.parentElement).gridTemplateColumns.split(" ")[0],
          inlinePadding: getComputedStyle(terminal).getPropertyValue("--service-log-inline-padding").trim(),
          leftDelta: timestampRect.left - headerRect.left,
          mode: terminal.dataset.serviceLogsView,
          timeAboveDate: timeRect.top < dateRect.top,
        };
      });
      if (!layout) throw new Error(`Missing timestamp layout (${label}, ${mode}).`);
      if (layout.mode !== mode || !layout.dateAfterTime || !layout.timeAboveDate) {
        throw new Error(`Timestamp order failed (${label}, ${mode}): ${JSON.stringify(layout)}`);
      }
      const expectedPadding = viewport.width <= 960 ? "14px" : "18px";
      const expectedTimeColumn = viewport.width <= 960 ? "112px" : "128px";
      if (
        layout.inlinePadding !== expectedPadding ||
        layout.timeColumn !== expectedTimeColumn ||
        !approxEqual(layout.leftDelta, 0, 1)
      ) {
        throw new Error(`Timestamp alignment failed (${label}, ${mode}): ${JSON.stringify(layout)}`);
      }
    };

    await assertLayout("human");
    await page.getByRole("button", { name: "Raw" }).click();
    await page.locator('.serviceLogRow[data-view="raw"] .serviceLogTsTime').first().waitFor({ timeout: 10_000 });
    await assertLayout("raw");
    await page.getByRole("button", { name: "UTC" }).click();
    await page.locator('button[aria-pressed="true"]', { hasText: "UTC" }).waitFor({ timeout: 10_000 });
    await assertLayout("raw");
  } finally {
    await page.close().catch(() => {});
  }
}

async function assertServiceLogsFollowAfterNewLog({
  baseUrl,
  browser,
  eventGate,
  expectedCount,
  initialCount,
  label,
  storyId,
  evictedHeadMarker,
  expectedHeadMarker,
  tailIndex,
  tailMarker,
}) {
  const page = await browser.newPage();
  try {
    await page.setViewportSize({ width: 1440, height: 1000 });
    const base = normalizeBaseUrl(baseUrl);
    const url = new URL("iframe.html", base);
    url.searchParams.set("id", storyId);
    url.searchParams.set("viewMode", "story");
    await page.goto(url.toString(), { waitUntil: "domcontentloaded" });

    const terminal = page.locator(
      `.serviceLogsTerminal[data-service-logs-total-count="${initialCount}"]`,
    );
    const viewport = page.getByRole("region", { name: "服务实时日志" });
    await terminal.waitFor({ timeout: 15_000 });
    await viewport.waitFor({ timeout: 15_000 });
    await page.getByRole("button", { name: "Raw" }).click();
    await page.locator('.serviceLogRow[data-view="raw"]').first().waitFor({ timeout: 10_000 });
    await page.waitForFunction(
      (gate) => {
        const eventGates = window.__DOCKREV_MOCK_EVENT_GATES__;
        return eventGates?.released instanceof Set
          && eventGates.waiting instanceof Set
          && eventGates.waiting.has(gate);
      },
      eventGate,
      { timeout: 10_000 },
    );

    await viewport.evaluate((element) => {
      element.scrollTop = 0;
      element.dispatchEvent(new Event("scroll"));
    });
    await page.evaluate(() => {
      const button = Array.from(document.querySelectorAll("button")).find(
        (candidate) => candidate.textContent?.trim() === "跳到最新",
      );
      if (!(button instanceof HTMLElement)) {
        throw new Error("Expected the jump-to-latest button to be mounted.");
      }
      button.click();
    });
    await page.evaluate((gate) => {
      const eventGates = window.__DOCKREV_MOCK_EVENT_GATES__;
      if (!(eventGates?.released instanceof Set && eventGates.waiting instanceof Set)) {
        throw new Error("Expected the current service log event gate to be armed.");
      }
      eventGates.released.add(gate);
      window.dispatchEvent(new Event(`dockrev:release-service-log-events:${gate}`));
    }, eventGate);

    await page.waitForFunction(
      (expectedCount) =>
        document.querySelector('.serviceLogsTerminal')?.dataset.serviceLogsTotalCount === expectedCount,
      expectedCount,
      { timeout: 5_000 },
    );
    const tailSelector = `.serviceLogRow[data-index="${tailIndex}"]`;
    const tailLastLine = "trace detail 24";
    await page.waitForFunction(
      ({ expectedCount, tailLastLine, tailMarker, tailSelector }) => {
        const viewport = document.querySelector('[aria-label="服务实时日志"]');
        const terminal = document.querySelector('.serviceLogsTerminal');
        const tail = document.querySelector(tailSelector);
        const jump = Array.from(document.querySelectorAll("button")).some(
          (button) => button.textContent?.trim() === "跳到最新",
        );
        if (!(viewport instanceof HTMLElement) || !(tail instanceof HTMLElement)) return false;
        const viewportRect = viewport.getBoundingClientRect();
        const tailRect = tail.getBoundingClientRect();
        return terminal?.dataset.serviceLogsTotalCount === expectedCount
          && tail.textContent?.includes(tailMarker)
          && tail.textContent?.includes(tailLastLine)
          && viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight < 48
          && tailRect.top >= viewportRect.top
          && tailRect.bottom <= viewportRect.bottom + 1
          && !jump;
      },
      { expectedCount, tailLastLine, tailMarker, tailSelector },
      { timeout: 10_000 },
    ).catch(async () => {
      const state = await page.evaluate(({ tailLastLine, tailMarker, tailSelector }) => {
        const viewport = document.querySelector('[aria-label="服务实时日志"]');
        const terminal = document.querySelector('.serviceLogsTerminal');
        const tail = document.querySelector(tailSelector);
        const viewportRect = viewport?.getBoundingClientRect();
        const tailRect = tail?.getBoundingClientRect();
        return {
          bufferCount: terminal?.dataset.serviceLogsTotalCount,
          distanceFromBottom: viewport ? viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight : null,
          jump: Array.from(document.querySelectorAll("button")).some(
            (button) => button.textContent?.trim() === "跳到最新",
          ),
          tailLastLineVisible: tail?.textContent?.includes(tailLastLine) === true,
          tailVisible: tail?.textContent?.includes(tailMarker) === true,
          tailFullyVisible: Boolean(
            viewportRect
              && tailRect
              && tailRect.top >= viewportRect.top
              && tailRect.bottom <= viewportRect.bottom + 1,
          ),
        };
      }, { tailLastLine, tailMarker, tailSelector });
      throw new Error(`Service logs stopped following after ${label}: ${JSON.stringify(state)}`);
    });

    const repeatedPayload = await page.evaluate(async (serviceId) => {
      const response = await fetch(`/api/services/${encodeURIComponent(serviceId)}/logs/events`);
      return response.text();
    }, "svc-prod-api");
    if (repeatedPayload.includes(tailMarker)) {
      throw new Error(`Service log event payload repeated after ${label}.`);
    }

    if (evictedHeadMarker && expectedHeadMarker) {
      await viewport.evaluate((element) => {
        element.scrollTop = 0;
        element.dispatchEvent(new Event("scroll"));
      });
      await page.waitForFunction(
        ({ evictedHeadMarker, expectedHeadMarker }) => {
          const head = document.querySelector('.serviceLogRow[data-index="0"]');
          return head?.textContent?.includes(expectedHeadMarker)
            && !head.textContent?.includes(evictedHeadMarker);
        },
        { evictedHeadMarker, expectedHeadMarker },
        { timeout: 10_000 },
      );
    }
  } finally {
    await page.close().catch(() => {});
  }
}

async function assertHoverPinKeepsPopoverOpen({
  page,
  trigger,
  popover,
  label,
}) {
  await trigger.hover();
  await popover.waitFor({ timeout: 10_000 });

  await trigger.click();

  const triggerBox = await requireBoundingBox(trigger, `${label} trigger`);
  const popoverBox = await requireBoundingBox(popover, `${label} popover`);
  const outsideX = Math.max(0, Math.min(triggerBox.x, popoverBox.x) - 24);
  const outsideY = Math.max(0, Math.min(triggerBox.y, popoverBox.y) - 24);
  await page.mouse.move(outsideX, outsideY);
  await page.waitForTimeout(450);

  const state = await popover.getAttribute("data-state");
  if (state !== "open") {
    throw new Error(
      `Popover did not stay pinned after hover+click (${label}).`,
    );
  }
}

async function assertGroupGuideAligned(page, label) {
  const allGroups = page.locator(".tableGroup");
  await allGroups.first().waitFor({ timeout: 10_000 });

  const groups = page.locator(".tableGroupExpanded");
  let groupCount = await groups.count();
  if (groupCount === 0) {
    // Story state may render groups collapsed (or render delay); try expanding the first group.
    const head = allGroups.first().locator(".groupHead");
    await head.click({ timeout: 10_000 });
    await groups.first().waitFor({ timeout: 10_000 });
    groupCount = await groups.count();
  }
  if (groupCount === 0)
    throw new Error(
      `No expanded table groups found${label ? ` (${label})` : ""}.`,
    );

  for (let gi = 0; gi < groupCount; gi += 1) {
    const group = groups.nth(gi);
    const guide = group.locator(".groupGuide");
    const rows = group.locator(".rowLine");

    await guide.waitFor({ timeout: 10_000 });
    const rowCount = await rows.count();
    if (rowCount === 0) continue;

    const guideBox = await requireBoundingBox(guide, `groupGuide[${gi}]`);
    const row0Box = await requireBoundingBox(rows.nth(0), `rowLine[${gi}][0]`);

    // CI runners can render fractional text metrics slightly differently than local macOS.
    if (!approxEqual(guideBox.y, row0Box.y, 2)) {
      throw new Error(
        `Guide top misaligned (group=${gi}${label ? `, ${label}` : ""}): guide.y=${guideBox.y}, row0.y=${row0Box.y}`,
      );
    }

    const rowHeight = row0Box.height;
    let rowGap = 0;
    if (rowCount > 1) {
      const row1Box = await requireBoundingBox(
        rows.nth(1),
        `rowLine[${gi}][1]`,
      );
      rowGap = row1Box.y - (row0Box.y + row0Box.height);
      // Flex `gap` should never be negative; tolerate minor rounding.
      if (rowGap < -0.5) {
        throw new Error(
          `Row gap is negative (group=${gi}${label ? `, ${label}` : ""}): gap=${rowGap}, row0.height=${row0Box.height}`,
        );
      }
    }

    let baselineBulletCenterInGuide = null;
    for (let ri = 0; ri < rowCount; ri += 1) {
      const rowBox = await requireBoundingBox(
        rows.nth(ri),
        `rowLine[${gi}][${ri}]`,
      );
      if (!approxEqual(rowBox.height, rowHeight, 1)) {
        throw new Error(
          `Row height drift (group=${gi}, row=${ri}${label ? `, ${label}` : ""}): row.height=${rowBox.height}, expected~${rowHeight}`,
        );
      }

      const bullet = rows.nth(ri).locator(".svcBullet");
      const bulletBox = await requireBoundingBox(
        bullet,
        `svcBullet[${gi}][${ri}]`,
      );
      const bulletCenterY = bulletBox.y + bulletBox.height / 2;
      const bulletCenterX = bulletBox.x + bulletBox.width / 2;

      // Bullet is centered in the row by CSS (`top: 50%`).
      const bulletCenterInRow = bulletCenterY - rowBox.y;
      // Chromium text metrics can shift the computed row box by a fractional pixel between
      // local and CI environments; allow a slightly roomier tolerance without weakening
      // the underlying "centered in the row" invariant.
      if (!approxEqual(bulletCenterInRow, rowHeight / 2, 2)) {
        throw new Error(
          `Bullet not vertically centered (group=${gi}, row=${ri}${label ? `, ${label}` : ""}): centerInRow=${bulletCenterInRow}, expected~${rowHeight / 2}`,
        );
      }

      // Bullet should also be horizontally centered on the guide line.
      const guideCenterX = guideBox.x + guideBox.width / 2;
      if (!approxEqual(bulletCenterX, guideCenterX, 1)) {
        throw new Error(
          `Bullet-guide X misaligned (group=${gi}, row=${ri}${label ? `, ${label}` : ""}): bullet.centerX=${bulletCenterX}, guide.centerX=${guideCenterX}`,
        );
      }

      // Track spacing between bullets by using row-0 as baseline to avoid cross-platform subpixel offsets.
      const bulletCenterInGuide = bulletCenterY - guideBox.y;
      if (baselineBulletCenterInGuide == null)
        baselineBulletCenterInGuide = bulletCenterInGuide;
      const expected = baselineBulletCenterInGuide + ri * (rowHeight + rowGap);
      // Keep a looser tolerance here: different runners can produce a few pixels
      // of cumulative subpixel stacking drift even when row height/gap invariants hold.
      if (!approxEqual(bulletCenterInGuide, expected, 4)) {
        throw new Error(
          `Bullet-guide alignment drift (group=${gi}, row=${ri}${label ? `, ${label}` : ""}): actual=${bulletCenterInGuide}, expected~${expected}`,
        );
      }
    }
  }
}

async function assertServiceOperationHistoryColumnsAligned(page, label) {
  await page.locator(".serviceOperationHistoryRow").first().waitFor({
    timeout: 10_000,
  });

  const layout = await page.evaluate(() => {
    const selectors = [
      ".serviceOperationHistoryOperation",
      ".serviceOperationHistoryStatus",
      ".serviceOperationHistoryBackup",
      ".serviceOperationHistorySource",
      ".serviceOperationHistoryTime",
      ".serviceOperationHistoryAction",
    ];

    const rows = Array.from(document.querySelectorAll(".serviceOperationHistoryRow")).map(
      (row) =>
        selectors.map((selector) => {
          const cell = row.querySelector(selector);
          if (!cell) return null;
          const rect = cell.getBoundingClientRect();
          return { left: rect.left, width: rect.width };
        }),
    );

    return { rows };
  });

  if (layout.rows.length < 2) {
    throw new Error(
      `Expected at least 2 history rows${label ? ` (${label})` : ""}, got ${layout.rows.length}.`,
    );
  }

  const baseline = layout.rows[0];
  for (let rowIndex = 1; rowIndex < layout.rows.length; rowIndex += 1) {
    const row = layout.rows[rowIndex];
    if (row.some((cell) => !cell)) {
      throw new Error(
        `Missing history cell${label ? ` (${label})` : ""} in row ${rowIndex + 1}.`,
      );
    }
    for (let columnIndex = 0; columnIndex < baseline.length; columnIndex += 1) {
      const baselineCell = baseline[columnIndex];
      const cell = row[columnIndex];
      if (!approxEqual(cell.left, baselineCell.left, 1)) {
        throw new Error(
          `History column left drift${label ? ` (${label})` : ""}: row=${rowIndex + 1}, column=${columnIndex + 1}, actual=${cell.left}, expected=${baselineCell.left}.`,
        );
      }
      if (!approxEqual(cell.width, baselineCell.width, 1)) {
        throw new Error(
          `History column width drift${label ? ` (${label})` : ""}: row=${rowIndex + 1}, column=${columnIndex + 1}, actual=${cell.width}, expected=${baselineCell.width}.`,
        );
      }
    }
  }
}

async function runSmoke({ baseUrl, storyIds, browser }) {
  if (storyIds.length === 0) {
    throw new Error(
      "No stories discovered from index.json. Storybook may be misconfigured or the index schema may have changed.",
    );
  }
  console.log(`Testing ${storyIds.length} stories...`);
  const failures = [];

  for (const id of storyIds) {
    const page = await browser.newPage();
    const pageErrors = [];
    page.on("pageerror", (err) => pageErrors.push(err));

    try {
      const base = normalizeBaseUrl(baseUrl);
      const url = new URL("iframe.html", base);
      url.searchParams.set("id", id);
      url.searchParams.set("viewMode", "story");

      await page.goto(url.toString(), {
        waitUntil: "domcontentloaded",
        timeout: 120_000,
      });
      await page.waitForFunction(
        () => {
          const root = document.querySelector("#storybook-root, #root");
          return Boolean(root && root.childElementCount > 0);
        },
        null,
        { timeout: 60_000 },
      );

      if (pageErrors.length > 0) {
        failures.push({ id, error: pageErrors[0] });
      }
    } catch (error) {
      failures.push({ id, error });
    } finally {
      await page.close().catch(() => {});
    }
  }

  if (failures.length > 0) {
    console.error(`Failed ${failures.length}/${storyIds.length} stories:`);
    for (const f of failures.slice(0, 20)) {
      console.error(`- ${f.id}: ${String(f.error?.message ?? f.error)}`);
    }
    if (failures.length > 20) {
      console.error(`...and ${failures.length - 20} more`);
    }
    throw new Error(
      `Storybook smoke test failed (${failures.length}/${storyIds.length}).`,
    );
  }

  console.log("All stories passed.");
}

async function runRollbackRefreshRace({ baseUrl, browser }) {
  const base = normalizeBaseUrl(baseUrl);
  const page = await browser.newPage();
  page.on("dialog", (d) => d.accept().catch(() => {}));
  const url = new URL("iframe.html", base);
  url.searchParams.set("id", "pages-servicedetailpage--rollback-refresh-race-after-update");
  url.searchParams.set("viewMode", "story");
  await page.goto(url.toString(), { waitUntil: "domcontentloaded" });
  await page.waitForFunction(
    () => {
      const root = document.querySelector("#storybook-root, #root");
      return Boolean(root && root.childElementCount > 0);
    },
    null,
    { timeout: 60_000 },
  );
  try {
    const rollbackRefresh = page.getByRole("button", { name: /回滚信息刷新中…/ }).first();
    await rollbackRefresh.waitFor({ timeout: 8_000 });
    await page.waitForFunction(
      () => {
        const button = Array.from(document.querySelectorAll("button")).find((item) =>
          item.textContent?.trim() === "回滚信息刷新中…",
        );
        return Boolean(button && button.disabled);
      },
      null,
      { timeout: 8_000 },
    );
    if ((await page.locator("body").textContent())?.includes("未找到可回滚到升级前版本的成功升级记录")) {
      throw new Error("Rollback refresh race exposed the stale unavailable hint.");
    }

    await page.waitForFunction(
      () => {
        const button = Array.from(document.querySelectorAll("button")).find((item) =>
          item.textContent?.trim() === "回滚",
        );
        return Boolean(button && !button.disabled && button.getAttribute("aria-busy") !== "true");
      },
      null,
      { timeout: 8_000 },
    );
  } finally {
    await page.close().catch(() => {});
  }
}

async function runInteractive({ baseUrl, browser }) {
  const base = normalizeBaseUrl(baseUrl);

  const openStory = async (id) => {
    const page = await browser.newPage();
    page.on("dialog", (d) => d.accept().catch(() => {}));
    const url = new URL("iframe.html", base);
    url.searchParams.set("id", id);
    url.searchParams.set("viewMode", "story");
    await page.goto(url.toString(), { waitUntil: "domcontentloaded" });
    await page.waitForFunction(
      () => {
        const root = document.querySelector("#storybook-root, #root");
        return Boolean(root && root.childElementCount > 0);
      },
      null,
      { timeout: 60_000 },
    );
    return page;
  };

  // Keep the rollback refresh race in the CI interaction suite, not only in the story play callback.
  await runRollbackRefreshRace({ baseUrl, browser });

  await assertServiceLogsTimestampLayout({
    baseUrl,
    browser,
    label: "desktop",
    storyId: "pages-servicedetailpage--logs-section",
    viewport: { width: 1440, height: 1000 },
  });
  await assertServiceLogsTimestampLayout({
    baseUrl,
    browser,
    label: "mobile",
    storyId: "pages-servicedetailpage--mobile-logs-timestamp-layout",
    viewport: { width: 393, height: 852 },
  });
  await assertServiceLogsLightContrast({ baseUrl, browser });
  await assertServiceLogsFollowAfterNewLog({
    baseUrl,
    browser,
    eventGate: "follow-after-append",
    expectedCount: "101",
    initialCount: 100,
    label: "ordinary append",
    storyId: "pages-service-log-follow--follows-after-append",
    tailIndex: 100,
    tailMarker: "follow-after-append",
  });
  await assertServiceLogsFollowAfterNewLog({
    baseUrl,
    browser,
    eventGate: "follow-after-buffer-eviction",
    expectedCount: "2000",
    initialCount: 2000,
    label: "buffer eviction",
    storyId: "pages-service-log-follow--follows-after-buffer-eviction",
    evictedHeadMarker: "trace=req-0000",
    expectedHeadMarker: "worker cycle=1",
    tailIndex: 1999,
    tailMarker: "follow-after-buffer-eviction",
  });

  // Version directory navigation must converge even when the target card is not rendered yet.
  {
    const page = await openStory("pages-servicedetailpage--versions-section");
    try {
      await page.setViewportSize({ width: 1800, height: 1200 });
      const viewport = page.locator(".serviceVersionsScrollViewport").first();
      await viewport.evaluate((element) => {
        element.scrollTop = element.scrollHeight;
        element.dispatchEvent(new Event("scroll"));
      });
      const targetIndex = page
        .locator('[data-service-versions-index-selected][data-release-tag="5.0.4"]')
        .first();
      await viewport.waitFor({ timeout: 10_000 });
      await targetIndex.waitFor({ timeout: 10_000 });
      await viewport.evaluate((element) => {
        element.scrollTop = 0;
        element.dispatchEvent(new Event("scroll"));
      });
      await page.locator(".serviceVersionsIndexViewport").evaluate((element) => {
        element.scrollTop = element.scrollHeight;
        element.dispatchEvent(new Event("scroll"));
      });
      await targetIndex.waitFor({ timeout: 10_000 });
      await page.waitForTimeout(100);

      if (
        (await viewport.locator('[data-service-version-card="true"][data-release-tag="5.0.4"]').count()) !== 0
      ) {
        throw new Error("Expected the distant 5.0.4 card to start outside the virtual window.");
      }

      await targetIndex.click();
      await page.waitForFunction(
        () => {
          const target = document.querySelector(
            '[data-service-versions-index-selected][data-release-tag="5.0.4"]',
          );
          return (
            target?.getAttribute("aria-pressed") === "true" &&
            target.getAttribute("data-service-versions-index-selected") === "true"
          );
        },
        null,
        { timeout: 1_000 },
      );

      await page.waitForFunction(
        () => {
          const scrollElement = document.querySelector(".serviceVersionsScrollViewport");
          const selected = document.querySelector('[data-service-versions-index-selected="true"]');
          if (!(scrollElement instanceof HTMLElement) || !(selected instanceof HTMLElement)) return false;
          const viewportRect = scrollElement.getBoundingClientRect();
          const viewportCenter = viewportRect.top + viewportRect.height / 2;
          const centered = Array.from(
            scrollElement.querySelectorAll('[data-service-version-card="true"]'),
          )
            .map((card) => {
              const rect = card.getBoundingClientRect();
              return {
                tag: card.getAttribute("data-release-tag"),
                distance: Math.abs(rect.top + rect.height / 2 - viewportCenter),
              };
            })
            .sort((left, right) => left.distance - right.distance)[0];
          return (
            selected.getAttribute("data-release-tag") === "5.0.4" &&
            centered?.tag === "5.0.4" &&
            centered.distance <= 48
          );
        },
        null,
        { timeout: 10_000 },
      );

      const newerIndex = page
        .locator('[data-service-versions-index-selected][data-release-tag="5.0.9"]')
        .first();
      await newerIndex.click();
      await viewport.dispatchEvent("wheel", { deltaY: 500 });
      await viewport.evaluate((element) => {
        element.scrollTop += 500;
        element.dispatchEvent(new Event("scroll"));
      });
      await page.waitForFunction(
        () => {
          const scrollElement = document.querySelector(".serviceVersionsScrollViewport");
          const selected = document.querySelector('[data-service-versions-index-selected="true"]');
          if (!(scrollElement instanceof HTMLElement) || !(selected instanceof HTMLElement)) return false;
          const rect = scrollElement.getBoundingClientRect();
          const center = rect.top + rect.height / 2;
          const nearest = Array.from(scrollElement.querySelectorAll('[data-service-version-card="true"]'))
            .map((card) => {
              const cardRect = card.getBoundingClientRect();
              return { tag: card.getAttribute("data-release-tag"), distance: Math.abs(cardRect.top + cardRect.height / 2 - center) };
            })
            .sort((left, right) => left.distance - right.distance)[0];
          return nearest?.tag === selected.getAttribute("data-release-tag");
        },
        null,
        { timeout: 10_000 },
      );

      const handoff = await viewport.evaluate((scrollElement) => {
        const selected = document.querySelector('[data-service-versions-index-selected="true"]');
        const viewportRect = scrollElement.getBoundingClientRect();
        const viewportCenter = viewportRect.top + viewportRect.height / 2;
        const centered = Array.from(
          scrollElement.querySelectorAll('[data-service-version-card="true"]'),
        )
          .map((card) => {
            const rect = card.getBoundingClientRect();
            return {
              tag: card.getAttribute("data-release-tag"),
              distance: Math.abs(rect.top + rect.height / 2 - viewportCenter),
            };
          })
          .sort((left, right) => left.distance - right.distance)[0];
        return {
          selected: selected?.getAttribute("data-release-tag"),
          centered: centered?.tag,
        };
      });
      if (!handoff.selected || handoff.selected !== handoff.centered) {
        throw new Error(
          `Expected user scrolling to take over version selection: ${JSON.stringify(handoff)}`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 0) Group guide line alignment must remain stable (no JS measuring).
  {
    const storyIds = ["pages-servicespage--guide-line-long-names"];
    for (const id of storyIds) {
      const page = await openStory(id);
      try {
        await assertGroupGuideAligned(page, id);

        const row0Before = await requireBoundingBox(
          page.locator(".tableGroupExpanded .rowLine").first(),
          `${id}:row0`,
        );
        await page.addStyleTag({
          content: `.tableGroup { --dockrev-table-font-size: 14px; --dockrev-table-line-height: 1.7; }`,
        });
        await page.waitForTimeout(100);
        const row0After = await requireBoundingBox(
          page.locator(".tableGroupExpanded .rowLine").first(),
          `${id}:row0`,
        );

        if (!(row0After.height > row0Before.height + 0.5)) {
          throw new Error(
            `Expected row height to scale with font changes (${id}): before=${row0Before.height}, after=${row0After.height}`,
          );
        }

        await assertGroupGuideAligned(page, `${id} (scaled)`);
      } finally {
        await page.close().catch(() => {});
      }
    }
  }

  // 1) Overview page: search should exclude services without Web launch hrefs and filter Web cards.
  {
    const page = await openStory("pages-overviewpage--search-and-fallback");
    try {
      const search = page
        .locator('.homepageOverviewSearchForm input[aria-label="搜索服务入口"]')
        .first();
      await search.waitFor({ timeout: 10_000 });
      if ((await page.locator(".homepageOverviewSearchButton").count()) > 0) {
        throw new Error("Expected overview search to submit with Enter and render no search button.");
      }
      await search.fill("worker");
      await search.press("Enter");
      await page.waitForTimeout(250);

      await page.waitForFunction(
        () => {
          const visibleCards = Array.from(
            document.querySelectorAll(".homepageServiceCard"),
          ).filter((card) => {
            const style = window.getComputedStyle(card);
            return style.display !== "none" && style.visibility !== "hidden";
          });
          return visibleCards.length === 0;
        },
        null,
        { timeout: 10_000 },
      );

      await search.fill("Acme API");
      await search.press("Enter");
      await page.waitForTimeout(250);

      await page.waitForFunction(
        () => {
          const visibleCards = Array.from(
            document.querySelectorAll(".homepageServiceCard"),
          ).filter((card) => {
            const style = window.getComputedStyle(card);
            return style.display !== "none" && style.visibility !== "hidden";
          });
          return (
            visibleCards.length === 1 &&
            visibleCards[0]?.textContent?.includes("Acme API")
          );
        },
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 2) Overview page: homepage cards should expose external links, status badges, and resource cells.
  {
    const page = await openStory("pages-overviewpage--default");
    try {
      const apiCard = page
        .locator(".homepageServiceCard", { hasText: "Acme API" })
        .first();
      await apiCard.waitFor({ timeout: 10_000 });
      const role = await apiCard.getAttribute("role");
      const cardText = (await apiCard.textContent()) ?? "";
      const badgeText = (await apiCard.locator(".homepageServiceStateBadge").textContent()) ?? "";
      const metricCount = await apiCard.locator(".homepageServiceMetric").count();
      const detailButtonCount = await apiCard.locator(".homepageServiceDetailButton").count();
      const updateButtonCount = await apiCard.locator(".homepageServiceStateButton").count();

      if (role !== "link") {
        throw new Error(
          `Expected Acme API card to expose direct launcher semantics, got role=${String(role)}.`,
        );
      }
      if (!badgeText.includes("可更新")) {
        throw new Error(
          `Expected Acme API card to surface the update status badge, got badge=${JSON.stringify(badgeText)} full=${JSON.stringify(cardText)}.`,
        );
      }
      if (metricCount !== 4) {
        throw new Error(
          `Expected Acme API card to render CPU/MEM/RX/TX metric cells, got ${metricCount}.`,
        );
      }
      if (detailButtonCount !== 1) {
        throw new Error(
          `Expected Acme API card to expose one service detail button, got ${detailButtonCount}.`,
        );
      }
      if (updateButtonCount !== 1) {
        throw new Error(
          `Expected Acme API updatable badge to be clickable, got ${updateButtonCount}.`,
        );
      }
      if (cardText.includes("新窗口")) {
        throw new Error(
          `Expected overview cards to remove the legacy new-window pill text, got ${JSON.stringify(cardText)}.`,
        );
      }

      await page.setViewportSize({ width: 390, height: 920 });
      await page.waitForTimeout(150);
      if (await page.locator(".topbarGlobalContent .homepageTopStrip").isVisible()) {
        throw new Error(
          "Expected mobile overview to remove the resource/search/time strip from the shell header.",
        );
      }
      const mobileModuleStrip = page.locator(
        ".homepageMobileNavModule .homepageTopStrip",
      );
      await mobileModuleStrip.waitFor({ state: "visible", timeout: 10_000 });
      if ((await mobileModuleStrip.getByRole("searchbox", { name: "搜索服务入口" }).count()) > 0) {
        throw new Error(
          "Expected mobile resource strip to keep search out of the metric row.",
        );
      }
      await page.locator(".topbar .homepageHeaderSearchToggle").waitFor({
        timeout: 10_000,
      });

      await page.locator(".mobileBottomNav").waitFor({ timeout: 10_000 });
      await page.locator(".mobileMenuButton").click();
      const drawerSearch = page.locator(
        "#mobileDockrevMenu .mobileMenuEmbeddedContent .homepageDrawerSearchSlot",
      );
      await drawerSearch.waitFor({ state: "visible", timeout: 10_000 });
      await drawerSearch.getByRole("searchbox", { name: "搜索服务入口" }).waitFor({
        timeout: 10_000,
      });
      await page
        .locator("#mobileDockrevMenu .homepageDrawerBottomSummary")
        .waitFor({
          state: "visible",
          timeout: 10_000,
        });
      await page.locator("#mobileDockrevMenu .homepageDrawerBottomSummary .homepageClock").waitFor({
        timeout: 10_000,
      });
      const mobileOverflow = await page.evaluate(() => document.body.style.overflow);
      if (mobileOverflow !== "hidden") {
        throw new Error(
          `Expected open mobile drawer to lock body scroll, got ${JSON.stringify(mobileOverflow)}.`,
        );
      }
      await page.setViewportSize({ width: 1200, height: 920 });
      await page.waitForFunction(() => document.body.style.overflow !== "hidden", null, {
        timeout: 10_000,
      });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 3) Queue job detail: logs should be shown on a dedicated page, and navigation must work.
  {
    const page = await openStory("pages-interactiveapp--queue-long-logs");
    try {
      const items = page.locator(".queueItem");
      await items.nth(1).waitFor({ timeout: 10_000 });

      // Open short-log job, go back, then open long-log job.
      await items.nth(0).click();
      await page.getByText("job:").waitFor({ timeout: 10_000 });
      await page.getByText("job-short").waitFor({ timeout: 10_000 });

      const back = page.getByRole("button", { name: "返回列表" });
      await back.waitFor({ timeout: 10_000 });
      await back.click();
      await page.locator(".queueList").waitFor({ timeout: 10_000 });

      // The queue-long-logs fixture keeps the archived long-log job as the final list item.
      await items.last().click();
      await page.getByText("job:").waitFor({ timeout: 10_000 });
      await page.getByText("job-long").waitFor({ timeout: 10_000 });
      // Use an exact match so fixture expansions (more lines mentioning the digest) won't break strict mode.
      const digest = `sha256:${"9".repeat(64)}`;
      await page
        .getByText(digest, { exact: true })
        .waitFor({ timeout: 10_000 });

      const back2 = page.getByRole("button", { name: "返回列表" });
      await back2.waitFor({ timeout: 10_000 });
      await back2.click();
      await page.locator(".queueList").waitFor({ timeout: 10_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 3b) Queue dual progress: split planned/completed must render as two layers on one bar.
  {
    const page = await openStory("pages-queuepage--default");
    try {
      const bar = page.locator(".queueProgressBarDual").first();
      await bar.waitFor({ timeout: 10_000 });

      const ariaValueText = await bar.getAttribute("aria-valuetext");
      if (
        !ariaValueText?.includes("安排 80%") ||
        !ariaValueText.includes("完成 40%")
      ) {
        throw new Error(
          `Unexpected queue dual-progress aria text: ${String(ariaValueText)}`,
        );
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll(".queueProgressFill"));
        const planned = fills[0];
        const completed = fills[1];
        return {
          fillCount: fills.length,
          plannedTransform: planned ? planned.style.transform : null,
          completedTransform: completed ? completed.style.transform : null,
        };
      });

      if (info.fillCount < 2)
        throw new Error(
          `Expected at least 2 queue progress fill layers, got ${info.fillCount}`,
        );
      if (info.plannedTransform === info.completedTransform) {
        throw new Error(
          `Expected queue planned/completed transforms to differ for split progress, got planned=${String(info.plannedTransform)}, completed=${String(info.completedTransform)}`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 3c) Queue fallback: legacy payload without planned* must fallback to planned=completed.
  {
    const page = await openStory("pages-queuepage--legacy-progress-fallback");
    try {
      const bar = page.locator(".queueProgressBarDual").first();
      await bar.waitFor({ timeout: 10_000 });

      const ariaValueText = await bar.getAttribute("aria-valuetext");
      if (
        !ariaValueText?.includes("安排 40%") ||
        !ariaValueText.includes("完成 40%")
      ) {
        throw new Error(
          `Unexpected queue legacy fallback aria text: ${String(ariaValueText)}`,
        );
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll(".queueProgressFill"));
        const planned = fills[0];
        const completed = fills[1];
        return {
          fillCount: fills.length,
          plannedTransform: planned ? planned.style.transform : null,
          completedTransform: completed ? completed.style.transform : null,
        };
      });

      if (info.fillCount < 2)
        throw new Error(
          `Expected at least 2 queue progress fill layers, got ${info.fillCount}`,
        );
      if (
        info.plannedTransform !== "scaleX(0.4)" ||
        info.completedTransform !== "scaleX(0.4)"
      ) {
        throw new Error(
          `Expected queue legacy fallback transforms to match 40%, got planned=${String(info.plannedTransform)}, completed=${String(info.completedTransform)}`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 3d) Job detail dual progress: planned/completed split must be visible and accessible.
  {
    const page = await openStory("pages-jobdetailpage--running-dual-progress");
    try {
      const bar = page.locator(".jobProgressBarDual").first();
      await bar.waitFor({ timeout: 10_000 });

      const ariaValueText = await bar.getAttribute("aria-valuetext");
      if (
        !ariaValueText?.includes("安排 90%") ||
        !ariaValueText.includes("完成 70%")
      ) {
        throw new Error(
          `Unexpected job detail dual-progress aria text: ${String(ariaValueText)}`,
        );
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll(".jobProgressFill"));
        const planned = fills[0];
        const completed = fills[1];
        return {
          fillCount: fills.length,
          plannedTransform: planned ? planned.style.transform : null,
          completedTransform: completed ? completed.style.transform : null,
        };
      });

      if (info.fillCount < 2)
        throw new Error(
          `Expected at least 2 job detail progress fill layers, got ${info.fillCount}`,
        );
      if (
        info.plannedTransform !== "scaleX(0.9)" ||
        info.completedTransform !== "scaleX(0.7)"
      ) {
        throw new Error(
          `Expected job detail split transforms planned=90% completed=70%, got planned=${String(info.plannedTransform)}, completed=${String(info.completedTransform)}`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 3e) Job detail fallback: legacy payload without planned* must fallback to planned=completed.
  {
    const page = await openStory(
      "pages-jobdetailpage--legacy-progress-fallback",
    );
    try {
      const bar = page.locator(".jobProgressBarDual").first();
      await bar.waitFor({ timeout: 10_000 });

      const ariaValueText = await bar.getAttribute("aria-valuetext");
      if (
        !ariaValueText?.includes("安排 40%") ||
        !ariaValueText.includes("完成 40%")
      ) {
        throw new Error(
          `Unexpected job detail legacy fallback aria text: ${String(ariaValueText)}`,
        );
      }

      const info = await bar.evaluate((el) => {
        const fills = Array.from(el.querySelectorAll(".jobProgressFill"));
        const planned = fills[0];
        const completed = fills[1];
        return {
          fillCount: fills.length,
          plannedTransform: planned ? planned.style.transform : null,
          completedTransform: completed ? completed.style.transform : null,
        };
      });

      if (info.fillCount < 2)
        throw new Error(
          `Expected at least 2 job detail progress fill layers, got ${info.fillCount}`,
        );
      if (
        info.plannedTransform !== "scaleX(0.4)" ||
        info.completedTransform !== "scaleX(0.4)"
      ) {
        throw new Error(
          `Expected job detail legacy fallback transforms to match 40%, got planned=${String(info.plannedTransform)}, completed=${String(info.completedTransform)}`,
        );
      }

      const counters = await page.locator(".jobProgressCounters").innerText();
      if (!counters.includes("安排 2/5") || !counters.includes("完成 2/5")) {
        throw new Error(
          `Unexpected job detail legacy fallback counters: ${counters}`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 3f) Settings notification test bubbles: outside-click dismiss with a 3s minimum visibility window.
  {
    const page = await openStory("pages-settingspage--notification-card");
    try {
      const testBtn = page.locator(
        'button[data-notification-test-channel="email"]',
      );
      await testBtn.waitFor({ timeout: 10_000 });
      await testBtn.click();

      const bubble = page.locator('[data-notification-test-bubble="email"]');
      await bubble.waitFor({ state: "visible", timeout: 10_000 });
      await bubble.getByText("Email 渠道返回成功").waitFor({ timeout: 10_000 });

      // Must not auto-dismiss without an outside click.
      await page.waitForTimeout(3200);
      if (!(await bubble.isVisible().catch(() => false))) {
        throw new Error(
          "Expected notification test bubble to remain visible without outside click.",
        );
      }

      // After 3s, outside click should dismiss immediately.
      await page.mouse.click(5, 5);
      await bubble.waitFor({ state: "hidden", timeout: 1_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  {
    const page = await openStory("pages-settingspage--notification-card");
    try {
      const testBtn = page.locator(
        'button[data-notification-test-channel="email"]',
      );
      await testBtn.waitFor({ timeout: 10_000 });
      await testBtn.click();

      const bubble = page.locator('[data-notification-test-bubble="email"]');
      await bubble.waitFor({ state: "visible", timeout: 10_000 });
      await bubble.getByText("Email 渠道返回成功").waitFor({ timeout: 10_000 });

      // Outside click within 3s should schedule dismiss at t=3s (not immediately).
      await page.mouse.click(5, 5);
      await page.waitForTimeout(1000);
      if (!(await bubble.isVisible().catch(() => false))) {
        throw new Error(
          "Expected notification test bubble to stay visible during the first 3s after outside click.",
        );
      }

      await bubble.waitFor({ state: "hidden", timeout: 3_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  {
    const page = await openStory("pages-settingspage--notification-card");
    try {
      const testBtn = page.locator(
        'button[data-notification-test-channel="webPush"]',
      );
      await testBtn.waitFor({ timeout: 10_000 });
      await testBtn.click();

      const bubble = page.locator('[data-notification-test-bubble="webPush"]');
      await bubble.waitFor({ state: "visible", timeout: 10_000 });
      await bubble
        .getByText("Web Push 渠道测试失败")
        .waitFor({ timeout: 10_000 });

      // Error bubble should follow the same 3s minimum-visibility rules.
      await page.mouse.click(5, 5);
      await page.waitForTimeout(1000);
      if (!(await bubble.isVisible().catch(() => false))) {
        throw new Error(
          "Expected notification test error bubble to stay visible during the first 3s after outside click.",
        );
      }

      await bubble.waitFor({ state: "hidden", timeout: 3_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 4) Update confirm modal: version popover must be above the modal overlay (not occluded).
  {
    const page = await openStory("pages-servicespage--dashboard-demo");
    try {
      const row = page.locator(".rowLine", { hasText: "api" }).first();
      const btn = row.getByRole("button", { name: "执行更新" });
      await btn.waitFor({ timeout: 10_000 });
      await btn.click();

      const modal = getModal(page);
      await modal.waitFor({ timeout: 10_000 });

      const trigger = modal.locator(".versionTagsTrigger").first();
      await trigger.waitFor({ timeout: 10_000 });
      await trigger.hover();

      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });

      const box = await requireBoundingBox(popover, "versionTagsPopover");
      const x = box.x + box.width / 2;
      const y = box.y + box.height / 2;
      const hit = await page.evaluate(
        ({ x, y }) => {
          const el = document.elementFromPoint(x, y);
          return Boolean(el && el.closest(".versionTagsPopover"));
        },
        { x, y },
      );
      if (!hit)
        throw new Error(
          "Expected versionTagsPopover to be on top (not occluded by modal overlay).",
        );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5) Update confirm modal: no target selector; update request must be pinned to scan-time candidate digest.
  {
    const page = await openStory("pages-servicespage--dashboard-demo");
    try {
      const row = page.locator(".rowLine", { hasText: "api" }).first();
      const btn = row.getByRole("button", { name: "执行更新" });
      await btn.waitFor({ timeout: 10_000 });
      await btn.click();

      const modal = getModal(page);
      await modal.waitFor({ timeout: 10_000 });

      // The confirm modal should not allow selecting a target version.
      const select = modal.locator("select.select");
      if (await select.count())
        throw new Error(
          "Expected no <select> in update confirm modal (version selection removed).",
        );

      await modal.getByRole("button", { name: "执行更新" }).click();

      await page.waitForFunction(
        () => Boolean(globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest),
        null,
        {
          timeout: 10_000,
        },
      );
      const req = await page.evaluate(
        () => globalThis.__DOCKREV_MOCK_DEBUG__?.lastUpdateRequest ?? null,
      );
      if (!req || typeof req !== "object")
        throw new Error("No update request recorded in mock API.");

      // The dashboard demo fixture uses a deterministic digest generator: d('b','9f') => sha256: + 62 * 'b' + '9f'.
      const expectedTargetDigest = `sha256:${"b".repeat(62)}9f`;
      const targetDigest = req.targetDigest;
      if (targetDigest !== expectedTargetDigest) {
        throw new Error(
          `Expected update request targetDigest=${expectedTargetDigest}, got ${String(targetDigest)} (req=${JSON.stringify(req)})`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5b) Services page: inference pending + candidate snapshot pending should read as a unified loading state.
  {
    const page = await openStory("pages-servicespage--dashboard-demo");
    try {
      const apiRow = page.locator(".rowLine", { hasText: "api" }).first();
      const workerRow = page.locator(".rowLine", { hasText: "worker" }).first();
      const apiButton = apiRow.getByRole("button", { name: "执行更新" });
      const workerButton = workerRow.getByRole("button", { name: "执行更新" });

      await apiButton.waitFor({ timeout: 10_000 });
      await workerButton.waitFor({ timeout: 10_000 });
      await apiButton.click();

      const modal = getModal(page);
      await modal.waitFor({ timeout: 10_000 });
      await modal.getByRole("button", { name: "执行更新" }).click();

      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const matchesAction = (text) =>
            ["执行更新", "更新中…", "更新排队中…", "排队中…", "提交中…"].some((label) =>
              text?.includes(label),
            );
          const findButton = (keyword) => {
            const row = rows.find((item) =>
              item.textContent?.includes(keyword),
            );
            if (!row) return null;
            const buttons = Array.from(row.querySelectorAll("button"));
            return (
              buttons.find((btn) => matchesAction(btn.textContent ?? "")) ??
              null
            );
          };
          const apiBtn = findButton("api");
          const workerBtn = findButton("worker");
          if (!apiBtn || !workerBtn) return false;
          const apiSpinning = Boolean(
            apiBtn.querySelector(".btnInlineSpinner"),
          );
          const workerSpinning = Boolean(
            workerBtn.querySelector(".btnInlineSpinner"),
          );
          return apiSpinning && !workerSpinning;
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const apiRow = rows.find((item) => item.textContent?.includes("api"));
          if (!apiRow) return false;
          const btn = Array.from(apiRow.querySelectorAll("button")).find(
            (item) =>
              ["更新中…", "更新排队中…", "排队中…"].some((label) =>
                item.textContent?.includes(label),
              ),
          );
          if (!btn) return false;
          return (
            Boolean(btn.querySelector(".btnInlineSpinner")) &&
            !btn.hasAttribute("disabled")
          );
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const apiRow = rows.find((item) => item.textContent?.includes("api"));
          if (!apiRow) return false;
          const btn = Array.from(apiRow.querySelectorAll("button")).find(
            (item) =>
              ["更新中…", "更新排队中…", "排队中…"].some((label) =>
                item.textContent?.includes(label),
              ),
          );
          if (!btn) return false;
          const hint =
            btn.getAttribute("data-hint") ??
            btn.getAttribute("title") ??
            btn.getAttribute("aria-label");
          return hint === "任务进行中，点击查看任务详情";
        },
        null,
        { timeout: 10_000 },
      );

      const jumped = await page.evaluate(() => {
        const rows = Array.from(document.querySelectorAll(".rowLine"));
        const apiRow = rows.find((item) => item.textContent?.includes("api"));
        if (!apiRow) return false;
        const btn = Array.from(apiRow.querySelectorAll("button")).find((item) =>
          ["更新中…", "更新排队中…", "排队中…"].some((label) =>
            item.textContent?.includes(label),
          ),
        );
        if (!btn) return false;
        btn.click();
        return true;
      });
      if (!jumped)
        throw new Error(
          "Expected active api update button to be clickable for job detail navigation.",
        );
      await page.waitForFunction(
        () => window.location.hash.startsWith("#/queue/job-ui-"),
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const apiRow = rows.find((item) => item.textContent?.includes("api"));
          if (!apiRow) return false;
          const btn = Array.from(apiRow.querySelectorAll("button")).find(
            (item) =>
              ["执行更新", "更新中…", "更新排队中…", "排队中…", "提交中…"].some((label) =>
                item.textContent?.includes(label),
              ),
          );
          if (!btn) return false;
          return !btn.querySelector(".btnInlineSpinner");
        },
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5b2) Services page: row state and stack updates count should settle on the same screen after update success.
  {
    const page = await openStory("pages-servicespage--dashboard-demo");
    try {
      const apiRow = page.locator(".rowLine", { hasText: "api" }).first();
      const applyBtn = apiRow.getByRole("button", { name: "执行更新" });
      await applyBtn.waitFor({ timeout: 10_000 });
      await applyBtn.click();

      const modal = getModal(page);
      await modal.waitFor({ timeout: 10_000 });
      await modal.getByRole("button", { name: "执行更新" }).click();

      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const apiRow = rows.find((item) => item.textContent?.includes("api"));
          if (!apiRow) return false;
          const btn = Array.from(apiRow.querySelectorAll("button")).find(
            (item) =>
              ["执行更新", "更新中…", "更新排队中…", "排队中…", "提交中…"].some((label) =>
                item.textContent?.includes(label),
              ),
          );
          return Boolean(btn?.querySelector(".btnInlineSpinner"));
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const apiRow = rows.find((item) => item.textContent?.includes("api"));
          const groups = Array.from(document.querySelectorAll(".tableGroup"));
          const prodGroup = groups.find((item) =>
            item.textContent?.includes("prod"),
          );
          if (!apiRow || !prodGroup) return false;
          const rowText = apiRow.textContent ?? "";
          const groupText = prodGroup.textContent ?? "";
          return (
            rowText.includes("无更新") &&
            !rowText.includes("可更新") &&
            (groupText.includes("updates 1") || groupText.includes("1 可更新"))
          );
        },
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5d) Service detail page: active apply button should be clickable and jump to job detail.
  {
    const page = await openStory("pages-servicedetailpage--version-anomaly-updatable");
    try {
      const applyBtn = page.getByRole("button", { name: "更新", exact: true });
      await applyBtn.waitFor({ timeout: 10_000 });
      await applyBtn.click();

      const modal = getModal(page);
      await modal.waitFor({ timeout: 10_000 });
      await modal.getByRole("button", { name: "更新", exact: true }).click();

      await page.waitForFunction(
        () => {
          const btn = Array.from(document.querySelectorAll("button")).find(
            (item) =>
              ["更新", "更新中…", "更新排队中…", "排队中…", "提交中…"].includes(
                item.textContent?.trim() ?? "",
              ),
          );
          if (!btn) return false;
          return Boolean(btn.querySelector(".btnInlineSpinner"));
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const btn = Array.from(document.querySelectorAll("button")).find(
            (item) =>
              ["更新中…", "更新排队中…", "排队中…"].includes(item.textContent?.trim() ?? ""),
          );
          if (!btn) return false;
          return (
            Boolean(btn.querySelector(".btnInlineSpinner")) &&
            !btn.hasAttribute("disabled")
          );
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const btn = Array.from(document.querySelectorAll("button")).find(
            (item) =>
              ["更新中…", "更新排队中…", "排队中…"].includes(item.textContent?.trim() ?? ""),
          );
          if (!btn) return false;
          return (
            btn.getAttribute("data-hint") === "任务进行中，点击查看任务详情"
          );
        },
        null,
        { timeout: 10_000 },
      );

      const jumped = await page.evaluate(() => {
        const btn = Array.from(document.querySelectorAll("button")).find(
          (item) =>
            ["更新中…", "更新排队中…", "排队中…"].includes(item.textContent?.trim() ?? ""),
        );
        if (!btn) return false;
        btn.click();
        return true;
      });
      if (!jumped)
        throw new Error(
          "Expected active service-detail update button to be clickable for job detail navigation.",
        );
      await page.waitForFunction(
        () => window.location.hash.startsWith("#/queue/job-ui-"),
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5c4) Services page: a pre-existing running update job should hydrate the row spinner on first load.
  {
    const page = await openStory("pages-servicespage--hydrated-running-update");
    try {
      await page.waitForFunction(
        () => {
          const rows = Array.from(document.querySelectorAll(".rowLine"));
          const apiRow = rows.find((item) => item.textContent?.includes("api"));
          if (!apiRow) return false;
          const btn = Array.from(apiRow.querySelectorAll("button")).find(
            (item) =>
              ["更新中…", "更新排队中…", "排队中…"].some((label) =>
                item.textContent?.includes(label),
              ),
          );
          if (!btn) return false;
          return (
            Boolean(btn.querySelector(".btnInlineSpinner")) &&
            btn.getAttribute("data-hint") === "任务进行中，点击查看任务详情" &&
            !btn.hasAttribute("disabled")
          );
        },
        null,
        { timeout: 10_000 },
      );

      const jumped = await page.evaluate(() => {
        const rows = Array.from(document.querySelectorAll(".rowLine"));
        const apiRow = rows.find((item) => item.textContent?.includes("api"));
        if (!apiRow) return false;
        const btn = Array.from(apiRow.querySelectorAll("button")).find((item) =>
          ["更新中…", "更新排队中…", "排队中…"].some((label) =>
            item.textContent?.includes(label),
          ),
        );
        if (!btn) return false;
        btn.click();
        return true;
      });
      if (!jumped)
        throw new Error(
          "Expected hydrated services update button to stay clickable for job detail navigation.",
        );
      await page.waitForFunction(
        () => window.location.hash === "#/queue/job-1",
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5d2) Service detail page: a pre-existing running update job should hydrate the top-level spinner on first load.
  {
    const page = await openStory(
      "pages-servicedetailpage--hydrated-running-update",
    );
    try {
      await page.waitForFunction(
        () => {
          const btn = Array.from(document.querySelectorAll("button")).find(
            (item) =>
              ["更新中…", "更新排队中…", "排队中…"].includes(item.textContent?.trim() ?? ""),
          );
          if (!btn) return false;
          return (
            Boolean(btn.querySelector(".btnInlineSpinner")) &&
            btn.getAttribute("data-hint") === "任务进行中，点击查看任务详情" &&
            !btn.hasAttribute("disabled")
          );
        },
        null,
        { timeout: 10_000 },
      );

      const jumped = await page.evaluate(() => {
        const btn = Array.from(document.querySelectorAll("button")).find(
          (item) =>
            ["更新中…", "更新排队中…", "排队中…"].includes(item.textContent?.trim() ?? ""),
        );
        if (!btn) return false;
        btn.click();
        return true;
      });
      if (!jumped)
        throw new Error(
          "Expected hydrated service-detail update button to stay clickable for job detail navigation.",
        );
      await page.waitForFunction(
        () => window.location.hash === "#/queue/job-1",
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5d1) Service detail page: badge should settle after update success without leaving the page.
  {
    const page = await openStory("pages-servicedetailpage--version-anomaly-updatable");
    try {
      const applyBtn = page.getByRole("button", { name: "更新", exact: true });
      await applyBtn.waitFor({ timeout: 10_000 });
      await applyBtn.click();

      const modal = getModal(page);
      await modal.waitFor({ timeout: 10_000 });
      await modal.getByRole("button", { name: "更新", exact: true }).click();

      await page.waitForFunction(
        () => {
          const btn = Array.from(document.querySelectorAll("button")).find(
            (item) =>
              ["更新", "更新中…", "更新排队中…", "排队中…", "提交中…"].includes(
                item.textContent?.trim() ?? "",
              ),
          );
          return Boolean(btn?.querySelector(".btnInlineSpinner"));
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const rootText = document.body.textContent ?? "";
          return rootText.includes("无候选") && !rootText.includes(" 可更新 ");
        },
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5e) Services page: inference pending + candidate snapshot pending should read as a unified loading state.
  {
    const page = await openStory(
      "pages-servicespage--inference-pending-candidate-loading",
    );
    try {
      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          const left = triggers[0]?.textContent?.trim();
          const right = triggers[1]?.textContent?.trim();
          return left === "加载中…" && right === "加载中…";
        },
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5f) Service detail page: should follow the same unified loading semantics.
  {
    const page = await openStory(
      "pages-servicedetailpage--inference-pending-candidate-loading",
    );
    try {
      await page.waitForFunction(
        () => {
          const detail = document.querySelector(".svcBannerDetail");
          if (!detail) return false;
          const text = detail.textContent?.replace(/\s+/g, " ").trim() ?? "";
          return (
            text.includes("当前 加载中…") &&
            text.includes("目标 latest") &&
            text.includes("跨度未知")
          );
        },
        null,
        { timeout: 10_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 5g) Service detail page: update history desktop columns must stay aligned even when one row exposes the rollback action.
  {
    const page = await openStory(
      "pages-servicedetailpage--update-history-section-evidence",
    );
    try {
      await assertServiceOperationHistoryColumnsAligned(
        page,
        "pages-servicedetailpage--update-history-section-evidence",
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 6) Version popovers: must read scan-time digest-tags snapshot only (no live /digest-tags fan-out).
  {
    const page = await openStory("components-versiontagspopover--multi-tags");
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null;
      });

      const btn = page.getByRole("button", { name: "v0.8.8-arm64" });
      await btn.waitFor({ timeout: 10_000 });
      await btn.click();

      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });

      await page.waitForFunction(
        () =>
          (globalThis.__DOCKREV_MOCK_DEBUG__?.digestTagsSnapshotCalls ?? 0) > 0,
        null,
        {
          timeout: 10_000,
        },
      );
      const dbg = await page.evaluate(
        () => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null,
      );
      if (!dbg) throw new Error("Missing mock debug object.");

      if (dbg.digestTagsCalls !== 0) {
        throw new Error(
          `Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`,
        );
      }
      if (dbg.digestTagsSnapshotCalls <= 0) {
        throw new Error(
          "Expected at least one /digest-tags-snapshot call, got 0.",
        );
      }

      await popover.getByText("快照时间").waitFor({ timeout: 10_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 7) Snapshot missing: popover should show a clear message and must not retry in a loop.
  {
    const page = await openStory(
      "components-versiontagspopover--missing-snapshot",
    );
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null;
      });

      const btn = page.getByRole("button", { name: "v0.8.8-arm64" });
      await btn.waitFor({ timeout: 10_000 });
      await btn.click();

      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });

      await popover
        .getByText("快照缺失：请先执行一次 check")
        .waitFor({ timeout: 10_000 });

      await page.waitForTimeout(700);
      const dbg = await page.evaluate(
        () => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null,
      );
      if (!dbg) throw new Error("Missing mock debug object.");
      if (dbg.digestTagsCalls !== 0) {
        throw new Error(
          `Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`,
        );
      }
      if (dbg.digestTagsSnapshotCalls !== 1) {
        throw new Error(
          `Expected exactly one /digest-tags-snapshot call, got ${dbg.digestTagsSnapshotCalls}.`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 7b) Hover-opened popovers must stay open after click-pin even after the pointer leaves.
  {
    const page = await openStory("components-versiontagspopover--multi-tags");
    try {
      const trigger = page.getByRole("button", { name: "v0.8.8-arm64" });
      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await trigger.waitFor({ timeout: 10_000 });
      await assertHoverPinKeepsPopoverOpen({
        page,
        trigger,
        popover,
        label: "version-tags hover-pin",
      });
    } finally {
      await page.close().catch(() => {});
    }
  }

  {
    const page = await openStory("components-currentversionpopover--resolved");
    try {
      const trigger = page.getByRole("button", { name: "v5.2.1" }).first();
      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await trigger.waitFor({ timeout: 10_000 });
      await assertHoverPinKeepsPopoverOpen({
        page,
        trigger,
        popover,
        label: "current-version hover-pin",
      });
    } finally {
      await page.close().catch(() => {});
    }
  }

  {
    const page = await openStory("components-statusremark--all-statuses");
    try {
      const trigger = page.locator(".discoveryHistoryTrigger").first();
      const popover = page.locator(
        ".discoveryHistoryPopover[data-state='open']",
      );
      await trigger.waitFor({ timeout: 10_000 });
      await trigger.hover();
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByText("当前候选").waitFor({ timeout: 10_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  {
    const page = await openStory(
      "components-aggregateupdatepreviewlist--all-states",
    );
    try {
      const trigger = page.locator(".discoveryHistoryTrigger").first();
      const popover = page.locator(
        ".discoveryHistoryPopover[data-state='open']",
      );
      await trigger.waitFor({ timeout: 10_000 });
      await trigger.hover();
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByText("当前运行").waitFor({ timeout: 10_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 8) Snapshot pending: trigger text should show loading before hover/click and recover after snapshot is ready.
  {
    const page = await openStory(
      "components-versiontagspopover--pending-snapshot",
    );
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null;
      });

      const trigger = page
        .locator(".versionLine")
        .first()
        .locator(".versionTagsTrigger")
        .nth(1);
      await trigger.waitFor({ timeout: 10_000 });

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const candidate = line.querySelectorAll(".versionTagsTrigger")[1];
          return candidate?.textContent?.trim() === "加载中…";
        },
        null,
        { timeout: 10_000 },
      );

      await trigger.click();

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const candidate = line.querySelectorAll(".versionTagsTrigger")[1];
          return candidate?.textContent?.trim() === "v0.8.8-arm64";
        },
        null,
        { timeout: 10_000 },
      );

      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByText("快照时间").waitFor({ timeout: 10_000 });

      const dbg = await page.evaluate(
        () => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null,
      );
      if (!dbg) throw new Error("Missing mock debug object.");
      if (dbg.digestTagsCalls !== 0) {
        throw new Error(
          `Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`,
        );
      }
      if (dbg.digestTagsSnapshotCalls < 2) {
        throw new Error(
          `Expected at least 2 /digest-tags-snapshot calls for pending->ready, got ${dbg.digestTagsSnapshotCalls}.`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 9) Current version popover should follow the same pending->ready trigger transition.
  {
    const page = await openStory(
      "components-currentversionpopover--pending-snapshot",
    );
    try {
      await page.evaluate(() => {
        if (!globalThis.__DOCKREV_MOCK_DEBUG__) return;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsSnapshotCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.digestTagsCalls = 0;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsSnapshotUrl = null;
        globalThis.__DOCKREV_MOCK_DEBUG__.lastDigestTagsUrl = null;
      });

      const trigger = page
        .locator(".versionLine")
        .first()
        .locator(".versionTagsTrigger")
        .first();
      await trigger.waitFor({ timeout: 10_000 });
      await trigger.click();

      await page.waitForFunction(
        () => {
          const triggerEl = document.querySelector(
            ".versionLine .versionTagsTrigger",
          );
          return triggerEl?.textContent?.trim() === "加载中…";
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const triggerEl = document.querySelector(
            ".versionLine .versionTagsTrigger",
          );
          return triggerEl?.textContent?.trim() === "v0.8.8-arm64";
        },
        null,
        { timeout: 10_000 },
      );

      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByText("快照时间").waitFor({ timeout: 10_000 });

      const dbg = await page.evaluate(
        () => globalThis.__DOCKREV_MOCK_DEBUG__ ?? null,
      );
      if (!dbg) throw new Error("Missing mock debug object.");
      if (dbg.digestTagsCalls !== 0) {
        throw new Error(
          `Expected no /digest-tags calls, got ${dbg.digestTagsCalls} (last=${String(dbg.lastDigestTagsUrl)})`,
        );
      }
      if (dbg.digestTagsSnapshotCalls < 2) {
        throw new Error(
          `Expected at least 2 /digest-tags-snapshot calls for current pending->ready, got ${dbg.digestTagsSnapshotCalls}.`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 10) Candidate popover force refresh must stay local (current trigger unchanged).
  {
    const page = await openStory("components-versiontagspopover--multi-tags");
    try {
      const line = page.locator(".versionLine").first();
      const currentTrigger = line.locator(".versionTagsTrigger").nth(0);
      const candidateTrigger = line.locator(".versionTagsTrigger").nth(1);

      await candidateTrigger.waitFor({ timeout: 10_000 });
      const currentBefore = (await currentTrigger.textContent())?.trim() ?? "";
      const candidateBefore =
        (await candidateTrigger.textContent())?.trim() ?? "";

      await candidateTrigger.click();
      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByRole("button", { name: "强制刷新" }).click();

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          return triggers[1]?.textContent?.trim() === "加载中…";
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        (expected) => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          return triggers[1]?.textContent?.trim() === expected;
        },
        candidateBefore,
        { timeout: 10_000 },
      );

      const currentAfter = (await currentTrigger.textContent())?.trim() ?? "";
      if (currentAfter !== currentBefore) {
        throw new Error(
          `Expected current trigger to stay unchanged (${currentBefore} -> ${currentAfter}).`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 11) Current popover force refresh must stay local (candidate trigger unchanged).
  {
    const page = await openStory("components-versiontagspopover--multi-tags");
    try {
      const line = page.locator(".versionLine").first();
      const currentTrigger = line.locator(".versionTagsTrigger").nth(0);
      const candidateTrigger = line.locator(".versionTagsTrigger").nth(1);

      await candidateTrigger.waitFor({ timeout: 10_000 });
      const candidateBefore =
        (await candidateTrigger.textContent())?.trim() ?? "";

      await currentTrigger.click();
      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByRole("button", { name: "强制刷新" }).click();

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          return triggers[0]?.textContent?.trim() === "加载中…";
        },
        null,
        { timeout: 10_000 },
      );

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          return triggers[0]?.textContent?.trim() === "v0.8.7";
        },
        null,
        { timeout: 10_000 },
      );

      const candidateAfter =
        (await candidateTrigger.textContent())?.trim() ?? "";
      if (candidateAfter !== candidateBefore) {
        throw new Error(
          `Expected candidate trigger to stay unchanged (${candidateBefore} -> ${candidateAfter}).`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 12) Same-digest local refresh should refresh the sibling side after ready, without loading it.
  {
    const page = await openStory("components-versiontagspopover--same-digest");
    try {
      const line = page.locator(".versionLine").first();
      const currentTrigger = line.locator(".versionTagsTrigger").nth(0);
      const candidateTrigger = line.locator(".versionTagsTrigger").nth(1);

      await candidateTrigger.waitFor({ timeout: 10_000 });
      const currentBefore = (await currentTrigger.textContent())?.trim() ?? "";

      await candidateTrigger.click();
      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByRole("button", { name: "强制刷新" }).click();

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          return triggers[1]?.textContent?.trim() === "加载中…";
        },
        null,
        { timeout: 10_000 },
      );

      const currentDuring = (await currentTrigger.textContent())?.trim() ?? "";
      if (currentDuring !== currentBefore) {
        throw new Error(
          `Expected same-digest current trigger to avoid loading during candidate refresh (${currentBefore} -> ${currentDuring}).`,
        );
      }

      await page.waitForFunction(
        () => {
          const line = document.querySelector(".versionLine");
          if (!line) return false;
          const triggers = line.querySelectorAll(".versionTagsTrigger");
          if (triggers.length < 2) return false;
          const current = triggers[0]?.textContent?.trim() ?? "";
          const candidate = triggers[1]?.textContent?.trim() ?? "";
          if (!current || !candidate) return false;
          return current !== "加载中…" && candidate !== "加载中…";
        },
        null,
        { timeout: 40_000 },
      );
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 13) ServicesPage: local refresh should backfill resolvedTag into host state.
  {
    const page = await openStory(
      "pages-servicespage--version-tags-popover-demo",
    );
    try {
      const line = page.locator(".versionLine").first();
      const currentTrigger = line.locator(".versionTagsTrigger").nth(0);
      // `getByRole(..., { name })` does substring matching by default and may match the
      // row container / candidate button as well. Require an exact match for the raw tag trigger.
      const rawTrigger = page
        .getByRole("button", { name: "0.8", exact: true })
        .first();

      await currentTrigger.waitFor({ timeout: 10_000 });
      await currentTrigger.click();

      const popover = page.locator(".versionTagsPopover[data-state='open']");
      await popover.waitFor({ timeout: 10_000 });
      await popover.getByRole("button", { name: "强制刷新" }).click();

      await page.waitForFunction(
        () => {
          const trigger =
            Array.from(document.querySelectorAll("button.versionTagsTrigger"))
              .map((node) => node.textContent?.trim() ?? "")
              .find((text) => text === "v0.8.7" || text === "加载中…") ?? "";
          return trigger === "v0.8.7";
        },
        null,
        { timeout: 10_000 },
      );

      // Close the popover so we can assert the raw-tag popover reflects resolvedTag.
      await currentTrigger.click();

      await rawTrigger.waitFor({ timeout: 10_000 });
      await rawTrigger.click();
      const rawPopover = page.locator(".versionTagsPopover[data-state='open']");
      await rawPopover.waitFor({ timeout: 10_000 });
      // `resolvedTag` can also appear in the inference section ("来源: resolvedTag"), so scope
      // the assertion to the "当前镜像" section.
      const imageSection = rawPopover.locator(".versionTagsPopoverSection", {
        hasText: "当前镜像",
      });
      const resolvedLine = imageSection.locator(".muted", {
        hasText: "resolvedTag",
      });
      await resolvedLine.waitFor({ timeout: 10_000 });
      await resolvedLine
        .getByText("v0.8.7", { exact: true })
        .waitFor({ timeout: 10_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 14) Repo link inference should preview in service detail immediately.
  {
    const page = await openStory(
      "pages-interactiveapp--repo-link-editing-flow",
    );
    try {
      await page.locator('[data-service-detail-tab="settings"]').click();
      await page.waitForFunction(
        () => document.querySelector(".serviceSafeguardCard") != null,
        null,
        { timeout: 10_000 },
      );
      await page
        .locator(".serviceSafeguardCard")
        .getByRole("button", { name: "打开" })
        .click();
      await page.getByRole("heading", { name: "服务保护设置" }).waitFor({
        timeout: 10_000,
      });

      const repoInput = page.getByPlaceholder("https://github.com/owner/repo");
      await repoInput.waitFor({ timeout: 10_000 });
      await page
        .getByText("清空并保存会禁用后续自动补齐；再次手动推断并保存可恢复。", {
          exact: true,
        })
        .waitFor({ timeout: 10_000 });

      const detailHeader = page.locator(
        '[data-service-detail-context="status-summary"]',
      );
      const detailRegistryLink = detailHeader.locator(
        '[data-link-kind="registry"]',
      );
      await detailRegistryLink.waitFor({ timeout: 10_000 });
      const detailRegistryHref = await detailRegistryLink.getAttribute("href");
      if (detailRegistryHref !== "https://ghcr.io/acme/api") {
        throw new Error(
          `Expected detail registry href to strip the tag, got ${detailRegistryHref}.`,
        );
      }

      const detailRepoLinksBefore = await detailHeader
        .locator('[data-link-kind="repo"]')
        .count();
      if (detailRepoLinksBefore !== 0) {
        throw new Error(
          `Expected no repo link in service detail header before inference, got ${detailRepoLinksBefore}.`,
        );
      }

      await page.getByRole("button", { name: "重新推断代码仓库" }).click();
      await page.waitForFunction(
        () => {
          const input = document.querySelector(
            'input[placeholder="https://github.com/owner/repo"]',
          );
          return (
            input instanceof HTMLInputElement &&
            input.value === "https://github.com/acme/api"
          );
        },
        null,
        { timeout: 10_000 },
      );

      await detailHeader
        .locator('[data-link-kind="repo"]')
        .waitFor({ timeout: 10_000 });
    } finally {
      await page.close().catch(() => {});
    }
  }

  // 15) Digest-pinned services should preserve the digest in display metadata instead of falling back to :latest.
  {
    const page = await openStory(
      "pages-servicespage--digest-pinned-image-display",
    );
    try {
      const prodGroup = page
        .locator(".tableGroup", { hasText: "prod" })
        .first();
      await prodGroup.waitFor({ timeout: 10_000 });
      const apiRow = prodGroup.locator(".rowLine", { hasText: "api" }).first();
      await apiRow.waitFor({ timeout: 10_000 });

      const imageRow = apiRow.locator(".imageLinkRow");
      await imageRow.waitFor({ timeout: 10_000 });
      const imageTitle = await imageRow.getAttribute("title");
      if (
        imageTitle !==
        "acme/api@sha256:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
      ) {
        throw new Error(
          `Expected digest-pinned image title to preserve @sha256, got ${imageTitle}.`,
        );
      }

      const registryHref = await imageRow
        .locator('[data-link-kind="registry"]')
        .getAttribute("href");
      if (registryHref !== "https://ghcr.io/acme/api") {
        throw new Error(
          `Expected digest-pinned registry href to point at the repo page, got ${registryHref}.`,
        );
      }
    } finally {
      await page.close().catch(() => {});
    }
  }
}

async function main() {
  const { url: cliUrl, passthrough } = parseArgs(process.argv.slice(2));
  const targetUrl = cliUrl ?? process.env.TARGET_URL ?? null;
  const lightLogsOnly = process.env.DOCKREV_TEST_STORYBOOK_LIGHT_LOGS_ONLY === "1";
  const smokeOnly = process.env.DOCKREV_TEST_STORYBOOK_SMOKE_ONLY === "1";
  const interactiveOnly = process.env.DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY === "1";
  const rollbackRaceOnly = process.env.DOCKREV_TEST_STORYBOOK_ROLLBACK_RACE_ONLY === "1";
  if (smokeOnly && (interactiveOnly || rollbackRaceOnly)) {
    throw new Error(
      "DOCKREV_TEST_STORYBOOK_SMOKE_ONLY and DOCKREV_TEST_STORYBOOK_INTERACTIVE_ONLY cannot both be set.",
    );
  }

  if (targetUrl) {
    if (passthrough.length > 0) {
      console.error(
        "Only --url is supported for now; extra args are not accepted.",
      );
      process.exit(2);
    }
    const { chromium } = await import("playwright");
    const browser = await chromium.launch();
    const storyIds = await getStoryIds(targetUrl);
    try {
      if (lightLogsOnly) {
        await assertServiceLogsLightContrast({ baseUrl: targetUrl, browser });
      } else {
        if (!interactiveOnly && !rollbackRaceOnly) {
          await runSmoke({
            baseUrl: targetUrl,
            storyIds: selectSmokeShard(storyIds),
            browser,
          });
        }
        if (rollbackRaceOnly) await runRollbackRefreshRace({ baseUrl: targetUrl, browser });
        else if (!smokeOnly) await runInteractive({ baseUrl: targetUrl, browser });
      }
    } finally {
      await browser.close().catch(() => {});
    }
    return;
  }

  await ensureStaticBuild();
  const requestedPort = parsePort(
    process.env.DOCKREV_TEST_STORYBOOK_PORT,
    DEFAULT_PORT,
  );
  const port = process.env.DOCKREV_TEST_STORYBOOK_PORT
    ? requestedPort
    : await findAvailablePort(requestedPort);
  const server = startStaticServer({ port });
  try {
    await server.listen();
  } catch (error) {
    if (error && typeof error === "object" && error.code === "EADDRINUSE") {
      console.error(`Port ${port} is already in use.`);
      process.exit(1);
    }
    throw error;
  }

  try {
    const localUrl = `http://127.0.0.1:${port}`;
    await waitForHttpOk(localUrl);
    if (passthrough.length > 0) {
      console.error(
        "Only --url is supported for now; extra args are not accepted.",
      );
      process.exit(2);
    }
    const { chromium } = await import("playwright");
    const browser = await chromium.launch();
    const storyIds = await getStoryIds(localUrl);
    try {
      if (lightLogsOnly) {
        await assertServiceLogsLightContrast({ baseUrl: localUrl, browser });
      } else {
        if (!interactiveOnly && !rollbackRaceOnly) {
          await runSmoke({
            baseUrl: localUrl,
            storyIds: selectSmokeShard(storyIds),
            browser,
          });
        }
        if (rollbackRaceOnly) await runRollbackRefreshRace({ baseUrl: localUrl, browser });
        else if (!smokeOnly) await runInteractive({ baseUrl: localUrl, browser });
      }
    } finally {
      await browser.close().catch(() => {});
    }
  } finally {
    await server.cleanup();
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
