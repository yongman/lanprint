'use strict';
const $ = (id) => document.getElementById(id);
const i18n = window.LanprintI18n;
const t = i18n.t;
let offset = 0;
const limit = 50;
let uploading = false;
let uploadedJob = null;
let maxUploadBytes = 50 * 1024 * 1024;
const jobName = (status) => t(`job.${status}`);
async function api(path, method = 'GET', body) {
  const multipart = body instanceof FormData;
  const headers = { 'X-Lanprint-Request': '1' };
  if (!multipart) headers['Content-Type'] = 'application/json';
  const response = await fetch(path, { method, headers, body: body === undefined ? undefined : multipart ? body : JSON.stringify(body) });
  const text = await response.text();
  let data;
  try { data = JSON.parse(text); } catch { throw new Error(text || `HTTP ${response.status}`); }
  if (!response.ok) throw new Error(i18n.error(data.error || `HTTP ${response.status}`));
  return data;
}
function notice(message, error = false) { $('notice').textContent = message; $('notice').className = error ? 'error' : 'success'; }
async function action(work) { try { await work(); } catch (error) { notice(i18n.error(error.message), true); } }
function pageLabel(day) {
  if (day.pages !== null) return t('chart.pageCount', {count: day.pages});
  return `${day.known_pages ? t('chart.atLeast', {count: day.known_pages}) : ''}${t('chart.unknownFiles', {count: day.unknown_page_files})}`;
}
async function refreshStats() {
  try {
    const stats = await api('/api/stats/weekly');
    const days = stats.days;
    const files = days.reduce((sum, day) => sum + day.submitted_files, 0);
    const known = days.reduce((sum, day) => sum + day.known_pages, 0);
    const unknown = days.reduce((sum, day) => sum + day.unknown_page_files, 0);
    const dateLabel = (value) => new Date(`${value}T00:00:00`).toLocaleDateString(i18n.dateLocale, {month: '2-digit', day: '2-digit'});
    const filesLabel = files ? t('chart.submittedFiles', {count: files}) : t('chart.none');
    const pagesLabel = unknown ? t('chart.incompletePages', {known, unknown}) : t('chart.totalPages', {count: known});
    $('weekly-summary').textContent = t('chart.rangeSummary', {start: dateLabel(days[0].date), end: dateLabel(days[6].date), files: filesLabel, pages: pagesLabel});
    const summaries = days.map((day) => t('chart.daySummary', {date: day.date, files: day.submitted_files, pages: pageLabel(day)}));
    $('weekly-chart').setAttribute('aria-label', t('chart.aria', {days: summaries.join(languageJoin())}));
    $('weekly-chart').replaceChildren();
    $('weekly-text').replaceChildren();
    const maximum = Math.max(1, ...days.flatMap((day) => [day.submitted_files, day.known_pages]));
    const svgNode = (name, attrs) => {
      const node = document.createElementNS('http://www.w3.org/2000/svg', name);
      for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, value);
      return node;
    };
    for (const [index, day] of days.entries()) {
      const group = document.createElement('div'); group.className = 'stat-day';
      const svg = svgNode('svg', {viewBox: '0 0 88 150', 'aria-hidden': 'true'});
      svg.append(svgNode('line', {x1: 4, y1: 130, x2: 84, y2: 130, class: 'chart-baseline'}));
      for (const [x, value, className, label] of [
        [17, day.submitted_files, 'bar-files', String(day.submitted_files)],
        [55, day.known_pages, `bar-pages${day.pages === null ? ' incomplete' : ''}`, day.pages === null ? (day.known_pages ? `≥${day.known_pages}` : '?') : String(day.pages)]
      ]) {
        const height = 102 * value / maximum;
        svg.append(svgNode('rect', {x, y: 130 - height, width: 20, height, class: className}));
        const text = svgNode('text', {x: x + 10, y: 122 - height, 'text-anchor': 'middle'});
        text.textContent = label; svg.append(text);
      }
      const date = document.createElement('span'); date.textContent = dateLabel(day.date);
      group.append(svg, date); $('weekly-chart').append(group);
      const item = document.createElement('li'); item.textContent = summaries[index]; $('weekly-text').append(item);
    }
  } catch (error) {
    $('weekly-chart').replaceChildren(); $('weekly-text').replaceChildren();
    $('weekly-chart').setAttribute('aria-label', t('chart.unavailable'));
    $('weekly-summary').textContent = t('chart.failed', {error: i18n.error(error.message)});
  }
}
function languageJoin() { return i18n.language === 'en' ? '; ' : '；'; }
async function refresh() {
  const [status, history] = await Promise.all([api('/api/status'), api(`/api/jobs?limit=${limit}&offset=${offset}`)]);
  maxUploadBytes = status.max_upload_bytes;
  $('upload-submit').disabled = uploading || !status.configured;
  $('upload-help').textContent = status.configured
    ? t('upload.helpConfigured', {limit: 50, printer: status.active_config.printer, restart: status.restart_required ? t('upload.helpRestart') : ''})
    : (status.restart_required ? t('upload.helpNeedsRestart') : t('upload.helpUnconfigured'));
  if (uploadedJob) {
    uploadedJob = history.jobs.find((job) => job.id === uploadedJob.id) || uploadedJob;
    $('upload-result').textContent = t('upload.result', {id: uploadedJob.id, name: uploadedJob.file_name, status: jobName(uploadedJob.status), error: uploadedJob.error ? `: ${i18n.error(uploadedJob.error)}` : ''});
    $('upload-result').className = uploadedJob.status === 'failed' ? 'error' : 'success';
  }
  $('jobs').replaceChildren();
  for (const job of history.jobs) {
    const row = document.createElement('tr');
    row.dataset.jobId = job.id;
    const labels = ['history.file', 'history.created', 'history.status', 'history.pdfPages', 'history.result'];
    for (const text of [job.file_name, new Date(job.created_at).toLocaleString(i18n.dateLocale), t('history.jobStatus', {status: jobName(job.status), attempts: job.attempts}), job.status === 'submitted' ? (job.page_count === null ? t('history.unknown') : t('chart.pageCount', {count: job.page_count})) : t('history.notRecorded'), i18n.error(job.error || job.cups_response || t('history.waiting'))]) {
      const cell = document.createElement('td'); cell.dataset.label = t(labels[row.children.length]); cell.textContent = text; row.append(cell);
    }
    row.firstChild.title = job.source_path;
    const cell = document.createElement('td'); cell.dataset.label = t('history.actions');
    if (job.status === 'failed') {
      const button = document.createElement('button'); button.textContent = t('history.retry'); button.className = 'secondary';
      button.onclick = () => action(async () => {
        if (!confirm(t('history.retryConfirm'))) return;
        await api(`/api/jobs/${job.id}/retry?confirm=checked_queue`, 'POST'); notice(t(job.origin === 'upload' ? 'notice.uploadRetry' : 'notice.watchRetry')); await refresh();
      });
      cell.append(button);
    }
    row.append(cell); $('jobs').append(row);
  }
  if (!history.jobs.length) { const row = document.createElement('tr'); const cell = document.createElement('td'); cell.colSpan = 6; cell.textContent = t('history.empty'); row.append(cell); $('jobs').append(row); }
  $('page').textContent = t('pagination.page', {number: offset / limit + 1});
  $('previous').disabled = offset === 0; $('next').disabled = history.jobs.length < limit;
  await refreshStats();
}
$('upload').onsubmit = async (event) => {
  event.preventDefault();
  if (uploading) return;
  const file = $('upload-file').files[0];
  if (!file) return;
  try {
    if (!/\.(pdf|doc|docx)$/i.test(file.name)) throw new Error(t('error.fileType'));
    if (!file.size) throw new Error(t('error.emptyFile'));
    if (file.size > maxUploadBytes) throw new Error(t('error.fileSize'));
    uploading = true; uploadedJob = null; $('upload-submit').disabled = true;
    $('upload-result').className = ''; $('upload-result').textContent = t('notice.uploading');
    const { job } = await api('/api/uploads', 'POST', new FormData($('upload')));
    uploadedJob = job; offset = 0;
    $('upload').reset();
    notice(t('notice.uploadAccepted', {id: job.id}));
    await refresh();
  } catch (error) {
    $('upload-result').textContent = i18n.error(error.message); $('upload-result').className = 'error';
  } finally {
    uploading = false;
    await refresh().catch(() => {});
  }
};
$('refresh').onclick = () => action(refresh);
$('previous').onclick = () => { offset = Math.max(0, offset - limit); action(refresh); };
$('next').onclick = () => { offset += limit; action(refresh); };
action(refresh);
setInterval(() => action(refresh), 3000);
document.addEventListener('lanprint-languagechange', () => action(refresh));
