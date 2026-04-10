pub(crate) const STYLE_CSS: &str = r#"
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
"#;
