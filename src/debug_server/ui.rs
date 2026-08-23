pub struct DebugPageContext {
    pub session: String,
    pub client: String,
    pub stream: String,
    pub limit: usize,
    pub sessions: Vec<String>,
    pub clients: Vec<String>,
    pub streams: Vec<String>,
    pub lines: Vec<String>,
}

pub fn render_index(context: &DebugPageContext) -> String {
    let session_options = render_options(&context.sessions, &context.session);
    let client_options = render_options(&context.clients, &context.client);
    let stream_options = render_options(&context.streams, &context.stream);
    let logs = if context.lines.is_empty() {
        "(no logs yet)".to_string()
    } else {
        context
            .lines
            .iter()
            .map(|line| escape_html(line))
            .collect::<Vec<_>>()
            .join("\n")
    };

    format!(
        r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Debug Server</title>
    <style>
      :root,
      .light,
      html[data-theme='light'] {{
        color-scheme: light;
        --background: 0 0% 100%;
        --foreground: 222.2 47% 11%;
        --card: 0 0% 100%;
        --card-foreground: 222.2 47% 11%;
        --popover: 0 0% 100%;
        --popover-foreground: 222.2 47% 11%;
        --primary: 222.2 47% 11%;
        --primary-foreground: 210 40% 98%;
        --secondary: 210 25% 96%;
        --secondary-foreground: 222.2 47% 11%;
        --muted: 210 22% 96%;
        --muted-foreground: 215 14% 40%;
        --accent: 210 22% 94%;
        --accent-foreground: 222.2 47% 11%;
        --destructive: 0 74% 52%;
        --destructive-foreground: 210 40% 98%;
        --success: 142 72% 35%;
        --success-foreground: 210 40% 98%;
        --warning: 35 92% 45%;
        --warning-foreground: 222.2 47% 11%;
        --border: 214 20% 88%;
        --input: 214 20% 88%;
        --ring: 221 39% 11%;
        --overlay: 222.2 47% 11%;
        --shadow-color: 222.2 47% 11%;
        --radius: 8px;
      }}
      .dark,
      html[data-theme='dark'] {{
        color-scheme: dark;
        --background: 222.2 47% 7%;
        --foreground: 210 40% 98%;
        --card: 222.2 47% 9%;
        --card-foreground: 210 40% 98%;
        --popover: 222.2 47% 9%;
        --popover-foreground: 210 40% 98%;
        --secondary: 217 30% 14%;
        --secondary-foreground: 210 40% 98%;
        --muted: 217 30% 14%;
        --muted-foreground: 215 20% 70%;
        --border: 217 30% 18%;
        --primary: 210 40% 98%;
        --primary-foreground: 222.2 47% 11%;
        --accent: 217 30% 16%;
        --accent-foreground: 210 40% 98%;
        --destructive: 0 62% 34%;
        --destructive-foreground: 210 40% 98%;
        --success: 142 62% 44%;
        --success-foreground: 210 40% 98%;
        --warning: 38 92% 54%;
        --warning-foreground: 222.2 47% 11%;
        --input: 217 30% 18%;
        --ring: 212 27% 84%;
        --overlay: 222.2 47% 4%;
        --shadow-color: 222.2 47% 4%;
        --radius: 8px;
      }}
      html {{
        height: 100%;
        font-size: 14px;
      }}
      body {{
        margin: 0;
        font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto, Helvetica, Arial, sans-serif;
        background: hsl(var(--background));
        color: hsl(var(--foreground));
        font-size: 14px;
        line-height: 1.5;
      }}
      .shell {{
        min-height: 100vh;
        background: hsl(var(--background));
      }}
      .header {{
        position: sticky;
        top: 0;
        z-index: 10;
        border-bottom: 1px solid hsl(var(--border));
        background: hsl(var(--background));
      }}
      .header-inner {{
        max-width: 1400px;
        margin: 0 auto;
        padding: 12px 16px;
        display: flex;
        align-items: center;
        justify-content: space-between;
      }}
      @media (min-width: 768px) {{
        .header-inner {{
          padding: 12px 24px;
        }}
      }}
      .brand {{
        display: flex;
        align-items: center;
        gap: 12px;
      }}
      .brand-badge {{
        display: flex;
        align-items: center;
        justify-content: center;
        width: 32px;
        height: 32px;
        border-radius: 6px;
        background: hsl(var(--primary));
        color: hsl(var(--primary-foreground));
        font-size: 11px;
        font-weight: 700;
        letter-spacing: 0.04em;
      }}
      .brand-title {{
        font-size: 0.875rem;
        font-weight: 600;
      }}
      .brand-subtitle {{
        font-size: 0.75rem;
        color: hsl(var(--muted-foreground));
      }}
      .page {{
        background: hsl(var(--secondary));
        min-height: calc(100vh - 61px);
      }}
      .wrap {{
        max-width: 1400px;
        margin: 0 auto;
        padding: 16px;
        display: grid;
        gap: 16px;
      }}
      @media (min-width: 768px) {{
        .wrap {{
          padding: 24px;
        }}
      }}
      .hero h1 {{
        margin: 0 0 6px;
        font-size: 1.5rem;
      }}
      .muted {{
        color: hsl(var(--muted-foreground));
        font-size: 0.875rem;
      }}
      .grid {{
        display: grid;
        gap: 16px;
      }}
      @media (min-width: 1024px) {{
        .grid {{
          grid-template-columns: 360px minmax(0, 1fr);
          align-items: start;
        }}
      }}
      .card {{
        border: 1px solid hsl(var(--border));
        border-radius: var(--radius);
        background: hsl(var(--card));
        color: hsl(var(--card-foreground));
        overflow: hidden;
      }}
      .card-pad {{
        padding: 20px;
      }}
      .card-header {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 12px;
        padding: 16px 20px;
        border-bottom: 1px solid hsl(var(--border));
        background: hsl(var(--card));
      }}
      .card-body {{
        padding: 20px;
      }}
      .stack {{
        display: grid;
        gap: 16px;
      }}
      .section-title {{
        margin: 0 0 12px;
        font-size: 0.875rem;
        font-weight: 600;
      }}
      .selectors {{
        display: grid;
        grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
        gap: 12px;
      }}
      .selector label {{
        display: block;
        font-size: 0.75rem;
        color: hsl(var(--muted-foreground));
        margin-bottom: 6px;
      }}
      .selector select {{
        width: 100%;
        height: 2.25rem;
        border-radius: calc(var(--radius) - 2px);
        border: 1px solid hsl(var(--input));
        background: hsl(var(--background));
        color: hsl(var(--foreground));
        padding: 0 12px;
        font: inherit;
      }}
      .theme-toggle {{
        display: inline-flex;
        align-items: center;
        gap: 8px;
        height: 2.25rem;
        padding: 0 12px;
      }}
      .theme-dot {{
        width: 8px;
        height: 8px;
        border-radius: 9999px;
        background: hsl(var(--foreground));
        opacity: 0.85;
      }}
      .actions {{
        display: flex;
        gap: 8px;
        flex-wrap: wrap;
      }}
      a, button {{
        border: 1px solid hsl(var(--border));
        border-radius: 8px;
        padding: 8px 12px;
        cursor: pointer;
        text-decoration: none;
        font: inherit;
        background: hsl(var(--secondary));
        color: hsl(var(--secondary-foreground));
      }}
      pre {{
        white-space: pre-wrap;
        word-break: break-word;
        background: hsl(var(--background));
        border: 1px solid hsl(var(--border));
        border-radius: 12px;
        padding: 16px;
        min-height: 420px;
        overflow: auto;
        font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
        font-size: 0.875rem;
        line-height: 1.6;
      }}
      .selection-list {{
        display: grid;
        gap: 10px;
      }}
      .selection-row {{
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 16px;
        padding: 10px 12px;
        border: 1px solid hsl(var(--border));
        border-radius: calc(var(--radius) - 2px);
        background: hsl(var(--background));
      }}
      .selection-key {{
        color: hsl(var(--muted-foreground));
        font-size: 0.75rem;
        text-transform: uppercase;
        letter-spacing: 0.03em;
      }}
      .selection-value {{
        font-size: 0.875rem;
        font-weight: 500;
      }}
      .panel-section + .panel-section {{
        padding-top: 16px;
        border-top: 1px solid hsl(var(--border));
      }}
    </style>
    <script>
      (function() {{
        const STORAGE_KEYS = ['SP_THEME', 'ADMIN_THEME', 'cw-theme'];
        function isTheme(value) {{
          return value === 'light' || value === 'dark';
        }}
        function resolveThemeMode() {{
          const query = new URLSearchParams(window.location.search);
          const queryTheme = query.get('theme');
          if (isTheme(queryTheme)) return queryTheme;
          for (const key of STORAGE_KEYS) {{
            const value = window.localStorage.getItem(key);
            if (isTheme(value)) return value;
          }}
          return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light';
        }}
        function applyTheme(mode) {{
          const root = document.documentElement;
          root.dataset.theme = mode;
          root.classList.toggle('dark', mode === 'dark');
          root.classList.toggle('light', mode === 'light');
          root.style.colorScheme = mode;
        }}
        applyTheme(resolveThemeMode());
      }})();
    </script>
  </head>
  <body>
    <div class="shell">
      <header class="header">
        <div class="header-inner">
          <div class="brand">
            <div class="brand-badge">SP</div>
            <div>
              <div class="brand-title">SPEAR Operations Console</div>
              <div class="brand-subtitle">Enterprise console</div>
            </div>
          </div>
          <button type="button" class="theme-toggle" id="theme-toggle" onclick="toggleTheme()"><span class="theme-dot"></span>Dark</button>
        </div>
      </header>
      <main class="page">
        <div class="wrap">
          <section class="hero">
            <h1>Debug Streams</h1>
            <div class="muted">Inspect runtime debug events by session, client, and stream.</div>
          </section>
          <div class="grid">
            <section class="card">
              <div class="card-header">
                <div class="section-title" style="margin: 0;">Filters</div>
              </div>
              <div class="card-body stack">
                <div class="panel-section">
                <div class="selectors">
                  <div class="selector">
                    <label for="session-select">Session</label>
                    <select id="session-select" onchange="changeSession(this.value)">{session_options}</select>
                  </div>
                  <div class="selector">
                    <label for="client-select">Client</label>
                    <select id="client-select" onchange="changeClient(this.value)">{client_options}</select>
                  </div>
                  <div class="selector">
                    <label for="stream-select">Stream</label>
                    <select id="stream-select" onchange="changeStream(this.value)">{stream_options}</select>
                  </div>
                </div>
                </div>
              <div class="panel-section">
                <div class="section-title">Selection</div>
                <div class="selection-list">
                  <div class="selection-row">
                    <div class="selection-key">Session</div>
                    <div class="selection-value">{session}</div>
                  </div>
                  <div class="selection-row">
                    <div class="selection-key">Client</div>
                    <div class="selection-value">{client}</div>
                  </div>
                  <div class="selection-row">
                    <div class="selection-key">Stream</div>
                    <div class="selection-value">{stream}</div>
                  </div>
                  <div class="selection-row">
                    <div class="selection-key">Limit</div>
                    <div class="selection-value">{limit}</div>
                  </div>
                </div>
              </div>
              <div class="panel-section">
                <div class="section-title">Actions</div>
                <div class="actions">
                  <a href="/?limit={limit}&session={session_q}&client={client_q}&stream={stream_q}">Refresh</a>
                  <a href="/logs?limit={limit}&session={session_q}&client={client_q}&stream={stream_q}" target="_blank" rel="noreferrer">Raw JSON</a>
                  <a href="/streams?session={session_q}&client={client_q}" target="_blank" rel="noreferrer">Hierarchy</a>
                  <button type="button" onclick="clearLogs()">Clear Logs</button>
                </div>
              </div>
              </div>
            </section>
            <section class="card">
              <div class="card-header">
                <div>
                  <div class="section-title" style="margin: 0;">Logs</div>
                  <div class="muted">Showing the latest {limit} lines for the selected stream.</div>
                </div>
              </div>
              <div class="card-body">
                <pre id="logs">{logs}</pre>
              </div>
            </section>
          </div>
        </div>
      </main>
    </div>
    <script>
      const limit = {limit};
      const defaultClient = {default_client:?};
      const defaultStream = {default_stream:?};
      const currentSession = {current_session:?};
      const currentClient = {current_client:?};
      const themeStorageKeys = ['SP_THEME', 'ADMIN_THEME', 'cw-theme'];

      function currentTheme() {{
        const query = new URLSearchParams(window.location.search);
        const queryTheme = query.get('theme');
        if (queryTheme === 'light' || queryTheme === 'dark') return queryTheme;
        const datasetTheme = document.documentElement.dataset.theme;
        return datasetTheme === 'light' ? 'light' : 'dark';
      }}

      function applyTheme(mode) {{
        document.documentElement.dataset.theme = mode;
        document.documentElement.classList.toggle('dark', mode === 'dark');
        document.documentElement.classList.toggle('light', mode === 'light');
        document.documentElement.style.colorScheme = mode;
        const toggle = document.getElementById('theme-toggle');
        if (toggle) {{
          toggle.textContent = mode === 'dark' ? 'Light' : 'Dark';
        }}
      }}

      function toUrl(session, client, stream, theme) {{
        const params = new URLSearchParams({{
          limit: String(limit),
          session,
          client,
          stream,
        }});
        if (theme === 'light' || theme === 'dark') {{
          params.set('theme', theme);
        }}
        location.href = `/?${{params.toString()}}`;
      }}

      function changeSession(nextSession) {{
        toUrl(nextSession, defaultClient, defaultStream, currentTheme());
      }}

      function changeClient(nextClient) {{
        toUrl(currentSession, nextClient, defaultStream, currentTheme());
      }}

      function changeStream(nextStream) {{
        toUrl(currentSession, currentClient, nextStream, currentTheme());
      }}

      function toggleTheme() {{
        const nextTheme = currentTheme() === 'dark' ? 'light' : 'dark';
        if (nextTheme !== 'light' && nextTheme !== 'dark') {{
          return;
        }}
        for (const key of themeStorageKeys) {{
          window.localStorage.setItem(key, nextTheme);
        }}
        applyTheme(nextTheme);
        toUrl(currentSession, currentClient, document.getElementById('stream-select').value, nextTheme);
      }}

      async function clearLogs() {{
        const resp = await fetch('/logs?session={session_q}&client={client_q}&stream={stream_q}', {{ method: 'DELETE' }});
        if (!resp.ok) {{
          alert('failed to clear logs');
          return;
        }}
        location.reload();
      }}

      applyTheme(currentTheme());
      setTimeout(() => location.reload(), 5000);
    </script>
  </body>
</html>"#,
        session_options = session_options,
        client_options = client_options,
        stream_options = stream_options,
        logs = logs,
        limit = context.limit,
        session = escape_html(&context.session),
        client = escape_html(&context.client),
        stream = escape_html(&context.stream),
        session_q = url_encode(&context.session),
        client_q = url_encode(&context.client),
        stream_q = url_encode(&context.stream),
        default_client = super::config::DEFAULT_CLIENT,
        default_stream = super::config::DEFAULT_STREAM,
        current_session = context.session.as_str(),
        current_client = context.client.as_str(),
    )
}

fn render_options(items: &[String], current: &str) -> String {
    items
        .iter()
        .map(|item| {
            if item == current {
                format!(
                    r#"<option value="{}" selected>{}</option>"#,
                    escape_html(item),
                    escape_html(item)
                )
            } else {
                format!(
                    r#"<option value="{}">{}</option>"#,
                    escape_html(item),
                    escape_html(item)
                )
            }
        })
        .collect::<Vec<_>>()
        .join("")
}

fn url_encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
