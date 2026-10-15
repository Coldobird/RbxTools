// Used from node_repl with the initialized @oai/sky client. The Rust probe
// records signals independently so screenshot capture cannot delay detection.
import fs from 'node:fs/promises';
import path from 'node:path';

const escapeHtml = value => String(value).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' })[c]);

export async function createReadinessReport(sky, directory) {
  await fs.mkdir(directory, { recursive: true });
  let eventsPath = path.join(directory, 'events.jsonl');
  const rows = [];
  let consumed = 0;
  let screenshotNumber = (await fs.readdir(directory)).filter(name => /^screenshot-\d+\.(jpg|png)$/.test(name)).length;

  async function saveReport() {
    await fs.writeFile(path.join(directory, 'captures.json'), JSON.stringify(rows, null, 2));
    await fs.writeFile(path.join(directory, 'index.html'), `<!doctype html><html><head><meta charset="utf-8"><title>Steam readiness comparison</title><style>
      body{font:16px system-ui;background:#17191d;color:#eee;margin:32px auto;max-width:1200px;padding:0 20px}article{background:#25282e;padding:20px;margin:20px 0;border-radius:8px}img{max-width:100%;border:1px solid #555}small{color:#bbb}pre{white-space:pre-wrap}a{color:#9dcaff}
      </style></head><body><h1>Steam readiness comparison</h1><p>Choose the earliest screenshot where Steam looks loaded. Signals are timed by the independent native probe; screenshots are captured afterward. Several close signals can share one capture. Before a Steam window exists, a screenshot is unavailable.</p>
      ${rows.map((row, i) => `<article id="capture-${i + 1}"><h2>${i + 1}. ${escapeHtml(row.event.signal ?? row.event.event ?? row.event.kind ?? 'Event')}</h2><pre>${escapeHtml(JSON.stringify(row.event, null, 2))}</pre>${row.image ? `<p>Captured ${escapeHtml(row.capturedAt)}. Screenshot ${row.screenshotNumber}.</p><a href="${escapeHtml(row.image)}"><img src="${escapeHtml(row.image)}"></a>` : `<p>${escapeHtml(row.note)}</p>`}</article>`).join('')}
      </body></html>`);
  }

  async function poll() {
    let raw;
    try { raw = await fs.readFile(eventsPath, 'utf8'); }
    catch (error) { if (error.code === 'ENOENT') return []; throw error; }
    const lines = raw.split('\n');
    lines.pop(); // Only consume complete JSONL records.
    const events = lines.slice(consumed).map(line => JSON.parse(line));
    consumed = lines.length;
    if (!events.length) return [];
    const windows = await sky.list_windows();
    const candidates = windows.filter(window => /steam(?:\.exe|webhelper\.exe)?/i.test(window.app));
    const main = candidates.filter(window => window.title === 'Steam');
    const selected = main.length === 1 ? main[0] : candidates.length === 1 ? candidates[0] : null;
    let capture = null;
    if (selected) {
      try {
        const window = await sky.get_window({ id: selected.id, app: selected.app });
        await sky.activate_window({ window });
        const state = await sky.get_window_state({ window, include_screenshot: true, include_text: false });
        const capturedAt = new Date().toISOString();
        const screenshot = state.screenshots?.[0];
        if (screenshot) {
          const match = /^data:image\/(png|jpeg);base64,(.+)$/s.exec(screenshot.url);
          if (!match) throw new Error('Unsupported screenshot format');
          screenshotNumber += 1;
          const image = `screenshot-${String(screenshotNumber).padStart(2, '0')}.${match[1] === 'jpeg' ? 'jpg' : 'png'}`;
          await fs.writeFile(path.join(directory, image), Buffer.from(match[2], 'base64'));
          capture = { image, screenshotNumber, capturedAt, window: state.window };
        }
      } catch (error) {
        capture = { note: `Capture unavailable: ${error.message}` };
      }
    }
    const added = events.map(event => ({ event, ...(capture ?? { note: candidates.length > 1 ? 'Multiple Steam windows; no unique capture target.' : 'No capturable Steam window at this signal.' }) }));
    rows.push(...added);
    await saveReport();
    return added;
  }

  await saveReport();
  return {
    poll, rows, eventsPath, reportPath: path.join(directory, 'index.html'),
    switchEvents(filename) { eventsPath = path.join(directory, filename); consumed = 0; },
  };
}

export async function writeReviewGallery(directory) {
  const rows = JSON.parse(await fs.readFile(path.join(directory, 'captures.json'), 'utf8'));
  const start = rows.find(row => row.event.event === 'probe_started')?.event.details.started_unix_ms;
  const launch = rows.find(row => row.event.event === 'steam_launch_observed')?.event.elapsed_ms ?? 0;
  const seconds = ms => `${(ms / 1000).toFixed(2)} s`;
  const screenshots = [...new Set(rows.map(row => row.image).filter(Boolean))];
  const summary = rows.findLast(row => row.event.event === 'probe_summary');
  const html = `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width"><title>Choose when Steam is loaded</title><style>
    body{font:16px system-ui;background:#17191d;color:#eee;max-width:1200px;margin:32px auto;padding:0 20px}article{background:#25282e;padding:20px;margin:24px 0;border-radius:8px}img{width:100%;height:auto;display:block}a{color:#b2d4ff}p{line-height:1.5}.signals{color:#bfc9d5}table{border-collapse:collapse;width:100%}td,th{text-align:left;border-bottom:1px solid #454951;padding:10px}details{margin-top:12px}pre{white-space:pre-wrap}nav a{margin-right:16px}
    </style></head><body><h1>When should Steam count as loaded?</h1><p>Choose a screenshot number. This is the clean startup after Steam finished updating.</p><p><strong>Found:</strong> Steam's main window is <code>SDL_app</code>. The application's current <code>vguiPopupWindow</code> check never triggered.</p><p>Signals and screenshots have separate timestamps. A screenshot shows what was visible when captured, not necessarily the exact trigger instant. Closely spaced signals share a capture. The initial capture was delayed by Steam's launch API waiting for a window.</p><nav>${screenshots.map((_, i) => `<a href="#shot-${i + 1}">#${i + 1}</a>`).join('')}</nav>
    ${screenshots.map((image, i) => {
      const group = rows.filter(row => row.image === image);
      const row = group[0];
      const capturedMs = Date.parse(row.capturedAt) - start - launch;
      return `<article id="shot-${i + 1}"><h2>Screenshot ${i + 1} · ${seconds(capturedMs)} after launch detection</h2><p class="signals">${group.map(item => `${escapeHtml(item.event.signal)} @ ${seconds(item.event.elapsed_ms - launch)}`).join('<br>')}</p><a href="${escapeHtml(image)}" target="_blank"><img src="${escapeHtml(image)}" alt="Steam startup screenshot ${i + 1}"></a><details><summary>Recorded signal details</summary><pre>${escapeHtml(JSON.stringify(group, null, 2))}</pre></details></article>`;
    }).join('')}
    <h2>Signals that never triggered</h2><pre>${escapeHtml(JSON.stringify(summary?.event.details.untriggered_candidates ?? [], null, 2))}</pre><p><a href="events.jsonl">Raw native event log</a> · <a href="captures.json">Capture timestamps</a></p></body></html>`;
  const result = path.join(directory, 'review.html');
  await fs.writeFile(result, html);
  return result;
}
