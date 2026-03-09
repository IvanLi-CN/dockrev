use super::meta::{SupervisorMeta, trimmed_non_empty};

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
      :root {{
        color-scheme: dark;
        --bg: #061227;
        --bg-layered:
          radial-gradient(960px 520px at -12% -20%, rgba(55, 133, 226, 0.18) 0%, rgba(55, 133, 226, 0) 55%),
          radial-gradient(880px 460px at 115% -12%, rgba(48, 161, 211, 0.14) 0%, rgba(48, 161, 211, 0) 56%),
          linear-gradient(160deg, #040b1a 0%, #061227 48%, #040f21 100%);
        --text: rgba(232, 241, 255, 0.96);
        --muted: rgba(220, 234, 254, 0.78);
        --panel: rgba(13, 35, 66, 0.92);
        --panel-border: rgba(156, 192, 232, 0.24);
        --panel-shadow: 0 24px 68px rgba(1, 10, 24, 0.54);
        --button-bg: rgba(255, 255, 255, 0.05);
        --button-hover: rgba(54, 191, 250, 0.16);
        --button-border: rgba(188, 223, 255, 0.22);
        --button-text: var(--text);
        --input-bg: rgba(255, 255, 255, 0.05);
        --input-border: rgba(188, 223, 255, 0.2);
        --input-shadow: 0 0 0 1px rgba(255, 255, 255, 0.01);
        --pre-bg: rgba(4, 11, 26, 0.72);
        --tab-bg: rgba(255, 255, 255, 0.06);
        --tab-active-bg: rgba(54, 191, 250, 0.16);
        --tab-active-border: rgba(54, 191, 250, 0.42);
        --pop-bg: rgba(7, 20, 42, 0.98);
        --link: #7dd3fc;
        --link-hover: #bae6fd;
        --code-bg: rgba(255, 255, 255, 0.08);
        --spinner-track: rgba(232, 241, 255, 0.24);
        --spinner-head: rgba(232, 241, 255, 0.84);
        --ok: #22c55e;
        --bad: #f87171;
        --new-badge-fg: #fbbf24;
        --new-badge-border: rgba(251, 191, 36, 0.28);
        --new-badge-bg: rgba(251, 191, 36, 0.16);
        --selection: rgba(54, 191, 250, 0.22);
      }}
      html[data-theme='light'] {{
        color-scheme: light;
        --bg: #f6faff;
        --bg-layered:
          radial-gradient(980px 500px at -14% -24%, rgba(76, 162, 238, 0.03) 0%, rgba(76, 162, 238, 0) 58%),
          radial-gradient(900px 460px at 112% -14%, rgba(57, 156, 228, 0.025) 0%, rgba(57, 156, 228, 0) 56%),
          linear-gradient(160deg, #ffffff 0%, #f8fcff 46%, #f3f9ff 100%);
        --text: rgba(16, 40, 66, 0.96);
        --muted: rgba(58, 86, 118, 0.88);
        --panel: rgba(255, 255, 255, 0.94);
        --panel-border: rgba(14, 56, 100, 0.14);
        --panel-shadow: 0 22px 50px rgba(17, 62, 112, 0.14);
        --button-bg: rgba(255, 255, 255, 0.92);
        --button-hover: rgba(12, 121, 207, 0.1);
        --button-border: rgba(14, 56, 100, 0.16);
        --button-text: rgba(16, 40, 66, 0.96);
        --input-bg: rgba(255, 255, 255, 0.9);
        --input-border: rgba(14, 56, 100, 0.16);
        --input-shadow: 0 0 0 1px rgba(12, 121, 207, 0.03);
        --pre-bg: rgba(12, 121, 207, 0.06);
        --tab-bg: rgba(12, 121, 207, 0.05);
        --tab-active-bg: rgba(12, 121, 207, 0.12);
        --tab-active-border: rgba(12, 121, 207, 0.34);
        --pop-bg: rgba(255, 255, 255, 0.98);
        --link: #0c79cf;
        --link-hover: #095ea0;
        --code-bg: rgba(12, 121, 207, 0.08);
        --spinner-track: rgba(16, 40, 66, 0.16);
        --spinner-head: rgba(16, 40, 66, 0.72);
        --ok: #138347;
        --bad: #bf1240;
        --new-badge-fg: #a55706;
        --new-badge-border: rgba(180, 83, 9, 0.28);
        --new-badge-bg: rgba(251, 191, 36, 0.18);
        --selection: rgba(12, 121, 207, 0.18);
      }}
      * {{ box-sizing: border-box; }}
      html {{ background: var(--bg); }}
      body {{
        font-family: 'Avenir Next', 'Avenir', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif;
        padding: 18px 18px 32px;
        max-width: 960px;
        min-height: 100vh;
        margin: 0 auto;
        background: var(--bg-layered);
        background-attachment: fixed;
        color: var(--text);
      }}
      ::selection {{ background: var(--selection); }}
      a {{ color: var(--link); text-decoration: none; }}
      a:hover {{ color: var(--link-hover); text-decoration: underline; }}
      code {{ background: var(--code-bg); padding: 1px 6px; border-radius: 999px; }}
      .row {{ display: flex; gap: 10px; align-items: center; flex-wrap: wrap; }}
      .card {{
        border: 1px solid var(--panel-border);
        border-radius: 12px;
        padding: 14px;
        margin-top: 12px;
        background: var(--panel);
        box-shadow: var(--panel-shadow);
        backdrop-filter: blur(14px);
      }}
      .muted {{ color: var(--muted); font-size: 12px; }}
      button {{
        padding: 8px 12px;
        border-radius: 10px;
        border: 1px solid var(--button-border);
        background: var(--button-bg);
        color: var(--button-text);
        cursor: pointer;
        transition: background-color 140ms ease, border-color 140ms ease, transform 140ms ease;
      }}
      button:hover:not([disabled]) {{ background: var(--button-hover); border-color: var(--tab-active-border); }}
      button:focus-visible, input:focus-visible {{ outline: 2px solid var(--tab-active-border); outline-offset: 2px; }}
      button[disabled] {{ opacity: 0.5; cursor: not-allowed; }}
      button.btnRunning {{ display: inline-flex; align-items: center; gap: 8px; }}
      button.btnRunning::before {{ content: ''; width: 12px; height: 12px; border-radius: 999px; border: 2px solid var(--spinner-track); border-top-color: var(--spinner-head); animation: supervisorButtonSpin 0.8s linear infinite; }}
      input {{
        padding: 8px 10px;
        border-radius: 10px;
        border: 1px solid var(--input-border);
        background: var(--input-bg);
        color: var(--text);
        box-shadow: var(--input-shadow);
      }}
      pre {{
        background: var(--pre-bg);
        padding: 10px;
        border-radius: 10px;
        overflow: auto;
        color: var(--text);
        line-height: 1.55;
      }}
      .tabsPanel {{ margin-top: 10px; }}
      .opTabs {{ display: flex; flex-wrap: wrap; gap: 8px; margin-top: 8px; max-height: 40px; overflow: hidden; }}
      .opTabs.expanded {{ max-height: none; overflow: visible; }}
      .opTab {{ display: inline-flex; align-items: center; gap: 8px; padding: 6px 10px; border-radius: 10px; border: 1px solid var(--button-border); background: var(--tab-bg); font-size: 12px; color: var(--text); }}
      .opTab.active {{ border-color: var(--tab-active-border); background: var(--tab-active-bg); }}
      .opDot {{ width: 8px; height: 8px; border-radius: 999px; flex: 0 0 auto; }}
      .opDot-running {{ background: #2563eb; }}
      .opDot-succeeded {{ background: #16a34a; }}
      .opDot-failed {{ background: #dc2626; }}
      .opDot-rolled_back {{ background: #dc2626; }}
      .opDot-unknown {{ background: #6b7280; }}
      .newBadge {{ color: var(--new-badge-fg); border: 1px solid var(--new-badge-border); background: var(--new-badge-bg); border-radius: 999px; padding: 1px 6px; font-size: 11px; }}
      #tabsToggle {{ padding: 4px 10px; font-size: 12px; }}
      .popWrap {{ position: relative; display: inline-flex; align-items: center; }}
      .popCard {{ position: absolute; top: calc(100% + 8px); left: 0; width: min(320px, calc(100vw - 36px)); border: 1px solid var(--button-border); border-radius: 12px; background: var(--pop-bg); box-shadow: var(--panel-shadow); padding: 10px; z-index: 20; }}
      .popTitle {{ margin: 0 0 6px; font-size: 13px; font-weight: 700; }}
      .popActions {{ display: flex; gap: 8px; justify-content: flex-end; margin-top: 10px; }}
      .danger {{ border-color: #dc2626; background: #dc2626; color: #fff; }}
      .ok {{ color: var(--ok); }}
      .bad {{ color: var(--bad); }}
      .metaLine {{ margin-top: 6px; display: flex; flex-wrap: wrap; gap: 8px 16px; }}
      .metaItem {{ color: var(--muted); font-size: 12px; }}
      .metaItem code {{ font-size: 12px; }}
      @keyframes supervisorButtonSpin {{ from {{ transform: rotate(0deg); }} to {{ transform: rotate(360deg); }} }}
      @media (prefers-reduced-motion: reduce) {{
        button.btnRunning::before {{ animation: none; }}
      }}
    </style>
  </head>
  <body>
    <div class="row" style="gap:12px;">
      <img src="{base_path}/favicon.png" alt="" aria-hidden="true" width="24" height="24" style="display:block" />
      <h1 style="margin:0;">Dockrev 自我升级（Supervisor）</h1>
    </div>
    <div class="muted">该页面独立于 Dockrev 生命周期；Dockrev 重启期间仍可用。</div>
    <div class="metaLine">
      <div class="metaItem">Supervisor 版本：{version_html}</div>
      <div class="metaItem">开源仓库：{repository_html}</div>
      <div class="metaItem">开发者：{developer_html}</div>
    </div>

    <div class="card">
      <div class="row">
        <div>Target tag:</div>
        <input id="tag" value="latest" />
        <button id="dry">预览（dry-run）</button>
        <button id="apply">开始升级（apply）</button>
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
        <a href="/" style="margin-left:auto">返回 Dockrev</a>
      </div>
      <div class="muted">提示：失败将尝试回滚到 previous digest（如可用）。</div>
    </div>

    <div class="card">
      <div id="status" class="muted">loading…</div>
      <div class="tabsPanel">
        <div class="row" style="justify-content: space-between; gap: 8px;">
          <div id="tabHint" class="muted">loading…</div>
          <button id="tabsToggle" hidden>展开</button>
        </div>
        <div id="opTabs" class="opTabs"></div>
      </div>
      <pre id="logs"></pre>
    </div>

    <script>
      const base = {base_path_json};
      const themeController = window.__dockrevSupervisorTheme;
      const themeMedia = themeController?.mediaQuery || window.matchMedia('(prefers-color-scheme: dark)');
      let activeOpId = null;
      let latestOpId = null;
      let tabsExpanded = false;
      let tabsCanExpand = false;
      let latestHasNewer = false;
      let lastKnownSelfUpgradeState = null;
      const toUrl = (p) => base.replace(/\/$/, '') + '/' + p.replace(/^\//, '');

      function syncTheme() {{
        if (themeController?.syncThemeFromPreference) {{
          themeController.syncThemeFromPreference();
        }}
      }}

      function handleSystemThemeChange() {{
        if (themeController?.hasStoredTheme && themeController.hasStoredTheme()) return;
        syncTheme();
      }}

	      async function fetchJson(path, init) {{
	        const resp = await fetch(toUrl(path), {{ ...init, headers: {{ 'Content-Type': 'application/json' }} }});
	        const text = await resp.text();
	        if (!resp.ok) throw new Error(`HTTP ${{resp.status}}: ${{text}}`);
	        return text ? JSON.parse(text) : null;
	      }}

	      const rollbackWrap = document.getElementById('rollbackWrap');
	      const dryBtn = document.getElementById('dry');
	      const applyBtn = document.getElementById('apply');
	      const rollbackBtn = document.getElementById('rollback');
	      const rollbackPop = document.getElementById('rollbackPop');
	      const rollbackOpId = document.getElementById('rollbackOpId');
	      const rollbackCancelBtn = document.getElementById('rollbackCancel');
	      const rollbackConfirmBtn = document.getElementById('rollbackConfirm');
	      let rollbackPopOpen = false;
	      let rollbackPendingOpId = null;

	      function canRollback(st) {{
	        return !!st.opId && (st.state === 'failed' || st.state === 'rolled_back' || st.state === 'succeeded');
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
      function statusClass(st) {{
        const s = st && st.state;
        return s === 'succeeded' ? 'ok' : (s === 'failed' || s === 'rolled_back') ? 'bad' : '';
      }}

      function renderStatusText(st) {{
        const target = `${{st.target?.image}}:${{st.target?.tag}}${{st.target?.digest ? '@'+st.target.digest : ''}}`;
        const prev = `${{st.previous?.tag}}${{st.previous?.digest ? '@'+st.previous.digest : ''}}`;
        return `${{st.state}} · opId=${{st.opId||'-'}} · step=${{st.progress?.step}} · target=${{target}} · previous=${{prev}}`;
      }}

      function formatLogs(logs) {{
        return (logs || []).map(l => `[${{l.ts}}] ${{l.level}} ${{l.msg}}`).join('\n');
      }}

      function pad2(v) {{
        return String(v).padStart(2, '0');
      }}

      function formatTabTime(ts) {{
        const d = new Date(ts || '');
        if (Number.isNaN(d.getTime())) return '-- --:--';
        return `${{pad2(d.getMonth() + 1)}}-${{pad2(d.getDate())}} ${{pad2(d.getHours())}}:${{pad2(d.getMinutes())}}`;
      }}

      function formatTabLabel(opId, startedAt) {{
        const suffix = String(opId || '-').slice(-6);
        return `${{formatTabTime(startedAt)}} · ${{suffix}}`;
      }}

      function measureTabsOverflow(tabsEl) {{
        const wasExpanded = tabsEl.classList.contains('expanded');
        if (wasExpanded) tabsEl.classList.remove('expanded');
        const overflow = tabsEl.scrollHeight > tabsEl.clientHeight + 1;
        if (wasExpanded) tabsEl.classList.add('expanded');
        return overflow;
      }}

      function syncTabsToggle() {{
        const tabsEl = document.getElementById('opTabs');
        const toggleEl = document.getElementById('tabsToggle');
        if (!tabsEl || !toggleEl) return;
        tabsEl.classList.toggle('expanded', tabsExpanded);
        tabsCanExpand = measureTabsOverflow(tabsEl);
        if (!tabsCanExpand) {{
          tabsExpanded = false;
          tabsEl.classList.remove('expanded');
        }}
        toggleEl.hidden = !tabsCanExpand;
        toggleEl.textContent = tabsExpanded ? '收起' : '展开';
      }}

      function renderOperations(st) {{
        const operations = Array.isArray(st.operations) ? st.operations : [];
        const tabsEl = document.getElementById('opTabs');
        const hintEl = document.getElementById('tabHint');
        const logsEl = document.getElementById('logs');
        tabsEl.textContent = '';

        if (!operations.length) {{
          activeOpId = null;
          latestOpId = null;
          latestHasNewer = false;
          hintEl.textContent = '暂无分组日志';
          logsEl.textContent = formatLogs(st.logs || []);
          requestAnimationFrame(syncTabsToggle);
          return;
        }}

        const previousLatest = latestOpId;
        const nextLatest = operations[0]?.opId || null;
        const wasViewingLatest = !activeOpId || (previousLatest && activeOpId === previousLatest);
        if (nextLatest && wasViewingLatest) {{
          activeOpId = nextLatest;
        }} else if (!operations.some((op) => op.opId === activeOpId)) {{
          activeOpId = nextLatest;
        }}
        if (!wasViewingLatest && previousLatest && nextLatest && previousLatest !== nextLatest) {{
          latestHasNewer = true;
        }}
        latestOpId = nextLatest;
        if (activeOpId && activeOpId === latestOpId) {{
          latestHasNewer = false;
        }}

        for (let i = 0; i < operations.length; i += 1) {{
          const op = operations[i];
          const btn = document.createElement('button');
          btn.type = 'button';
          btn.className = 'opTab';
          if (op.opId === activeOpId) {{
            btn.classList.add('active');
          }}
          btn.onclick = () => {{
            activeOpId = op.opId;
            if (activeOpId === latestOpId) {{
              latestHasNewer = false;
            }}
            renderOperations(st);
          }};

          const dot = document.createElement('span');
          dot.className = `opDot opDot-${{op.state || 'unknown'}}`;
          btn.appendChild(dot);

          const text = document.createElement('span');
          text.textContent = formatTabLabel(op.opId, op.startedAt);
          btn.appendChild(text);

          if (i === 0 && latestHasNewer && activeOpId !== op.opId) {{
            const badge = document.createElement('span');
            badge.className = 'newBadge';
            badge.textContent = '新';
            btn.appendChild(badge);
          }}

          tabsEl.appendChild(btn);
        }}

        const active = operations.find((op) => op.opId === activeOpId) || operations[0];
        logsEl.textContent = formatLogs(active.logs || []);
        hintEl.textContent = `operations: ${{operations.length}}（当前 ${{active.opId}}）`;
        requestAnimationFrame(syncTabsToggle);
      }}

      async function refresh() {{
        const statusEl = document.getElementById('status');
        try {{
          const st = await fetchJson('self-upgrade');
          lastKnownSelfUpgradeState = st;
          statusEl.className = `muted ${{statusClass(st)}}`.trim();
          statusEl.textContent = renderStatusText(st);
          syncUpgradeActionState(st);
          renderOperations(st);
          syncRollbackState(st);
        }} catch (e) {{
          statusEl.className = 'muted bad';
          statusEl.textContent = `offline ${{String(e.message||e)}}`;
          if (lastKnownSelfUpgradeState) syncUpgradeActionState(lastKnownSelfUpgradeState);
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
      document.getElementById('tabsToggle').onclick = () => {{
        if (!tabsCanExpand) return;
        tabsExpanded = !tabsExpanded;
        syncTabsToggle();
      }};
      window.addEventListener('resize', () => {{
        requestAnimationFrame(syncTabsToggle);
      }});
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
            .unwrap_or_else(|_| "\"dockrev:theme\"".to_string())
    )
}
