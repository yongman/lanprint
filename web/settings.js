'use strict';
const $ = (id) => document.getElementById(id);
const i18n = window.LanprintI18n;
const t = i18n.t;
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
async function refresh() {
  const status = await api('/api/status');
  $('status').textContent = `${t(status.monitoring ? 'settings.running' : 'settings.paused')}${status.busy ? ` · ${status.runtime.active_job === null ? t('settings.scanning') : t('settings.processing', {id: status.runtime.active_job})}` : ''}`;
  $('scan').textContent = status.runtime.scan_error ? t('settings.scanError', {error: i18n.error(status.runtime.scan_error)}) : (status.runtime.last_scan_at ? t('settings.lastScan', {time: new Date(status.runtime.last_scan_at).toLocaleString(i18n.dateLocale)}) : t('settings.neverScanned'));
  const countLabels = Object.entries(status.counts).map(([key, value]) => `${t(`job.${key}`)} ${value}`);
  $('counts').textContent = status.configured ? `${t('settings.activeConfig', {printer: status.active_config.printer, path: status.active_config.watch_dir})}${countLabels.length ? ` · ${countLabels.join(i18n.language === 'en' ? ' · ' : ' · ')}` : ''}` : t('settings.noActiveConfig');
  $('restart').textContent = status.restart_required ? t(status.configured ? 'settings.restartActive' : 'settings.restartNoActive') : t(status.configured ? 'settings.restartNotNeeded' : 'settings.notConfigured');
  $('start').disabled = !status.configured || status.monitoring || status.busy;
  $('pause').disabled = !status.monitoring;
  $('config').querySelectorAll('input, button').forEach((field) => { field.disabled = status.monitoring || status.busy; });
}
$('config').onsubmit = (event) => { event.preventDefault(); action(async () => {
  const data = Object.fromEntries(new FormData($('config')));
  data.stable_seconds = Number(data.stable_seconds); data.command_timeout_seconds = Number(data.command_timeout_seconds);
  const saved = await api('/api/config', 'PUT', data);
  $('config').elements.watch_dir.value = saved.config.watch_dir;
  notice(t(saved.restart_required ? 'settings.configNeedsRestart' : 'settings.configSame')); await refresh();
}); };
$('start').onclick = () => action(async () => { await api('/api/monitor/start', 'POST'); notice(t('settings.started')); await refresh(); });
$('pause').onclick = () => action(async () => { await api('/api/monitor/pause', 'POST'); notice(t('settings.pausedNotice')); await refresh(); });
action(async () => {
  const { config } = await api('/api/config');
  if (config) for (const [key, value] of Object.entries(config)) $('config').elements[key].value = value;
  await refresh();
});
setInterval(() => action(refresh), 3000);
document.addEventListener('lanprint-languagechange', () => action(refresh));
