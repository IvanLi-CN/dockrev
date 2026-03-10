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
        --muted: rgba(220, 234, 254, 0.76);
        --panel: rgba(10, 28, 54, 0.88);
        --panel-strong: rgba(12, 34, 66, 0.96);
        --panel-border: rgba(156, 192, 232, 0.24);
        --panel-shadow: 0 26px 74px rgba(1, 10, 24, 0.52);
        --surface: rgba(255, 255, 255, 0.055);
        --surface-strong: rgba(255, 255, 255, 0.08);
        --button-bg: rgba(255, 255, 255, 0.05);
        --button-hover: rgba(54, 191, 250, 0.16);
        --button-border: rgba(188, 223, 255, 0.22);
        --button-text: var(--text);
        --input-bg: rgba(255, 255, 255, 0.05);
        --input-border: rgba(188, 223, 255, 0.2);
        --input-shadow: 0 0 0 1px rgba(255, 255, 255, 0.01);
        --pre-bg: rgba(4, 11, 26, 0.76);
        --console-bg: rgba(2, 10, 24, 0.88);
        --pop-bg: rgba(7, 20, 42, 0.98);
        --link: #7dd3fc;
        --link-hover: #bae6fd;
        --code-bg: rgba(255, 255, 255, 0.08);
        --accent: rgba(54, 191, 250, 0.18);
        --accent-strong: #36bffa;
        --spinner-track: rgba(232, 241, 255, 0.24);
        --spinner-head: rgba(232, 241, 255, 0.84);
        --ok: #22c55e;
        --bad: #f87171;
        --warn: #fbbf24;
        --info: #60a5fa;
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
        --muted: rgba(58, 86, 118, 0.84);
        --panel: rgba(255, 255, 255, 0.94);
        --panel-strong: rgba(255, 255, 255, 0.985);
        --panel-border: rgba(14, 56, 100, 0.14);
        --panel-shadow: 0 22px 50px rgba(17, 62, 112, 0.14);
        --surface: rgba(12, 121, 207, 0.05);
        --surface-strong: rgba(12, 121, 207, 0.09);
        --button-bg: rgba(255, 255, 255, 0.92);
        --button-hover: rgba(12, 121, 207, 0.1);
        --button-border: rgba(14, 56, 100, 0.16);
        --button-text: rgba(16, 40, 66, 0.96);
        --input-bg: rgba(255, 255, 255, 0.9);
        --input-border: rgba(14, 56, 100, 0.16);
        --input-shadow: 0 0 0 1px rgba(12, 121, 207, 0.03);
        --pre-bg: rgba(12, 121, 207, 0.06);
        --console-bg: rgba(245, 250, 255, 0.98);
        --pop-bg: rgba(255, 255, 255, 0.98);
        --link: #0c79cf;
        --link-hover: #095ea0;
        --code-bg: rgba(12, 121, 207, 0.08);
        --accent: rgba(12, 121, 207, 0.12);
        --accent-strong: #0c79cf;
        --spinner-track: rgba(16, 40, 66, 0.16);
        --spinner-head: rgba(16, 40, 66, 0.72);
        --ok: #138347;
        --bad: #bf1240;
        --warn: #a55706;
        --info: #2563eb;
        --selection: rgba(12, 121, 207, 0.18);
      }}
      * {{ box-sizing: border-box; }}
      html {{ background: var(--bg); }}
      body {{
        font-family: 'Avenir Next', 'Avenir', 'SF Pro Display', 'Segoe UI', 'PingFang SC', 'Hiragino Sans GB', 'Microsoft YaHei', 'Noto Sans SC', system-ui, -apple-system, sans-serif;
        min-height: 100vh;
        margin: 0 auto;
        padding: 0 20px 40px;
        max-width: 1240px;
        background: var(--bg-layered);
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
        min-height: 44px;
        padding: 10px 14px;
        border-radius: 14px;
        border: 1px solid var(--button-border);
        background: var(--button-bg);
        color: var(--button-text);
        cursor: pointer;
        transition: background-color 160ms ease, border-color 160ms ease, box-shadow 160ms ease, opacity 160ms ease;
      }}
      button:hover:not([disabled]) {{
        background: var(--button-hover);
        border-color: rgba(54, 191, 250, 0.36);
        box-shadow: 0 0 0 1px rgba(54, 191, 250, 0.08);
      }}
      button:focus-visible,
      input:focus-visible {{
        outline: 2px solid rgba(54, 191, 250, 0.42);
        outline-offset: 2px;
      }}
      button[disabled] {{
        opacity: 0.52;
        cursor: not-allowed;
        box-shadow: none;
      }}
      button.primary {{
        background: linear-gradient(180deg, rgba(54, 191, 250, 0.28) 0%, rgba(23, 118, 196, 0.34) 100%);
        border-color: rgba(93, 209, 255, 0.42);
      }}
      html[data-theme='light'] button.primary {{
        background: linear-gradient(180deg, rgba(12, 121, 207, 0.18) 0%, rgba(12, 121, 207, 0.12) 100%);
        border-color: rgba(12, 121, 207, 0.3);
      }}
      button.btnRunning {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        gap: 8px;
      }}
      button.btnRunning::before {{
        content: '';
        width: 12px;
        height: 12px;
        border-radius: 999px;
        border: 2px solid var(--spinner-track);
        border-top-color: var(--spinner-head);
        animation: supervisorButtonSpin 0.8s linear infinite;
      }}
      input {{
        width: 100%;
        min-width: 0;
        padding: 12px 14px;
        border-radius: 14px;
        border: 1px solid var(--input-border);
        background: var(--input-bg);
        color: var(--text);
        box-shadow: var(--input-shadow);
      }}
      pre {{
        margin: 0;
        padding: 16px;
        border-radius: 18px;
        overflow: auto;
        white-space: pre-wrap;
        word-break: break-word;
        color: var(--text);
        line-height: 1.62;
        font-family: 'SFMono-Regular', 'JetBrains Mono', 'Fira Code', 'IBM Plex Mono', 'Menlo', monospace;
      }}
      .shell {{
        display: grid;
        gap: 20px;
        padding-top: 28px;
      }}
      .panel {{
        border: 1px solid var(--panel-border);
        border-radius: 24px;
        padding: 22px;
        background: var(--panel);
        box-shadow: var(--panel-shadow);
        backdrop-filter: blur(18px);
      }}
      .muted {{
        color: var(--muted);
        font-size: 13px;
        line-height: 1.55;
      }}
      .eyebrow,
      .sectionEyebrow {{
        color: var(--muted);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.12em;
      }}
      .sectionHeadingRow {{
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 14px;
        flex-wrap: wrap;
        margin-bottom: 18px;
      }}
      .sectionHeadingRow h2 {{
        margin: 6px 0 0;
        font-size: 24px;
        line-height: 1.1;
      }}
      .masthead {{
        padding-top: 26px;
        padding-bottom: 24px;
      }}
      .mastheadTop {{
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 20px;
      }}
      .brandRow {{
        display: flex;
        gap: 16px;
        align-items: center;
      }}
      .brandMark {{
        width: 52px;
        height: 52px;
        display: inline-flex;
        align-items: center;
        justify-content: center;
        border-radius: 18px;
        background: linear-gradient(160deg, rgba(54, 191, 250, 0.18), rgba(54, 191, 250, 0.04));
        border: 1px solid rgba(54, 191, 250, 0.22);
        flex: 0 0 auto;
      }}
      .brandMark img {{
        display: block;
      }}
      .masthead h1 {{
        margin: 6px 0 0;
        font-size: clamp(34px, 4.6vw, 64px);
        line-height: 0.96;
        letter-spacing: -0.04em;
      }}
      .intro {{
        max-width: 760px;
        margin: 16px 0 0;
        color: var(--muted);
        font-size: clamp(15px, 1.8vw, 18px);
        line-height: 1.65;
      }}
      .metaGrid {{
        display: grid;
        grid-template-columns: repeat(3, minmax(0, 1fr));
        gap: 14px;
        margin-top: 24px;
      }}
      .metaCard {{
        padding: 14px 16px;
        border-radius: 18px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
      }}
      .metaLabel {{
        color: var(--muted);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .metaValue {{
        margin-top: 8px;
        font-size: 15px;
        line-height: 1.45;
        word-break: break-word;
      }}
      .linkButton {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 44px;
        padding: 10px 16px;
        border-radius: 14px;
        border: 1px solid var(--button-border);
        background: var(--surface);
        color: var(--link);
        flex: 0 0 auto;
      }}
      .linkButton:hover {{
        background: var(--button-hover);
        text-decoration: none;
      }}
      .actionDeckGrid {{
        display: grid;
        grid-template-columns: minmax(220px, 280px) minmax(0, 1fr) auto;
        gap: 16px;
        align-items: end;
      }}
      .fieldBlock {{
        display: grid;
        gap: 10px;
      }}
      .fieldLabel {{
        font-size: 13px;
        color: var(--muted);
        letter-spacing: 0.02em;
      }}
      .fieldHint {{
        font-size: 12px;
        color: var(--muted);
      }}
      .buttonGroup {{
        display: flex;
        flex-wrap: wrap;
        gap: 12px;
        align-items: center;
      }}
      .buttonGroup > * {{
        flex: 0 0 auto;
      }}
      .buttonGroup-aux {{
        justify-content: flex-end;
      }}
      .statusGrid {{
        display: grid;
        grid-template-columns: repeat(4, minmax(0, 1fr));
        gap: 14px;
      }}
      .statusTile {{
        min-height: 122px;
        padding: 16px;
        border-radius: 20px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
        display: grid;
        gap: 10px;
        align-content: start;
      }}
      .statusTile-hero {{
        background: linear-gradient(180deg, rgba(54, 191, 250, 0.14) 0%, rgba(54, 191, 250, 0.05) 100%);
      }}
      html[data-theme='light'] .statusTile-hero {{
        background: linear-gradient(180deg, rgba(12, 121, 207, 0.12) 0%, rgba(12, 121, 207, 0.05) 100%);
      }}
      .statusTile-wide {{
        grid-column: span 2;
        min-height: 156px;
      }}
      .statusTile-full {{
        grid-column: 1 / -1;
        min-height: auto;
      }}
      .statusLabel {{
        color: var(--muted);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .statusValue {{
        font-size: 17px;
        line-height: 1.42;
        font-weight: 600;
        word-break: break-word;
      }}
      .statusValue-lg {{
        font-size: clamp(26px, 4vw, 38px);
        line-height: 1;
        letter-spacing: -0.04em;
      }}
      .statusMeta {{
        color: var(--muted);
        font-size: 13px;
        line-height: 1.55;
      }}
      .statusTone {{
        display: inline-flex;
        align-items: center;
        justify-content: center;
        min-height: 36px;
        padding: 6px 12px;
        border-radius: 999px;
        border: 1px solid var(--panel-border);
        background: var(--surface-strong);
        font-size: 12px;
        text-transform: uppercase;
        letter-spacing: 0.08em;
      }}
      .statusTone.state-running,
      .stateBadge-running {{
        color: var(--info);
        border-color: rgba(96, 165, 250, 0.32);
        background: rgba(96, 165, 250, 0.12);
      }}
      .statusTone.state-succeeded,
      .stateBadge-succeeded {{
        color: var(--ok);
        border-color: rgba(34, 197, 94, 0.28);
        background: rgba(34, 197, 94, 0.12);
      }}
      .statusTone.state-failed,
      .statusTone.state-rolled_back,
      .statusTone.state-offline,
      .stateBadge-failed,
      .stateBadge-rolled_back,
      .stateBadge-offline {{
        color: var(--bad);
        border-color: rgba(248, 113, 113, 0.28);
        background: rgba(248, 113, 113, 0.12);
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
        background: var(--pre-bg);
        border: 1px solid var(--panel-border);
        border-radius: 16px;
        padding: 14px;
        color: var(--text);
        font-family: 'SFMono-Regular', 'JetBrains Mono', 'Fira Code', 'IBM Plex Mono', 'Menlo', monospace;
        font-size: 13px;
        line-height: 1.6;
        white-space: pre-wrap;
        word-break: break-word;
      }}
      .statusCodeInline {{
        min-height: 64px;
      }}
      .workspaceGrid {{
        display: grid;
        grid-template-columns: minmax(280px, 340px) minmax(0, 1fr);
        gap: 16px;
        align-items: stretch;
      }}
      .workspaceGrid.workspaceGrid-logsOnly {{
        grid-template-columns: minmax(0, 1fr);
      }}
      .historyRail,
      .logPanel {{
        min-height: 420px;
        border-radius: 22px;
        border: 1px solid var(--panel-border);
        background: var(--surface);
      }}
      .historyRail {{
        padding: 16px;
        max-height: min(680px, calc(100vh - 220px));
        overflow: auto;
        overscroll-behavior: contain;
      }}
      .historyList {{
        display: grid;
        gap: 12px;
      }}
      .historyCard {{
        width: 100%;
        min-height: 96px;
        padding: 14px 16px;
        border-radius: 18px;
        border: 1px solid var(--panel-border);
        background: var(--panel-strong);
        color: var(--text);
        text-align: left;
        display: grid;
        gap: 10px;
      }}
      .historyCard.active {{
        border-color: rgba(54, 191, 250, 0.38);
        background: linear-gradient(180deg, rgba(54, 191, 250, 0.16) 0%, rgba(54, 191, 250, 0.07) 100%);
        box-shadow: 0 0 0 1px rgba(54, 191, 250, 0.08);
      }}
      html[data-theme='light'] .historyCard.active {{
        background: linear-gradient(180deg, rgba(12, 121, 207, 0.12) 0%, rgba(12, 121, 207, 0.04) 100%);
      }}
      .historyTop,
      .historyBottom {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        flex-wrap: wrap;
      }}
      .historyTime {{
        font-size: 15px;
        font-weight: 600;
      }}
      .historyMeta,
      .historyTail {{
        color: var(--muted);
        font-size: 12px;
        line-height: 1.5;
      }}
      .historyBadges {{
        display: flex;
        flex-wrap: wrap;
        gap: 8px;
        justify-content: flex-end;
      }}
      .stateBadge {{
        display: inline-flex;
        align-items: center;
        gap: 6px;
        padding: 4px 8px;
        border-radius: 999px;
        border: 1px solid var(--panel-border);
        font-size: 11px;
        text-transform: uppercase;
        letter-spacing: 0.06em;
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
      .stateDot-unknown {{ background: #6b7280; }}
      .newBadge {{
        color: var(--warn);
        border: 1px solid rgba(251, 191, 36, 0.28);
        background: rgba(251, 191, 36, 0.16);
        border-radius: 999px;
        padding: 4px 8px;
        font-size: 11px;
      }}
      .logPanel {{
        display: grid;
        grid-template-rows: auto 1fr;
        padding: 16px;
      }}
      .logHeader {{
        display: flex;
        justify-content: space-between;
        align-items: flex-start;
        gap: 12px;
        flex-wrap: wrap;
        margin-bottom: 14px;
      }}
      .logTitle {{
        margin-top: 6px;
        font-size: 20px;
        line-height: 1.1;
        font-weight: 600;
      }}
      .logSummary {{
        max-width: 320px;
        text-align: right;
      }}
      #logs {{
        height: 100%;
        min-height: 300px;
        background: var(--console-bg);
      }}
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
        width: min(340px, calc(100vw - 48px));
        border: 1px solid var(--button-border);
        border-radius: 18px;
        background: var(--pop-bg);
        box-shadow: var(--panel-shadow);
        padding: 14px;
        z-index: 20;
      }}
      .popTitle {{
        margin: 0 0 6px;
        font-size: 14px;
        font-weight: 700;
      }}
      .popActions {{
        display: flex;
        gap: 8px;
        justify-content: flex-end;
        margin-top: 12px;
      }}
      .danger {{
        border-color: rgba(220, 38, 38, 0.92);
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
          padding-left: 18px;
          padding-right: 18px;
        }}
        .panel {{
          padding: 20px;
        }}
        .metaGrid,
        .statusGrid,
        .actionDeckGrid,
        .workspaceGrid {{
          grid-template-columns: repeat(2, minmax(0, 1fr));
        }}
        .actionDeckGrid > :last-child {{
          grid-column: 1 / -1;
        }}
        .workspaceGrid {{
          grid-template-columns: 1fr;
        }}
        .historyRail,
        .logPanel {{
          min-height: auto;
        }}
        .historyRail {{
          max-height: min(420px, 50vh);
        }}
        .logSummary {{
          text-align: left;
          max-width: none;
        }}
      }}
      @media (max-width: 720px) {{
        body {{
          padding-left: 14px;
          padding-right: 14px;
        }}
        .shell {{
          gap: 16px;
          padding-top: 16px;
        }}
        .panel {{
          padding: 16px;
          border-radius: 20px;
        }}
        .mastheadTop,
        .brandRow,
        .sectionHeadingRow,
        .historyTop,
        .historyBottom,
        .logHeader {{
          align-items: flex-start;
        }}
        .mastheadTop,
        .brandRow {{
          flex-direction: column;
        }}
        .metaGrid,
        .statusGrid,
        .actionDeckGrid,
        .workspaceGrid {{
          grid-template-columns: 1fr;
        }}
        .statusTile-wide,
        .statusTile-full {{
          grid-column: auto;
        }}
        .buttonGroup,
        .buttonGroup-aux {{
          width: 100%;
          justify-content: stretch;
        }}
        .buttonGroup > *,
        .buttonGroup-aux > *,
        .linkButton,
        .popWrap,
        .popWrap > button {{
          width: 100%;
        }}
        .logTitle {{
          font-size: 18px;
        }}
        .popCard {{
          left: 0;
          right: auto;
          width: min(340px, calc(100vw - 40px));
        }}
      }}
    </style>
  </head>
  <body>
    <main class="shell">
      <section class="panel masthead" data-panel="masthead">
        <div class="mastheadTop">
          <div>
            <div class="brandRow">
              <div class="brandMark">
                <img src="{base_path}/favicon.png" alt="" aria-hidden="true" width="26" height="26" />
              </div>
              <div>
                <div class="eyebrow">Supervisor Console</div>
                <h1>Dockrev 自我升级（Supervisor）</h1>
              </div>
            </div>
            <p class="intro">该页面独立于 Dockrev 生命周期；Dockrev 重启期间仍可用。这里会持续轮询升级状态，并保留最近 operation 的上下文供排障使用。</p>
          </div>
          <a class="linkButton" href="/">返回 Dockrev</a>
        </div>
        <div class="metaGrid">
          <div class="metaCard">
            <div class="metaLabel">Supervisor 版本</div>
            <div class="metaValue">{version_html}</div>
          </div>
          <div class="metaCard">
            <div class="metaLabel">开源仓库</div>
            <div class="metaValue">{repository_html}</div>
          </div>
          <div class="metaCard">
            <div class="metaLabel">开发者</div>
            <div class="metaValue">{developer_html}</div>
          </div>
        </div>
      </section>

      <section class="panel actionDeck" data-panel="action-deck">
        <div class="sectionHeadingRow">
          <div>
            <div class="sectionEyebrow">Action deck</div>
            <h2>升级控制台</h2>
          </div>
          <div class="muted">失败将尝试回滚到 previous digest（如可用），所有请求都会在同一页面保留运行态。</div>
        </div>
        <div class="actionDeckGrid">
          <label class="fieldBlock" for="tag">
            <span class="fieldLabel">Target tag</span>
            <input id="tag" value="latest" />
            <span class="fieldHint">默认使用 <code>latest</code>，也支持输入固定 tag 进行验证或升级。</span>
          </label>
          <div class="buttonGroup buttonGroup-main">
            <button id="dry">预览（dry-run）</button>
            <button id="apply" class="primary">开始升级（apply）</button>
          </div>
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

      <section class="panel statusPanel" data-panel="status-grid">
        <div class="sectionHeadingRow">
          <div>
            <div class="sectionEyebrow">Live status</div>
            <h2>当前状态</h2>
          </div>
          <div id="statusTone" class="statusTone" aria-live="polite">loading…</div>
        </div>
        <div class="statusGrid">
          <article class="statusTile statusTile-hero">
            <div class="statusLabel">State</div>
            <div id="statusState" class="statusValue statusValue-lg">loading…</div>
            <div id="statusSummary" class="statusMeta">等待首次轮询结果…</div>
          </article>
          <article class="statusTile">
            <div class="statusLabel">Current opId</div>
            <div id="statusOpId" class="statusValue">-</div>
            <div class="statusMeta">当前正在追踪的 operation 标识。</div>
          </article>
          <article class="statusTile">
            <div class="statusLabel">Current step</div>
            <div id="statusStep" class="statusValue">-</div>
            <div id="statusMode" class="statusMeta">mode -</div>
          </article>
          <article class="statusTile">
            <div class="statusLabel">Timestamps</div>
            <div id="statusStartedAt" class="statusValue">-</div>
            <div id="statusUpdatedAt" class="statusMeta">updated -</div>
          </article>
          <article class="statusTile statusTile-full">
            <div class="statusLabel">Progress message</div>
            <div id="statusProgressMessage" class="statusCodeInline">-</div>
          </article>
          <article class="statusTile statusTile-wide">
            <div class="statusLabel">Target</div>
            <pre id="statusTarget" class="statusCode">-</pre>
          </article>
          <article class="statusTile statusTile-wide">
            <div class="statusLabel">Previous</div>
            <pre id="statusPrevious" class="statusCode">-</pre>
          </article>
        </div>
      </section>

      <section class="panel workspacePanel" data-panel="workspace">
        <div class="sectionHeadingRow">
          <div>
            <div class="sectionEyebrow">History & logs</div>
            <h2>操作历史与日志</h2>
          </div>
          <div id="historyHint" class="muted">loading…</div>
        </div>
        <div id="workspaceGrid" class="workspaceGrid">
          <aside id="historyRail" class="historyRail">
            <div id="historyList" class="historyList"></div>
          </aside>
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
        </div>
      </section>
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

      function normalizeState(value) {{
        return value || 'unknown';
      }}

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

      function formatPreviousRef(previous) {{
        const tag = previous?.tag || '-';
        return `${{tag}}${{previous?.digest ? '@' + previous.digest : ''}}`;
      }}

      function formatLogs(logs) {{
        return (logs || []).map((line) => `[${{line.ts}}] ${{line.level}} ${{line.msg}}`).join('\n');
      }}

      function renderStatus(st) {{
        statusToneEl.className = statusToneClass(st?.state);
        statusToneEl.textContent = normalizeState(st?.state);
        statusStateEl.textContent = normalizeState(st?.state);
        statusSummaryEl.textContent = `${{st?.request?.mode || 'mode -'}} · auto-refresh 1.5s`;
        statusOpIdEl.textContent = st?.opId || '-';
        statusStepEl.textContent = st?.progress?.step || '-';
        statusModeEl.textContent = `mode ${{st?.request?.mode || '-'}}`;
        statusStartedAtEl.textContent = formatTimestamp(st?.startedAt);
        statusUpdatedAtEl.textContent = `updated ${{formatTimestamp(st?.updatedAt)}}`;
        statusProgressMessageEl.textContent = st?.progress?.message || '-';
        statusTargetEl.textContent = formatTargetRef(st?.target);
        statusPreviousEl.textContent = formatPreviousRef(st?.previous);
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
          return;
        }}
        statusOpIdEl.textContent = cached.opId ? `${{cached.opId}} · stale` : 'stale';
        statusStepEl.textContent = cached.progress?.step ? `${{cached.progress.step}} · stale` : 'stale';
        statusModeEl.textContent = `last seen ${{lastSeen}} · cached`;
        statusStartedAtEl.textContent = cached.startedAt ? `${{formatTimestamp(cached.startedAt)}} · stale` : '-';
        statusUpdatedAtEl.textContent = `last seen ${{lastSeen}}`;
        statusProgressMessageEl.textContent = cached.progress?.message
          ? `${{cached.progress.message}} · stale while offline`
          : 'offline; waiting for supervisor to respond again';
        statusTargetEl.textContent = cached.target
          ? `${{formatTargetRef(cached.target)}} · stale`
          : 'stale while offline';
        statusPreviousEl.textContent = cached.previous
          ? `${{formatPreviousRef(cached.previous)}} · stale`
          : 'stale while offline';
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
          logsEl.textContent = formatLogs(st.logs || []) || '暂无日志';
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
        logsEl.textContent = formatLogs(active.logs || []) || '暂无日志';
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
            logsEl.textContent = '等待日志…';
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
