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
        --bg: #081019;
        --bg-layered:
          linear-gradient(rgba(255, 255, 255, 0.018) 1px, transparent 1px),
          linear-gradient(90deg, rgba(255, 255, 255, 0.018) 1px, transparent 1px),
          radial-gradient(66% 80% at 0% 0%, rgba(64, 180, 255, 0.14) 0%, rgba(64, 180, 255, 0) 58%),
          radial-gradient(54% 70% at 100% 0%, rgba(255, 176, 87, 0.11) 0%, rgba(255, 176, 87, 0) 48%),
          linear-gradient(180deg, #071018 0%, #0a141d 45%, #0d1823 100%);
        --text: rgba(236, 242, 248, 0.97);
        --muted: rgba(187, 198, 210, 0.78);
        --panel: rgba(10, 18, 26, 0.86);
        --panel-strong: rgba(10, 17, 24, 0.96);
        --panel-border: rgba(139, 157, 177, 0.2);
        --panel-shadow: 0 16px 40px rgba(0, 0, 0, 0.28);
        --surface: rgba(255, 255, 255, 0.035);
        --surface-strong: rgba(255, 255, 255, 0.05);
        --button-bg: rgba(255, 255, 255, 0.04);
        --button-hover: rgba(64, 180, 255, 0.11);
        --button-border: rgba(160, 178, 198, 0.18);
        --button-text: var(--text);
        --input-bg: rgba(255, 255, 255, 0.04);
        --input-border: rgba(160, 178, 198, 0.18);
        --input-shadow: inset 0 0 0 1px rgba(255, 255, 255, 0.012);
        --console-bg: rgba(5, 10, 16, 0.94);
        --pop-bg: rgba(9, 15, 22, 0.98);
        --code-bg: rgba(255, 255, 255, 0.08);
        --link: #91dfff;
        --link-hover: #c8f1ff;
        --accent: #40b4ff;
        --accent-strong: #1687d9;
        --accent-warm: #ffb057;
        --spinner-track: rgba(236, 242, 248, 0.2);
        --spinner-head: rgba(236, 242, 248, 0.88);
        --ok: #32c96c;
        --bad: #ff7b7b;
        --warn: #ffb155;
        --info: #53a9ff;
        --selection: rgba(64, 180, 255, 0.18);
      }}
      html[data-theme='light'] {{
        color-scheme: light;
        --bg: #f4f7fb;
        --bg-layered:
          linear-gradient(rgba(10, 24, 40, 0.03) 1px, transparent 1px),
          linear-gradient(90deg, rgba(10, 24, 40, 0.03) 1px, transparent 1px),
          radial-gradient(66% 80% at 0% 0%, rgba(64, 180, 255, 0.1) 0%, rgba(64, 180, 255, 0) 58%),
          radial-gradient(54% 70% at 100% 0%, rgba(255, 176, 87, 0.09) 0%, rgba(255, 176, 87, 0) 48%),
          linear-gradient(180deg, #fbfdff 0%, #f4f8fb 45%, #edf3f8 100%);
        --text: rgba(14, 24, 37, 0.96);
        --muted: rgba(83, 98, 116, 0.82);
        --panel: rgba(255, 255, 255, 0.9);
        --panel-strong: rgba(255, 255, 255, 0.98);
        --panel-border: rgba(12, 30, 52, 0.13);
        --panel-shadow: 0 16px 38px rgba(15, 23, 42, 0.08);
        --surface: rgba(12, 30, 52, 0.035);
        --surface-strong: rgba(12, 30, 52, 0.055);
        --button-bg: rgba(255, 255, 255, 0.86);
        --button-hover: rgba(22, 135, 217, 0.08);
        --button-border: rgba(12, 30, 52, 0.13);
        --button-text: rgba(14, 24, 37, 0.96);
        --input-bg: rgba(255, 255, 255, 0.92);
        --input-border: rgba(12, 30, 52, 0.13);
        --input-shadow: inset 0 0 0 1px rgba(22, 135, 217, 0.02);
        --console-bg: rgba(242, 247, 252, 0.98);
        --pop-bg: rgba(255, 255, 255, 0.98);
        --code-bg: rgba(12, 30, 52, 0.07);
        --link: #0f87bb;
        --link-hover: #0b668f;
        --accent: #1687d9;
        --accent-strong: #0f6aa9;
        --accent-warm: #cc7a29;
        --spinner-track: rgba(14, 24, 37, 0.18);
        --spinner-head: rgba(14, 24, 37, 0.8);
        --ok: #15803d;
        --bad: #c24141;
        --warn: #a16207;
        --info: #2563eb;
        --selection: rgba(22, 135, 217, 0.15);
      }}
      * {{ box-sizing: border-box; }}
      html {{ background: var(--bg); }}
      body {{
        font-family: 'IBM Plex Sans', 'Avenir Next', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', sans-serif;
        min-height: 100vh;
        margin: 0 auto;
        padding: 0 18px 36px;
        max-width: 1280px;
        background: var(--bg-layered);
        background-size: 24px 24px, 24px 24px, auto, auto, auto;
        background-attachment: fixed;
        color: var(--text);
      }}
      ::selection {{ background: var(--selection); }}
      a {{ color: var(--link); text-decoration: none; }}
      a:hover {{ color: var(--link-hover); text-decoration: underline; }}
      code {{
        background: var(--code-bg);
        padding: 2px 7px;
        border-radius: 999px;
        word-break: break-all;
      }}
      button,
      input,
      textarea,
      select {{ font: inherit; }}
      button {{
        min-height: 38px;
        padding: 8px 12px;
        border-radius: 12px;
        border: 1px solid var(--button-border);
        background: var(--button-bg);
        color: var(--button-text);
        cursor: pointer;
        transition: background-color 160ms ease, border-color 160ms ease, box-shadow 160ms ease, opacity 160ms ease;
      }}
      button:hover:not([disabled]) {{
        background: var(--button-hover);
        border-color: rgba(64, 180, 255, 0.34);
        box-shadow: inset 0 0 0 1px rgba(64, 180, 255, 0.08);
      }}
      button:focus-visible,
      input:focus-visible {{
        outline: 2px solid rgba(64, 180, 255, 0.38);
        outline-offset: 2px;
      }}
      button[disabled] {{
        opacity: 0.52;
        cursor: not-allowed;
        box-shadow: none;
      }}
      button.primary {{
        background: linear-gradient(135deg, rgba(64, 180, 255, 0.2), rgba(255, 176, 87, 0.16));
        border-color: rgba(64, 180, 255, 0.4);
      }}
      html[data-theme='light'] button.primary {{
        background: linear-gradient(135deg, rgba(22, 135, 217, 0.14), rgba(204, 122, 41, 0.12));
        border-color: rgba(15, 106, 169, 0.22);
      }}
      button.btnRunning {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 7px;
      }}
      button.btnRunning::before {{
        content: '';
        width: 11px;
        height: 11px;
        border-radius: 999px;
        border: 2px solid var(--spinner-track);
        border-top-color: var(--spinner-head);
        animation: supervisorButtonSpin 0.8s linear infinite;
      }}
      input {{
        width: 100%;
        min-width: 0;
        padding: 9px 12px;
        border-radius: 12px;
        border: 1px solid var(--input-border);
        background: var(--input-bg);
        color: var(--text);
        box-shadow: var(--input-shadow);
      }}
      pre {{
        margin: 0;
        padding: 12px 14px;
        border-radius: 16px;
        overflow: auto;
        white-space: pre-wrap;
        word-break: break-word;
        color: var(--text);
        font-size: 13px;
        line-height: 1.52;
        font-family: 'IBM Plex Mono', 'JetBrains Mono', 'SFMono-Regular', 'Menlo', monospace;
      }}
      .shell {{
        display: grid;
        gap: 14px;
        padding-top: 18px;
      }}
      .panel {{
        border: 1px solid var(--panel-border);
        border-radius: 20px;
        padding: 16px;
        background: var(--panel);
        box-shadow: var(--panel-shadow);
      }}
      .muted {{
        color: var(--muted);
        font-size: 12px;
        line-height: 1.48;
      }}
      .eyebrow,
      .sectionEyebrow {{
        color: var(--accent);
        font-size: 11px;
        font-weight: 600;
        text-transform: uppercase;
        letter-spacing: 0.14em;
      }}
      .sectionHeadingRow {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        flex-wrap: wrap;
        margin-bottom: 12px;
      }}
      .sectionHeadingRow h2 {{
        margin: 5px 0 0;
        font-size: 18px;
        line-height: 1.08;
        letter-spacing: -0.02em;
      }}
      .sectionHeadingMeta {{
        text-align: right;
      }}
      .sectionLead {{
        display: grid;
        gap: 4px;
        margin-bottom: 12px;
      }}
      .sectionLead h2 {{
        margin: 0;
        font-size: 18px;
        line-height: 1.08;
        letter-spacing: -0.02em;
      }}
      .sectionNote {{
        max-width: 76ch;
      }}
      .masthead {{
        padding-top: 10px;
        padding-bottom: 10px;
      }}
      .mastheadRow {{
        display: grid;
        grid-template-columns: minmax(0, 1fr) auto;
        gap: 12px;
        align-items: center;
      }}
      .titleBlock {{
        display: grid;
        gap: 6px;
      }}
      .brandRow {{
        display: flex;
        gap: 10px;
        align-items: center;
      }}
      .brandMark {{
        width: 36px;
        height: 36px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 10px;
        background: linear-gradient(145deg, rgba(64, 180, 255, 0.16), rgba(255, 176, 87, 0.08));
        border: 1px solid rgba(64, 180, 255, 0.2);
        flex: 0 0 auto;
      }}
      .brandMark img {{
        display: block;
        width: 20px;
        height: 20px;
      }}
      .masthead h1 {{
        margin: 0;
        font-size: clamp(24px, 2.2vw, 32px);
        line-height: 1;
        letter-spacing: -0.04em;
      }}
      .intro {{
        margin: 0;
        max-width: 62ch;
        color: var(--muted);
        font-size: 12px;
        line-height: 1.45;
      }}
      .metaFooter {{
        display: grid;
        grid-template-columns: 180px minmax(0, 1fr) 160px;
        gap: 18px;
        align-items: start;
        margin-top: 6px;
        padding: 16px 4px 0;
        border-top: 1px solid var(--panel-border);
      }}
      .metaPill {{
        display: grid;
        gap: 4px;
        min-width: 0;
        padding: 0;
        border: 0;
        border-radius: 0;
        background: none;
      }}
      .metaPill + .metaPill {{
        padding-left: 18px;
        border-left: 1px solid var(--panel-border);
      }}
      .metaLabel {{
        color: var(--muted);
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.14em;
      }}
      .metaValue {{
        font-size: 12px;
        line-height: 1.42;
        word-break: break-word;
      }}
      .linkButton {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 38px;
        padding: 8px 12px;
        border-radius: 12px;
        border: 1px solid var(--button-border);
        background: var(--surface);
        color: var(--link);
      }}
      .linkButton:hover {{
        background: var(--button-hover);
        text-decoration: none;
      }}
      .actionDeckGrid {{
        display: grid;
        grid-template-columns: minmax(220px, 250px) minmax(0, 1fr) auto;
        grid-template-areas:
          'field primary aux'
          'hint note note';
        column-gap: 12px;
        row-gap: 10px;
        align-items: end;
      }}
      .actionDeckGrid > * {{ min-width: 0; }}
      .fieldBlock {{
        grid-area: field;
        display: grid;
        gap: 7px;
        align-self: end;
      }}
      .fieldLabel {{
        font-size: 11px;
        color: var(--muted);
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .fieldHint {{
        font-size: 11px;
        color: var(--muted);
      }}
      .actionFieldHint {{
        grid-area: hint;
        margin-top: -2px;
      }}
      .actionControls {{
        display: grid;
        gap: 10px;
      }}
      .buttonGroup {{
        display: flex;
        flex-wrap: wrap;
        gap: 10px;
        align-items: center;
      }}
      .buttonGroup > * {{ flex: 0 0 auto; }}
      .buttonGroup-main {{
        grid-area: primary;
        align-self: end;
      }}
      .buttonGroup-aux {{
        grid-area: aux;
        justify-content: flex-end;
        justify-self: end;
        align-self: end;
      }}
      .actionCallout {{
        grid-area: note;
        min-height: 40px;
        display: flex;
        align-items: center;
        padding: 10px 12px;
        border-radius: 14px;
        border: 1px dashed var(--panel-border);
        background: var(--surface);
      }}
      .workspaceGrid {{
        display: grid;
        grid-template-columns: minmax(0, 1fr) 320px;
        grid-template-areas:
          'logs sidebar'
          'detail sidebar';
        gap: 14px;
        align-items: start;
      }}
      .workspaceGrid.workspaceGrid-logsOnly {{
        grid-template-columns: minmax(0, 1fr);
        grid-template-areas:
          'logs'
          'sidebar'
          'detail';
      }}
      .opsSidebar {{
        grid-area: sidebar;
        display: grid;
        gap: 12px;
        align-content: start;
      }}
      .historyRail,
      .logPanel,
      .statusSidebar {{
        border-radius: 18px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
      }}
      .logPanel {{
        grid-area: logs;
        min-height: 420px;
        display: grid;
        grid-template-rows: auto 1fr;
        padding: 14px;
      }}
      .logHeader {{
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 12px;
        flex-wrap: wrap;
        margin-bottom: 10px;
      }}
      .logTitle {{
        margin-top: 2px;
        font-size: 16px;
        line-height: 1.08;
        font-weight: 600;
      }}
      .logSummary {{
        max-width: 300px;
        text-align: right;
        font-size: 11px;
        line-height: 1.42;
      }}
      #logs {{
        height: 100%;
        min-height: 320px;
        padding: 10px 12px;
        font-size: 13px;
        line-height: 1.48;
        background: var(--console-bg);
        border: 1px solid var(--panel-border);
      }}
      .logLine {{
        display: block;
        margin: 0 -4px;
        padding: 3px 8px;
        border-radius: 10px;
      }}
      .logLine + .logLine {{
        margin-top: 2px;
      }}
      .logLine-warn {{
        background: rgba(255, 177, 85, 0.08);
      }}
      .logLine-error,
      .logLine-fatal {{
        background: rgba(255, 123, 123, 0.1);
      }}
      html[data-theme='light'] .logLine-warn {{
        background: rgba(161, 98, 7, 0.09);
      }}
      html[data-theme='light'] .logLine-error,
      html[data-theme='light'] .logLine-fatal {{
        background: rgba(194, 65, 65, 0.1);
      }}
      .logToken-ts {{
        color: var(--muted);
      }}
      .logToken-level {{
        font-weight: 700;
        letter-spacing: 0.04em;
      }}
      .logLevel-trace,
      .logLevel-debug {{
        color: var(--muted);
      }}
      .logLevel-info {{
        color: var(--info);
      }}
      .logLevel-warn {{
        color: var(--warn);
      }}
      .logLevel-error,
      .logLevel-fatal {{
        color: var(--bad);
      }}
      .logToken-msg {{
        color: var(--text);
      }}
      .logToken-ref {{
        color: var(--link);
      }}
      .logToken-opid {{
        color: var(--accent);
      }}
      .logToken-digest {{
        color: var(--accent-warm);
      }}
      .statusSidebar {{
        padding: 14px;
        background: linear-gradient(180deg, rgba(64, 180, 255, 0.08), rgba(255, 255, 255, 0.015));
      }}
      html[data-theme='light'] .statusSidebar {{
        background: linear-gradient(180deg, rgba(22, 135, 217, 0.06), rgba(255, 255, 255, 0.6));
      }}
      .statusSidebarHeader {{
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 10px;
        margin-bottom: 12px;
      }}
      .statusSidebarTitle {{
        display: grid;
        gap: 6px;
      }}
      .snapshotGrid {{
        display: grid;
        gap: 10px;
      }}
      .snapshotItem {{
        padding: 10px 12px;
        border-radius: 14px;
        border: 1px solid var(--panel-border);
        background: var(--panel-strong);
        display: grid;
        gap: 6px;
      }}
      .historyRail {{
        min-height: 248px;
        padding: 12px;
        max-height: min(560px, calc(100vh - 190px));
        overflow: auto;
        overscroll-behavior: contain;
      }}
      .historyRailHeader {{
        display: grid;
        gap: 4px;
        padding-bottom: 10px;
        margin-bottom: 10px;
        border-bottom: 1px solid var(--panel-border);
      }}
      .historyHint {{
        font-size: 11px;
        line-height: 1.45;
      }}
      .historyList {{
        display: grid;
        gap: 10px;
      }}
      .historyCard {{
        width: 100%;
        min-height: 78px;
        padding: 12px;
        border-radius: 16px;
        border: 1px solid var(--panel-border);
        background: var(--panel-strong);
        color: var(--text);
        text-align: left;
        display: grid;
        gap: 10px;
      }}
      .historyCard.active {{
        border-color: rgba(64, 180, 255, 0.34);
        box-shadow: inset 3px 0 0 var(--accent), 0 0 0 1px rgba(64, 180, 255, 0.08);
        background: linear-gradient(180deg, rgba(64, 180, 255, 0.08), rgba(255, 255, 255, 0.015));
      }}
      html[data-theme='light'] .historyCard.active {{
        background: linear-gradient(180deg, rgba(22, 135, 217, 0.08), rgba(255, 255, 255, 0.62));
      }}
      .historyTop,
      .historyBottom {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 10px;
        flex-wrap: wrap;
      }}
      .historyTime {{
        font-size: 13px;
        font-weight: 600;
      }}
      .historyMeta,
      .historyTail {{
        color: var(--muted);
        font-size: 12px;
        line-height: 1.45;
      }}
      .historyBadges {{
        display: flex;
        flex-wrap: wrap;
        gap: 6px;
        justify-content: flex-end;
      }}
      .stateBadge {{
        display: inline-flex;
        align-items: center;
        gap: 5px;
        padding: 2px 7px;
        border-radius: 999px;
        border: 1px solid var(--panel-border);
        font-size: 10px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .stateDot {{
        width: 8px;
        height: 8px;
        border-radius: 999px;
        flex: 0 0 auto;
      }}
      .stateDot-running {{ background: var(--info); }}
      .stateDot-succeeded {{ background: var(--ok); }}
      .stateDot-failed,
      .stateDot-rolled_back,
      .stateDot-offline {{ background: var(--bad); }}
      .stateDot-idle,
      .stateDot-unknown {{ background: #7f93a8; }}
      .newBadge {{
        color: var(--warn);
        border: 1px solid rgba(255, 177, 85, 0.26);
        background: rgba(255, 177, 85, 0.14);
        border-radius: 999px;
        padding: 3px 8px;
        font-size: 10px;
      }}
      .statusPanel {{
        grid-area: detail;
        margin: 0;
      }}
      .statusGrid {{
        display: grid;
        grid-template-columns: repeat(2, minmax(0, 1fr));
        gap: 12px;
      }}
      .statusTile {{
        padding: 14px;
        border-radius: 18px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
        display: grid;
        gap: 8px;
        align-content: start;
      }}
      .statusTile-wide {{
        grid-column: auto;
      }}
      .statusTile-full {{
        grid-column: 1 / -1;
      }}
      .statusLabelRow {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 8px;
      }}
      .statusLabel {{
        color: var(--muted);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .copyButton {{
        position: relative;
        min-height: 24px;
        min-width: 24px;
        width: 24px;
        padding: 0;
        border-radius: 8px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        color: var(--muted);
        background: var(--surface-strong);
        overflow: visible;
      }}
      .copyButton::after {{
        content: attr(data-tooltip);
        position: absolute;
        left: 50%;
        bottom: calc(100% + 8px);
        transform: translate(-50%, 4px);
        padding: 4px 8px;
        border-radius: 8px;
        border: 1px solid var(--panel-border);
        background: var(--pop-bg);
        color: var(--text);
        font-size: 10px;
        line-height: 1.2;
        white-space: nowrap;
        box-shadow: var(--panel-shadow);
        opacity: 0;
        pointer-events: none;
        transition: opacity 140ms ease, transform 140ms ease;
      }}
      .copyButton::before {{
        content: '';
        position: absolute;
        left: 50%;
        bottom: calc(100% + 3px);
        width: 8px;
        height: 8px;
        transform: translateX(-50%) rotate(45deg);
        border-right: 1px solid var(--panel-border);
        border-bottom: 1px solid var(--panel-border);
        background: var(--pop-bg);
        opacity: 0;
        pointer-events: none;
        transition: opacity 140ms ease, transform 140ms ease;
      }}
      .copyButton:not([disabled]):is(:hover, :focus-visible)::after,
      .copyButton:not([disabled]):is(:hover, :focus-visible)::before {{
        opacity: 1;
        transform: translate(-50%, 0);
      }}
      .copyButton:not([disabled]):is(:hover, :focus-visible)::before {{
        transform: translateX(-50%) rotate(45deg);
      }}
      .copyButton svg {{
        width: 12px;
        height: 12px;
        display: block;
      }}
      .copyButton.copied {{
        color: var(--ok);
        border-color: rgba(50, 201, 108, 0.32);
        background: rgba(50, 201, 108, 0.12);
      }}
      .copyButton.copied::after {{
        color: var(--ok);
        border-color: rgba(50, 201, 108, 0.28);
      }}
      .copyButton.copied::before {{
        border-right-color: rgba(50, 201, 108, 0.28);
        border-bottom-color: rgba(50, 201, 108, 0.28);
      }}
      .copyButton.failed {{
        color: var(--bad);
        border-color: rgba(255, 123, 123, 0.32);
        background: rgba(255, 123, 123, 0.12);
      }}
      .copyButton.failed::after {{
        color: var(--bad);
        border-color: rgba(255, 123, 123, 0.28);
      }}
      .copyButton.failed::before {{
        border-right-color: rgba(255, 123, 123, 0.28);
        border-bottom-color: rgba(255, 123, 123, 0.28);
      }}
      .statusValue {{
        font-size: 13px;
        line-height: 1.35;
        font-weight: 600;
        word-break: break-word;
      }}
      .statusValue-lg {{
        font-size: clamp(22px, 2.4vw, 28px);
        line-height: 0.96;
        letter-spacing: -0.05em;
      }}
      .statusMeta {{
        color: var(--muted);
        font-size: 11px;
        line-height: 1.42;
      }}
      .statusTone {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 30px;
        padding: 5px 10px;
        border-radius: 999px;
        border: 1px solid var(--panel-border);
        background: var(--surface-strong);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .statusTone.state-running,
      .stateBadge-running {{
        color: var(--info);
        border-color: rgba(83, 169, 255, 0.32);
        background: rgba(83, 169, 255, 0.12);
      }}
      .statusTone.state-succeeded,
      .stateBadge-succeeded {{
        color: var(--ok);
        border-color: rgba(50, 201, 108, 0.28);
        background: rgba(50, 201, 108, 0.12);
      }}
      .statusTone.state-failed,
      .statusTone.state-rolled_back,
      .statusTone.state-offline,
      .stateBadge-failed,
      .stateBadge-rolled_back,
      .stateBadge-offline {{
        color: var(--bad);
        border-color: rgba(255, 123, 123, 0.28);
        background: rgba(255, 123, 123, 0.12);
      }}
      .statusTone.state-idle,
      .statusTone.state-unknown,
      .stateBadge-idle,
      .stateBadge-unknown {{
        color: var(--muted);
        border-color: var(--panel-border);
        background: var(--surface-strong);
      }}
      .statusCode,
      .statusCodeInline {{
        background: var(--console-bg);
        border: 1px solid var(--panel-border);
        border-radius: 14px;
        padding: 10px 12px;
        color: var(--text);
        font-family: 'IBM Plex Mono', 'JetBrains Mono', 'SFMono-Regular', 'Menlo', monospace;
        font-size: 12px;
        line-height: 1.58;
        white-space: pre-wrap;
        word-break: break-word;
      }}
      .statusCodeInline {{ min-height: 44px; }}
      .popWrap {{
        position: relative;
        display: inline-flex;
        align-items: center;
      }}
      .popCard {{
        position: absolute;
        top: calc(100% + 10px);
        right: 0;
        left: auto;
        width: min(320px, calc(100vw - 48px));
        border: 1px solid var(--button-border);
        border-radius: 16px;
        background: var(--pop-bg);
        box-shadow: var(--panel-shadow);
        padding: 12px;
        z-index: 20;
      }}
      .popTitle {{
        margin: 0 0 6px;
        font-size: 13px;
        font-weight: 700;
      }}
      .popActions {{
        display: flex;
        gap: 8px;
        justify-content: flex-end;
        margin-top: 12px;
      }}
      .danger {{
        border-color: rgba(220, 38, 38, 0.84);
        background: rgba(220, 38, 38, 0.92);
        color: #fff;
      }}
      @keyframes supervisorButtonSpin {{
        from {{ transform: rotate(0deg); }}
        to {{ transform: rotate(360deg); }}
      }}
      @media (prefers-reduced-motion: reduce) {{
        button,
        .linkButton {{ transition: none; }}
        button.btnRunning::before {{ animation: none; }}
      }}
      @media (max-width: 1024px) {{
        body {{
          padding-left: 16px;
          padding-right: 16px;
        }}
        .panel {{ padding: 15px; }}
        .mastheadRow,
        .statusGrid {{
          grid-template-columns: 1fr;
        }}
        .workspaceGrid {{
          grid-template-columns: 1fr;
          grid-template-areas:
            'logs'
            'sidebar'
            'detail';
        }}
        .actionDeckGrid {{
          grid-template-columns: 1fr;
          grid-template-areas:
            'field'
            'hint'
            'primary'
            'note'
            'aux';
          align-items: stretch;
        }}
        .buttonGroup-aux {{
          justify-content: flex-start;
          justify-self: stretch;
        }}
        .actionCallout,
        .buttonGroup-main,
        .buttonGroup-aux,
        .fieldBlock {{
          align-self: stretch;
        }}
        .statusSidebarHeader {{
          align-items: flex-start;
        }}
        .historyRail,
        .logPanel {{
          min-height: auto;
        }}
        .historyRail {{
          max-height: min(360px, 44vh);
        }}
        .logSummary,
        .sectionHeadingMeta {{
          text-align: left;
          max-width: none;
        }}
        .metaFooter {{
          grid-template-columns: 160px minmax(0, 1fr) 140px;
          gap: 14px;
        }}
        .metaPill + .metaPill {{
          padding-left: 14px;
        }}
      }}
      @media (max-width: 720px) {{
        body {{
          padding-left: 12px;
          padding-right: 12px;
        }}
        .shell {{
          gap: 12px;
          padding-top: 12px;
        }}
        .panel {{
          padding: 14px;
          border-radius: 18px;
        }}
        .mastheadRow,
        .brandRow,
        .sectionHeadingRow,
        .historyTop,
        .historyBottom,
        .logHeader,
        .statusSidebarHeader {{
          align-items: flex-start;
        }}
        .brandRow {{
          flex-direction: column;
          align-items: flex-start;
        }}
        .metaFooter,
        .buttonGroup,
        .buttonGroup-aux {{
          width: 100%;
        }}
        .metaFooter {{
          grid-template-columns: 1fr;
          gap: 10px;
          padding-top: 12px;
        }}
        .metaPill + .metaPill {{
          padding-left: 0;
          border-left: 0;
          padding-top: 10px;
          border-top: 1px solid var(--panel-border);
        }}
        .buttonGroup > *,
        .buttonGroup-aux > *,
        .linkButton,
        .popWrap,
        .popWrap > button {{
          width: 100%;
        }}
        .logTitle {{
          font-size: 15px;
        }}
        .popCard {{
          left: 0;
          right: auto;
          width: min(320px, calc(100vw - 36px));
        }}
      }}
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

      async function writeClipboardText(text) {{
        if (navigator.clipboard?.writeText) {{
          await navigator.clipboard.writeText(text);
          return;
        }}
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
        setCopyButtonValue(copyPreviousBtn, previousText !== '-' ? previousText : '');
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
        const cachedPreviousText = cached.previous ? formatPreviousRef(cached.previous) : '';
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
        setCopyButtonValue(copyPreviousBtn, cachedPreviousText);
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
            .unwrap_or_else(|_| "\"dockrev:theme\"".to_string())
    )
}
