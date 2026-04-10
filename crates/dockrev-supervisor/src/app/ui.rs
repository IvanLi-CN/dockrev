use super::meta::{SupervisorMeta, trimmed_non_empty};

mod style;

use style::STYLE_CSS;

fn escape_html_text(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

fn escape_html_attr(input: &str) -> String {
    escape_html_text(input)
}

const THEME_STORAGE_KEY: &str = "dockrev:theme";

pub(crate) fn render_ui(base_path: &str, meta: &SupervisorMeta) -> String {
    let version_html = if let Some(release_url) = trimmed_non_empty(meta.release_url.as_deref()) {
        format!(
            r#"<a href="{url}" target="_blank" rel="noopener noreferrer"><code>{value}</code></a>"#,
            url = escape_html_attr(release_url),
            value = escape_html_text(&meta.version)
        )
    } else {
        format!("<code>{}</code>", escape_html_text(&meta.version))
    };
    let repository_html = format!(
        r#"<a href="{url}" target="_blank" rel="noopener noreferrer">{value}</a>"#,
        url = escape_html_attr(&meta.repository),
        value = escape_html_text(&meta.repository)
    );
    let developer_html = format!(
        r#"<a href="{url}" target="_blank" rel="noopener noreferrer">{value}</a>"#,
        url = escape_html_attr(&meta.developer_url),
        value = escape_html_text(&meta.developer_name)
    );

    // Minimal, dependency-free console. Uses same-origin fetch under base_path.
    format!(
        r#"<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Dockrev Supervisor</title>
    <link rel="icon" type="image/png" href="{base_path}/favicon.png" />
    <script>
      (() => {{
        const STORAGE_KEY = {theme_storage_key_json};
        const root = document.documentElement;
        const media = window.matchMedia('(prefers-color-scheme: dark)');

        function normalizeTheme(value) {{
          return value === 'light' || value === 'dark' ? value : null;
        }}

        function preferredTheme() {{
          return media.matches ? 'dark' : 'light';
        }}

        function readStoredTheme() {{
          try {{
            return normalizeTheme(window.localStorage.getItem(STORAGE_KEY));
          }} catch (_error) {{
            return null;
          }}
        }}

        function applyTheme(theme) {{
          root.dataset.theme = theme;
          root.style.colorScheme = theme;
        }}

        function syncThemeFromPreference() {{
          const storedTheme = readStoredTheme();
          const theme = storedTheme || preferredTheme();
          applyTheme(theme);
          return storedTheme;
        }}

        window.__dockrevSupervisorTheme = {{
          storageKey: STORAGE_KEY,
          mediaQuery: media,
          applyTheme,
          syncThemeFromPreference,
          hasStoredTheme() {{
            return !!readStoredTheme();
          }},
        }};

        syncThemeFromPreference();
      }})();
    </script>
    <style>
{style_css}
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="panel masthead" data-panel="masthead">
        <div class="mastheadRow">
          <div class="titleBlock">
            <div class="eyebrow">Supervisor Console</div>
            <div class="brandRow">
              <div class="brandMark">
                <img src="{base_path}/favicon.png" alt="" aria-hidden="true" width="26" height="26" />
              </div>
              <div>
                <h1>Dockrev 自我升级（Supervisor）</h1>
                <p class="intro">该页面独立于 Dockrev 生命周期；Dockrev 重启期间仍可用。操作、日志与状态会按工作流优先级排布。</p>
              </div>
            </div>

          </div>
          <a class="linkButton" href="/">返回 Dockrev</a>
        </div>
      </section>

      <section class="panel actionDeck" data-panel="action-deck">
        <div class="sectionLead">
          <div class="sectionEyebrow">Action deck</div>
          <h2>升级控制</h2>
          <div class="muted sectionNote">失败会尝试回滚到 previous digest（如可用）；操作过程会持续留在当前页。</div>
        </div>
        <div class="actionDeckGrid">
          <label class="fieldBlock" for="tag">
            <span class="fieldLabel">Target tag</span>
            <input id="tag" value="latest" />
          </label>
          <div class="fieldHint actionFieldHint">默认使用 <code>latest</code>，也支持输入固定 tag 进行验证或升级。</div>
          <div class="buttonGroup buttonGroup-main">
            <button id="dry">预览（dry-run）</button>
            <button id="apply" class="primary">开始升级（apply）</button>
          </div>
          <div class="actionCallout muted">先用 dry-run 看目标 tag 与 digest，再决定 apply；operation 结束后可直接回滚。</div>
          <div class="buttonGroup buttonGroup-aux">
            <div id="rollbackWrap" class="popWrap">
              <button id="rollback" aria-haspopup="dialog" aria-expanded="false">回滚</button>
              <div id="rollbackPop" class="popCard" role="dialog" aria-modal="false" hidden>
                <div class="popTitle">确认手动回滚？</div>
                <div class="muted">将尝试回滚到 previous digest，并可能触发容器重启。</div>
                <div class="muted">opId: <code id="rollbackOpId">-</code></div>
                <div class="popActions">
                  <button id="rollbackCancel">取消</button>
                  <button id="rollbackConfirm" class="danger">确认回滚</button>
                </div>
              </div>
            </div>
            <button id="refresh">刷新</button>
          </div>
        </div>
      </section>

      <section class="panel workspacePanel" data-panel="workspace">
        <div class="sectionLead">
          <div class="sectionEyebrow">Operations workspace</div>
          <h2>日志与运行态</h2>
        </div>
        <div id="workspaceGrid" class="workspaceGrid">
          <section class="logPanel">
            <div class="logHeader">
              <div>
                <div class="sectionEyebrow">Console</div>
                <div id="logTitle" class="logTitle">loading…</div>
              </div>
              <div id="logSummary" class="logSummary muted">等待日志…</div>
            </div>
            <pre id="logs"></pre>
          </section>
          <aside class="opsSidebar">
            <section class="statusSidebar">
              <div class="statusSidebarHeader">
                <div class="statusSidebarTitle">
                  <div class="sectionEyebrow">Live status</div>
                  <div id="statusState" class="statusValue statusValue-lg">loading…</div>
                  <div id="statusSummary" class="statusMeta">等待首次轮询结果…</div>
                </div>
                <div id="statusTone" class="statusTone" aria-live="polite">loading…</div>
              </div>
              <div class="snapshotGrid">
                <article class="snapshotItem">
                  <div class="statusLabelRow">
                    <div class="statusLabel">Current opId</div>
                    <button id="copyOpId" class="copyButton" type="button" aria-label="复制当前 opId"></button>
                  </div>
                  <div id="statusOpId" class="statusValue">-</div>
                </article>
                <article class="snapshotItem">
                  <div class="statusLabel">Current step</div>
                  <div id="statusStep" class="statusValue">-</div>
                  <div id="statusMode" class="statusMeta">mode -</div>
                </article>
                <article class="snapshotItem">
                  <div class="statusLabel">Timestamps</div>
                  <div id="statusStartedAt" class="statusValue">-</div>
                  <div id="statusUpdatedAt" class="statusMeta">updated -</div>
                </article>
              </div>
            </section>
            <aside id="historyRail" class="historyRail">
              <div class="historyRailHeader">
                <div class="sectionEyebrow">Recent operations</div>
                <div id="historyHint" class="muted historyHint">loading…</div>
              </div>
              <div id="historyList" class="historyList"></div>
            </aside>
          </aside>
          <section class="panel statusPanel" data-panel="status-grid">
            <div class="sectionLead">
              <div class="sectionEyebrow">Operation detail</div>
              <h2>镜像与进度</h2>
              <div class="muted sectionNote">关键引用默认直出，方便复制与排障。</div>
            </div>
            <div class="statusGrid">
              <article class="statusTile statusTile-full">
                <div class="statusLabel">Progress message</div>
                <div id="statusProgressMessage" class="statusCodeInline">-</div>
              </article>
              <article class="statusTile statusTile-wide">
                <div class="statusLabelRow">
                  <div class="statusLabel">Target</div>
                  <button id="copyTarget" class="copyButton" type="button" aria-label="复制 target 引用"></button>
                </div>
                <pre id="statusTarget" class="statusCode">-</pre>
              </article>
              <article class="statusTile statusTile-wide">
                <div class="statusLabelRow">
                  <div class="statusLabel">Previous</div>
                  <button id="copyPrevious" class="copyButton" type="button" aria-label="复制 previous 引用"></button>
                </div>
                <pre id="statusPrevious" class="statusCode">-</pre>
              </article>
            </div>
          </section>
        </div>
      </section>

      <footer class="metaFooter muted">
        <div class="metaPill">
          <span class="metaLabel">Supervisor 版本</span>
          <span class="metaValue">{version_html}</span>
        </div>
        <div class="metaPill">
          <span class="metaLabel">开源仓库</span>
          <span class="metaValue">{repository_html}</span>
        </div>
        <div class="metaPill">
          <span class="metaLabel">开发者</span>
          <span class="metaValue">{developer_html}</span>
        </div>
      </footer>
    </main>

    <script>
      const base = {base_path_json};
      const themeController = window.__dockrevSupervisorTheme;
      const themeMedia = themeController?.mediaQuery || window.matchMedia('(prefers-color-scheme: dark)');
      let activeOpId = null;
      let latestOpId = null;
      let latestOpMarker = null;
      let latestHasNewer = false;
      let lastKnownSelfUpgradeState = null;

      const dryBtn = document.getElementById('dry');
      const applyBtn = document.getElementById('apply');
      const rollbackBtn = document.getElementById('rollback');
      const rollbackWrap = document.getElementById('rollbackWrap');
      const rollbackPop = document.getElementById('rollbackPop');
      const rollbackOpId = document.getElementById('rollbackOpId');
      const rollbackConfirmBtn = document.getElementById('rollbackConfirm');
      const rollbackCancelBtn = document.getElementById('rollbackCancel');
      const statusToneEl = document.getElementById('statusTone');
      const statusStateEl = document.getElementById('statusState');
      const statusSummaryEl = document.getElementById('statusSummary');
      const statusOpIdEl = document.getElementById('statusOpId');
      const statusStepEl = document.getElementById('statusStep');
      const statusModeEl = document.getElementById('statusMode');
      const statusStartedAtEl = document.getElementById('statusStartedAt');
      const statusUpdatedAtEl = document.getElementById('statusUpdatedAt');
      const statusProgressMessageEl = document.getElementById('statusProgressMessage');
      const statusTargetEl = document.getElementById('statusTarget');
      const statusPreviousEl = document.getElementById('statusPrevious');
      const copyOpIdBtn = document.getElementById('copyOpId');
      const copyTargetBtn = document.getElementById('copyTarget');
      const copyPreviousBtn = document.getElementById('copyPrevious');
      const historyRailEl = document.getElementById('historyRail');
      const historyListEl = document.getElementById('historyList');
      const historyHintEl = document.getElementById('historyHint');
      const workspaceGridEl = document.getElementById('workspaceGrid');
      const logTitleEl = document.getElementById('logTitle');
      const logSummaryEl = document.getElementById('logSummary');
      const logsEl = document.getElementById('logs');
      const toUrl = (p) => base.replace(/\/$/, '') + '/' + p.replace(/^\//, '');
      let rollbackPopOpen = false;
      let rollbackPendingOpId = null;

      function syncTheme() {{
        if (themeController?.syncThemeFromPreference) {{
          themeController.syncThemeFromPreference();
        }}
      }}

      function handleSystemThemeChange() {{
        if (themeController?.hasStoredTheme && themeController.hasStoredTheme()) return;
        syncTheme();
      }}

      async function fetchJson(path, init = {{}}) {{
        const res = await fetch(toUrl(path), {{
          headers: {{ 'content-type': 'application/json' }},
          ...init,
        }});
        if (!res.ok) {{
          const text = await res.text();
          throw new Error(`${{res.status}} ${{text}}`);
        }}
        return res.json();
      }}

      function canRollback(st) {{
        return !!st.opId
          && hasPreviousRollbackTarget(st?.previous)
          && (st.state === 'failed' || st.state === 'rolled_back' || st.state === 'succeeded');
      }}

      function setRollbackPopOpen(open) {{
        rollbackPopOpen = open;
        rollbackPop.hidden = !open;
        rollbackBtn.setAttribute('aria-expanded', open ? 'true' : 'false');
        if (!open) rollbackPendingOpId = null;
      }}

      function syncRollbackState(st) {{
        const allowed = canRollback(st);
        rollbackBtn.disabled = !allowed;
        if (!allowed) {{
          setRollbackPopOpen(false);
          rollbackOpId.textContent = '-';
          return;
        }}
        if (rollbackPopOpen) rollbackOpId.textContent = st.opId || '-';
      }}

      function setRunningButton(button, running) {{
        button.classList.toggle('btnRunning', running);
        button.setAttribute('aria-busy', running ? 'true' : 'false');
      }}

      function syncUpgradeActionState(st) {{
        const running = !!st && st.state === 'running';
        const runningUpgrade = running && st?.progress?.step !== 'rollback';
        const mode = st?.request?.mode;
        dryBtn.disabled = running;
        applyBtn.disabled = running;
        setRunningButton(dryBtn, runningUpgrade && mode === 'dry-run');
        setRunningButton(applyBtn, runningUpgrade && mode === 'apply');
      }}

      function normalizeState(value) {{
        return value || 'unknown';
      }}

      const ICONIFY_ICONS = {{
        copy: {{
          name: 'mdi:content-copy',
          width: 24,
          height: 24,
          body: '<path fill="currentColor" d="M19 21H8V7h11m0-2H8a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h11a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2m-3-4H4a2 2 0 0 0-2 2v14h2V3h12V1Z"/>'
        }},
        copied: {{
          name: 'mdi:check-bold',
          width: 24,
          height: 24,
          body: '<path fill="currentColor" d="m9 20.42l-6.21-6.21l2.83-2.83L9 14.77l9.88-9.89l2.83 2.83L9 20.42Z"/>'
        }}
      }};

      function renderCopyButtonIcon(button, state = 'copy') {{
        if (!button) return;
        const icon = ICONIFY_ICONS[state] || ICONIFY_ICONS.copy;
        button.dataset.icon = icon.name;
        button.innerHTML = `<svg viewBox="0 0 ${{icon.width}} ${{icon.height}}" aria-hidden="true" focusable="false">${{icon.body}}</svg>`;
      }}

      function setCopyButtonTooltip(button, text) {{
        if (!button) return;
        button.dataset.tooltip = text || button.dataset.defaultTooltip || button.getAttribute('aria-label') || '';
      }}

      function resetCopyButton(button) {{
        if (!button) return;
        button.classList.remove('copied', 'failed');
        renderCopyButtonIcon(button, 'copy');
        setCopyButtonTooltip(button, button.dataset.defaultTooltip);
      }}

      function setCopyButtonValue(button, value) {{
        if (!button) return;
        const nextValue = String(value || '').trim();
        button.dataset.copyValue = nextValue;
        button.disabled = !nextValue || nextValue === '-';
        resetCopyButton(button);
      }}

      function fallbackCopyText(text) {{
        const area = document.createElement('textarea');
        area.value = text;
        area.setAttribute('readonly', 'true');
        area.style.position = 'fixed';
        area.style.top = '-9999px';
        document.body.appendChild(area);
        area.select();
        const copied = document.execCommand('copy');
        area.remove();
        if (!copied) throw new Error('execCommand copy failed');
      }}

      async function writeClipboardText(text) {{
        if (navigator.clipboard?.writeText) {{
          try {{
            await navigator.clipboard.writeText(text);
            return;
          }} catch (_error) {{
            fallbackCopyText(text);
            return;
          }}
        }}
        fallbackCopyText(text);
      }}

      function flashCopyButton(button, state) {{
        if (!button) return;
        window.clearTimeout(button._copyTimer);
        button.classList.remove('copied', 'failed');
        if (state === 'copied') {{
          button.classList.add('copied');
          renderCopyButtonIcon(button, 'copied');
          setCopyButtonTooltip(button, '已复制');
        }} else if (state === 'failed') {{
          button.classList.add('failed');
          renderCopyButtonIcon(button, 'copy');
          setCopyButtonTooltip(button, '复制失败');
        }} else {{
          renderCopyButtonIcon(button, 'copy');
          setCopyButtonTooltip(button, button.dataset.defaultTooltip);
        }}
        if (state) {{
          button._copyTimer = window.setTimeout(() => resetCopyButton(button), 1400);
        }}
      }}

      function bindCopyButton(button) {{
        if (!button) return;
        button.dataset.defaultTooltip = button.getAttribute('aria-label') || '';
        renderCopyButtonIcon(button, 'copy');
        setCopyButtonTooltip(button, button.dataset.defaultTooltip);
        button.addEventListener('click', async () => {{
          const value = button.dataset.copyValue || '';
          if (!value) return;
          try {{
            await writeClipboardText(value);
            flashCopyButton(button, 'copied');
          }} catch (_error) {{
            flashCopyButton(button, 'failed');
          }}
        }});
      }}

      bindCopyButton(copyOpIdBtn);
      bindCopyButton(copyTargetBtn);
      bindCopyButton(copyPreviousBtn);

      function statusToneClass(state) {{
        const normalized = normalizeState(state);
        if (normalized === 'failed' || normalized === 'rolled_back' || normalized === 'offline') return `statusTone state-${{normalized}}`;
        if (normalized === 'running' || normalized === 'succeeded' || normalized === 'idle' || normalized === 'unknown') return `statusTone state-${{normalized}}`;
        return 'statusTone state-unknown';
      }}

      function stateBadgeClass(state) {{
        const normalized = normalizeState(state);
        if (normalized === 'failed' || normalized === 'rolled_back') return `stateBadge stateBadge-${{normalized}}`;
        if (normalized === 'running' || normalized === 'succeeded' || normalized === 'idle' || normalized === 'unknown') return `stateBadge stateBadge-${{normalized}}`;
        return 'stateBadge stateBadge-unknown';
      }}

      function stateDotClass(state) {{
        const normalized = normalizeState(state);
        if (normalized === 'failed' || normalized === 'rolled_back') return `stateDot stateDot-${{normalized}}`;
        if (normalized === 'running' || normalized === 'succeeded' || normalized === 'idle' || normalized === 'unknown') return `stateDot stateDot-${{normalized}}`;
        return 'stateDot stateDot-unknown';
      }}

      function pad2(v) {{
        return String(v).padStart(2, '0');
      }}

      function formatTimestamp(ts) {{
        const d = new Date(ts || '');
        if (Number.isNaN(d.getTime())) return '-';
        return `${{d.getFullYear()}}-${{pad2(d.getMonth() + 1)}}-${{pad2(d.getDate())}} ${{pad2(d.getHours())}}:${{pad2(d.getMinutes())}}:${{pad2(d.getSeconds())}}`;
      }}

      function formatHistoryTime(ts) {{
        const d = new Date(ts || '');
        if (Number.isNaN(d.getTime())) return '-- --:--';
        return `${{pad2(d.getMonth() + 1)}}-${{pad2(d.getDate())}} ${{pad2(d.getHours())}}:${{pad2(d.getMinutes())}}`;
      }}

      function shortOpId(opId) {{
        return String(opId || '-').slice(-6);
      }}

      function operationMarker(op) {{
        if (!op?.opId) return null;
        return `${{op.opId}}:${{op.updatedAt || ''}}:${{(op.logs || []).length}}`;
      }}

      function formatTargetRef(target) {{
        const image = target?.image || '-';
        const tag = target?.tag ? `:${{target.tag}}` : '';
        const digest = target?.digest ? `@${{target.digest}}` : '';
        return `${{image}}${{tag}}${{digest}}`;
      }}

      function hasPreviousRollbackTarget(previous) {{
        const tag = String(previous?.tag || '').trim();
        return !!previous?.digest || (tag && tag !== 'unknown');
      }}

      function formatPreviousRef(previous) {{
        if (!hasPreviousRollbackTarget(previous)) return '-';
        const tag = previous?.tag || '-';
        return `${{tag}}${{previous?.digest ? '@' + previous.digest : ''}}`;
      }}

      function formatPreviousCopyRef(target, previous) {{
        const repo = String(target?.image || '').trim();
        const digest = String(previous?.digest || '').trim();
        if (digest) return repo ? `${{repo}}@${{digest}}` : digest;

        const tag = String(previous?.tag || '').trim();
        if (!tag || tag === 'unknown') return '';
        if (!repo) return tag;
        if (tag === repo
          || tag.startsWith(`${{repo}}:`)
          || tag.startsWith(`${{repo}}@`)
          || tag.includes('/')
          || tag.includes(':')
          || tag.includes('@')) {{
          return tag;
        }}
        return `${{repo}}:${{tag}}`;
      }}

      const LOG_TOKEN_PATTERN = /(sha256:[0-9a-f]{{12,}}|sup_[A-Za-z0-9]+|(?:[a-z0-9.-]+\.[a-z]{{2,}}(?::\d+)?\/[a-z0-9._/-]+(?::[\w.-]+)?(?:@sha256:[0-9a-f]{{12,}})?))/gi;

      function setLogsText(text) {{
        logsEl.replaceChildren(document.createTextNode(text));
      }}

      function appendLogToken(parent, className, text) {{
        if (!text) return;
        const span = document.createElement('span');
        span.className = className;
        span.textContent = text;
        parent.appendChild(span);
      }}

      function appendHighlightedMessage(parent, message) {{
        const text = String(message || '');
        LOG_TOKEN_PATTERN.lastIndex = 0;
        let cursor = 0;
        let match = LOG_TOKEN_PATTERN.exec(text);
        while (match) {{
          const value = match[0];
          const index = match.index;
          if (index > cursor) {{
            parent.appendChild(document.createTextNode(text.slice(cursor, index)));
          }}
          let className = 'logToken-ref';
          if (value.startsWith('sha256:')) {{
            className = 'logToken-digest';
          }} else if (value.startsWith('sup_')) {{
            className = 'logToken-opid';
          }}
          appendLogToken(parent, className, value);
          cursor = index + value.length;
          match = LOG_TOKEN_PATTERN.exec(text);
        }}
        if (cursor < text.length) {{
          parent.appendChild(document.createTextNode(text.slice(cursor)));
        }}
      }}

      function renderLogEntries(logs) {{
        if (!Array.isArray(logs) || !logs.length) {{
          setLogsText('暂无日志');
          return;
        }}
        const fragment = document.createDocumentFragment();
        for (const line of logs) {{
          const row = document.createElement('span');
          const level = String(line?.level || 'info').toUpperCase();
          row.className = `logLine logLine-${{level.toLowerCase()}}`;
          appendLogToken(row, 'logToken-ts', `[${{line?.ts || '-'}}]`);
          row.appendChild(document.createTextNode(' '));
          appendLogToken(row, `logToken-level logLevel-${{level.toLowerCase()}}`, level);
          row.appendChild(document.createTextNode(' '));
          const msg = document.createElement('span');
          msg.className = 'logToken-msg';
          appendHighlightedMessage(msg, line?.msg || '');
          row.appendChild(msg);
          fragment.appendChild(row);
        }}
        logsEl.replaceChildren(fragment);
      }}

      function renderStatus(st) {{
        const opIdText = st?.opId || '-';
        const targetText = formatTargetRef(st?.target);
        const previousText = formatPreviousRef(st?.previous);
        const previousCopyText = formatPreviousCopyRef(st?.target, st?.previous);
        statusToneEl.className = statusToneClass(st?.state);
        statusToneEl.textContent = normalizeState(st?.state);
        statusStateEl.textContent = normalizeState(st?.state);
        statusSummaryEl.textContent = `${{st?.request?.mode || 'mode -'}} · auto-refresh 1.5s`;
        statusOpIdEl.textContent = opIdText;
        statusStepEl.textContent = st?.progress?.step || '-';
        statusModeEl.textContent = `mode ${{st?.request?.mode || '-'}}`;
        statusStartedAtEl.textContent = formatTimestamp(st?.startedAt);
        statusUpdatedAtEl.textContent = `updated ${{formatTimestamp(st?.updatedAt)}}`;
        statusProgressMessageEl.textContent = st?.progress?.message || '-';
        statusTargetEl.textContent = targetText;
        statusPreviousEl.textContent = previousText;
        setCopyButtonValue(copyOpIdBtn, st?.opId || '');
        setCopyButtonValue(copyTargetBtn, targetText !== '-' ? targetText : '');
        setCopyButtonValue(copyPreviousBtn, previousCopyText);
      }}

      function renderOffline(error) {{
        const cached = lastKnownSelfUpgradeState;
        const hasCachedState = !!cached;
        const lastSeen = cached?.updatedAt ? formatTimestamp(cached.updatedAt) : '-';
        statusToneEl.className = 'statusTone state-offline';
        statusToneEl.textContent = 'offline';
        statusStateEl.textContent = 'offline';
        statusSummaryEl.textContent = `poll failed · ${{String(error.message || error)}}`;
        if (!hasCachedState) {{
          statusOpIdEl.textContent = 'unavailable';
          statusStepEl.textContent = 'waiting for reconnect';
          statusModeEl.textContent = 'no cached state yet';
          statusStartedAtEl.textContent = '-';
          statusUpdatedAtEl.textContent = 'last seen -';
          statusProgressMessageEl.textContent = 'supervisor unreachable on first poll; retrying…';
          statusTargetEl.textContent = 'unavailable while offline';
          statusPreviousEl.textContent = 'unavailable while offline';
          setCopyButtonValue(copyOpIdBtn, '');
          setCopyButtonValue(copyTargetBtn, '');
          setCopyButtonValue(copyPreviousBtn, '');
          return;
        }}
        const cachedTargetText = cached.target ? formatTargetRef(cached.target) : '';
        const cachedPreviousText = hasPreviousRollbackTarget(cached.previous)
          ? formatPreviousRef(cached.previous)
          : '';
        const cachedPreviousCopyText = formatPreviousCopyRef(cached.target, cached.previous);
        statusOpIdEl.textContent = cached.opId ? `${{cached.opId}} · stale` : 'stale';
        statusStepEl.textContent = cached.progress?.step ? `${{cached.progress.step}} · stale` : 'stale';
        statusModeEl.textContent = `last seen ${{lastSeen}} · cached`;
        statusStartedAtEl.textContent = cached.startedAt ? `${{formatTimestamp(cached.startedAt)}} · stale` : '-';
        statusUpdatedAtEl.textContent = `last seen ${{lastSeen}}`;
        statusProgressMessageEl.textContent = cached.progress?.message
          ? `${{cached.progress.message}} · stale while offline`
          : 'offline; waiting for supervisor to respond again';
        statusTargetEl.textContent = cachedTargetText
          ? `${{cachedTargetText}} · stale`
          : 'stale while offline';
        statusPreviousEl.textContent = cachedPreviousText
          ? `${{cachedPreviousText}} · stale`
          : 'stale while offline';
        setCopyButtonValue(copyOpIdBtn, cached.opId || '');
        setCopyButtonValue(copyTargetBtn, cachedTargetText);
        setCopyButtonValue(copyPreviousBtn, cachedPreviousCopyText);
      }}

      function syncWorkspaceMode(hasOperations) {{
        workspaceGridEl.classList.toggle('workspaceGrid-logsOnly', !hasOperations);
        historyRailEl.hidden = !hasOperations;
      }}

      function createStateBadge(state) {{
        const badge = document.createElement('span');
        badge.className = stateBadgeClass(state);

        const dot = document.createElement('span');
        dot.className = stateDotClass(state);
        badge.appendChild(dot);

        const text = document.createElement('span');
        text.textContent = normalizeState(state);
        badge.appendChild(text);
        return badge;
      }}

      function renderOperations(st) {{
        const operations = Array.isArray(st.operations) ? st.operations : [];
        historyListEl.textContent = '';

        if (!operations.length) {{
          syncWorkspaceMode(false);
          activeOpId = null;
          latestOpId = null;
          latestOpMarker = null;
          latestHasNewer = false;
          historyHintEl.textContent = '暂无 operation 历史，已回退到扁平日志视图。';
          logTitleEl.textContent = '当前日志';
          logSummaryEl.textContent = '未发现按 operation 分组的历史记录。';
          renderLogEntries(st.logs || []);
          return;
        }}

        syncWorkspaceMode(true);
        const previousLatest = latestOpId;
        const previousLatestMarker = latestOpMarker;
        const nextLatestOp = operations[0] || null;
        const nextLatest = nextLatestOp?.opId || null;
        const nextLatestMarker = operationMarker(nextLatestOp);
        const wasViewingLatest = !activeOpId || (previousLatest && activeOpId === previousLatest);
        if (nextLatest && wasViewingLatest) {{
          activeOpId = nextLatest;
        }} else if (!operations.some((op) => op.opId === activeOpId)) {{
          activeOpId = nextLatest;
        }}
        if (!wasViewingLatest && previousLatestMarker && nextLatestMarker && previousLatestMarker !== nextLatestMarker) {{
          latestHasNewer = true;
        }}
        latestOpId = nextLatest;
        latestOpMarker = nextLatestMarker;
        if (activeOpId && activeOpId === latestOpId) {{
          latestHasNewer = false;
        }}

        for (let i = 0; i < operations.length; i += 1) {{
          const op = operations[i];
          const button = document.createElement('button');
          button.type = 'button';
          button.className = 'historyCard';
          if (op.opId === activeOpId) {{
            button.classList.add('active');
          }}
          button.onclick = () => {{
            activeOpId = op.opId;
            if (activeOpId === latestOpId) {{
              latestHasNewer = false;
            }}
            renderOperations(st);
          }};

          const top = document.createElement('div');
          top.className = 'historyTop';

          const time = document.createElement('div');
          time.className = 'historyTime';
          time.textContent = formatHistoryTime(op.startedAt);
          top.appendChild(time);

          const badges = document.createElement('div');
          badges.className = 'historyBadges';
          badges.appendChild(createStateBadge(op.state));
          if (i === 0) {{
            const latest = document.createElement('span');
            latest.className = 'newBadge';
            latest.textContent = '最新';
            badges.appendChild(latest);
          }}
          if (i === 0 && latestHasNewer && activeOpId !== op.opId) {{
            const newer = document.createElement('span');
            newer.className = 'newBadge';
            newer.textContent = '新日志';
            badges.appendChild(newer);
          }}
          top.appendChild(badges);
          button.appendChild(top);

          const bottom = document.createElement('div');
          bottom.className = 'historyBottom';

          const meta = document.createElement('div');
          meta.className = 'historyMeta';
          meta.textContent = `${{shortOpId(op.opId)}} · ${{(op.logs || []).length}} lines`;
          bottom.appendChild(meta);

          const tail = document.createElement('div');
          tail.className = 'historyTail';
          tail.textContent = `updated ${{formatHistoryTime(op.updatedAt)}}`;
          bottom.appendChild(tail);
          button.appendChild(bottom);

          historyListEl.appendChild(button);
        }}

        const active = operations.find((op) => op.opId === activeOpId) || operations[0];
        historyHintEl.textContent = `最近 ${{operations.length}} 次 operation · 当前 ${{active.opId}}`;
        logTitleEl.textContent = `${{formatHistoryTime(active.startedAt)}} · ${{shortOpId(active.opId)}}`;
        logSummaryEl.textContent = `${{normalizeState(active.state)}} · updated ${{formatTimestamp(active.updatedAt)}} · ${{(active.logs || []).length}} lines`;
        renderLogEntries(active.logs || []);
      }}

      async function refresh() {{
        try {{
          const st = await fetchJson('self-upgrade');
          lastKnownSelfUpgradeState = st;
          renderStatus(st);
          syncUpgradeActionState(st);
          renderOperations(st);
          syncRollbackState(st);
        }} catch (error) {{
          renderOffline(error);
          if (lastKnownSelfUpgradeState) {{
            syncUpgradeActionState(lastKnownSelfUpgradeState);
            renderOperations(lastKnownSelfUpgradeState);
            syncRollbackState(lastKnownSelfUpgradeState);
          }} else {{
            syncWorkspaceMode(false);
            historyListEl.textContent = '';
            historyHintEl.textContent = 'Supervisor 暂时离线，尚未拿到可展示的 operation 历史。';
            logTitleEl.textContent = '等待 supervisor 响应';
            logSummaryEl.textContent = '首次轮询失败；暂无可复用的缓存日志。';
            setLogsText('等待日志…');
            syncRollbackState({{}});
          }}
          setRollbackPopOpen(false);
        }}
      }}

      window.addEventListener('storage', (evt) => {{
        if (evt.key && evt.key !== themeController?.storageKey) return;
        syncTheme();
      }});
      if (typeof themeMedia.addEventListener === 'function') {{
        themeMedia.addEventListener('change', handleSystemThemeChange);
      }} else if (typeof themeMedia.addListener === 'function') {{
        themeMedia.addListener(handleSystemThemeChange);
      }}

      document.getElementById('refresh').onclick = () => refresh();
      dryBtn.onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'dry-run', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      applyBtn.onclick = async () => {{
        const tag = document.getElementById('tag').value || 'latest';
        await fetchJson('self-upgrade', {{ method: 'POST', body: JSON.stringify({{ target: {{ tag }}, mode: 'apply', rollbackOnFailure: true }}) }});
        await refresh();
      }};
      document.getElementById('rollback').onclick = async (evt) => {{
        evt.preventDefault();
        if (rollbackBtn.disabled) return;
        const st = await fetchJson('self-upgrade');
        syncRollbackState(st);
        if (!canRollback(st)) {{
          await refresh();
          return;
        }}
        rollbackPendingOpId = st.opId || null;
        rollbackOpId.textContent = rollbackPendingOpId || '-';
        setRollbackPopOpen(true);
      }};
      rollbackCancelBtn.onclick = () => {{
        setRollbackPopOpen(false);
      }};
      document.getElementById('rollbackConfirm').onclick = async () => {{
        if (!rollbackPendingOpId) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        const st = await fetchJson('self-upgrade');
        syncRollbackState(st);
        if (!canRollback(st)) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        if (!st.opId || st.opId !== rollbackPendingOpId) {{
          setRollbackPopOpen(false);
          await refresh();
          return;
        }}
        rollbackConfirmBtn.disabled = true;
        rollbackCancelBtn.disabled = true;
        try {{
          await fetchJson('self-upgrade/rollback', {{ method: 'POST', body: JSON.stringify({{ opId: rollbackPendingOpId }}) }});
          setRollbackPopOpen(false);
          await refresh();
        }} finally {{
          rollbackConfirmBtn.disabled = false;
          rollbackCancelBtn.disabled = false;
        }}
      }};
      document.addEventListener('click', (evt) => {{
        if (!rollbackPopOpen) return;
        const target = evt.target;
        if (!rollbackWrap.contains(target)) setRollbackPopOpen(false);
      }});
      document.addEventListener('keydown', (evt) => {{
        if (evt.key === 'Escape' && rollbackPopOpen) {{
          evt.preventDefault();
          setRollbackPopOpen(false);
          rollbackBtn.focus();
        }}
      }});

      refresh();
      setInterval(refresh, 1500);
    </script>
  </body>
</html>"#,
        base_path = base_path,
        version_html = version_html,
        repository_html = repository_html,
        developer_html = developer_html,
        base_path_json =
            serde_json::to_string(base_path).unwrap_or_else(|_| "\"/supervisor\"".to_string()),
        theme_storage_key_json = serde_json::to_string(THEME_STORAGE_KEY)
            .unwrap_or_else(|_| "\"dockrev:theme\"".to_string()),
        style_css = STYLE_CSS
    )
}
