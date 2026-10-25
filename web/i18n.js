'use strict';
(() => {
  const messages = {
    zh: {
      'page.homeTitle': 'Lanprint 打印',
      'page.settingsTitle': 'Lanprint 设置',
      'language.label': '界面语言',
      'nav.aria': '主导航',
      'nav.home': '首页',
      'nav.settings': '设置',
      'home.title': 'Lanprint 打印',
      'settings.title': 'Lanprint 设置',
      'upload.title': '上传打印',
      'upload.file': '选择 PDF、DOC、DOCX、PNG、JPG 或 JPEG',
      'upload.submit': '上传并打印',
      'upload.help': '支持 PDF、DOC、DOCX、PNG、JPG、JPEG，单个文件最多 50 MiB。上传后自动排队打印，目录监控暂停时也会执行。',
      'upload.helpConfigured': '支持 PDF、DOC、DOCX、PNG、JPG、JPEG，单个文件最多 {limit} MiB。上传任务自动排队，目录监控暂停时也会执行。当前活动打印队列：{printer}。{restart}已有任务使用入队时的原配置。',
      'upload.helpRestart': '已保存新配置，需要重启后生效；重启前继续使用当前活动配置。',
      'upload.helpUnconfigured': '请先前往“设置”保存目录与打印机配置。',
      'upload.helpNeedsRestart': '配置已保存，需要重启服务后才能上传。',
      'upload.result': '任务 #{id} · {name} · {status}{error}',
      'history.title': '打印历史',
      'history.help': '“已提交”表示 CUPS 已接收，实际打印结果请查看打印机或 CUPS 队列。重试使用原文件快照及入队时的打印配置。',
      'history.file': '文件',
      'history.created': '创建时间',
      'history.status': '状态 / 次数',
      'history.pdfPages': 'PDF 页数',
      'history.result': '结果',
      'history.actions': '操作',
      'history.empty': '暂无记录',
      'history.waiting': '等待处理',
      'history.unknown': '未知',
      'history.notRecorded': '未记录',
      'history.retry': '重试',
      'history.retryConfirm': '请先检查 CUPS 队列。失败或超时的任务可能已被接收，重试可能重复打印。确认使用原快照和原打印配置重试？',
      'chart.title': '最近 7 天 · 已提交至 CUPS',
      'chart.help': '服务本地自然日：今天及之前 6 天。页数为 PDF 文档页数，不代表物理出纸张数。',
      'chart.files': '文件数',
      'chart.pages': '页数',
      'chart.loading': '正在读取统计…',
      'chart.details': '查看每日统计文本',
      'chart.submittedFiles': '已提交至 CUPS {count} 个文件',
      'chart.none': '暂无已提交至 CUPS 的记录',
      'chart.incompletePages': '页数不完整：已知 {known} 页，{unknown} 个文件页数未知',
      'chart.totalPages': '共 {count} 页',
      'chart.pageCount': '{count} 页',
      'chart.atLeast': '至少 {count} 页，',
      'chart.unknownFiles': '{count} 个文件页数未知（不完整）',
      'chart.daySummary': '{date}：已提交至 CUPS {files} 个文件，{pages}',
      'chart.rangeSummary': '{start} 至 {end}（服务本地日期）：{files}；{pages}。',
      'chart.aria': '最近 7 天已提交至 CUPS 的文件数与页数柱状图。{days}。',
      'chart.unavailable': '最近 7 天已提交至 CUPS 统计暂不可用',
      'chart.failed': '统计暂不可用：{error}',
      'pagination.previous': '上一页',
      'pagination.next': '下一页',
      'pagination.page': '第 {number} 页',
      'common.refresh': '刷新',
      'common.connecting': '正在连接…',
      'common.noJobs': '暂无打印任务',
      'common.networkError': '无法连接 Lanprint 服务。请确认服务正在运行后重试。',
      'job.queued': '排队中',
      'job.converting': '转换中',
      'job.submitting': '提交中',
      'job.submitted': '已提交 CUPS',
      'job.failed': '失败',
      'job.cancelled': '配置变更已取消',
      'notice.uploading': '正在上传…',
      'notice.uploadAccepted': '任务 #{id} 已排队，将自动打印。',
      'notice.uploadRetry': '上传任务已重新入队，将自动处理。',
      'notice.watchRetry': '已重新入队；目录任务将在设置页启动监控后处理。',
      'notice.monitorStarted': '监控已启动。',
      'notice.monitorPaused': '已暂停；等待当前扫描或打印结束后，可修改配置或重新启动。',
      'notice.configNeedsRestart': '配置已保存，需要重启服务后生效。排队任务保留原配置。',
      'notice.configUnchanged': '配置已保存，与当前活动配置一致，无需重启。',
      'error.fileType': '仅支持 PDF、DOC、DOCX、PNG、JPG 和 JPEG 文件。',
      'error.emptyFile': '不能打印空文件。',
      'error.fileSize': '文件超过 50 MiB 上限。',
      'error.unsupported': '仅支持 PDF、DOC、DOCX、PNG、JPG 和 JPEG 文件。',
      'error.fileTooLarge': '文件超过 50 MiB 上限。',
      'error.emptyUpload': '不能打印空文件。',
      'error.saveConfig': '请先在设置页保存打印配置。',
      'error.pauseFirst': '修改配置前请先暂停监控。',
      'error.waitIdle': '请等待当前扫描或打印任务结束后再修改配置。',
      'error.waitIdleStart': '请等待当前扫描或打印任务结束后再启动监控。',
      'error.saveBeforeStart': '请先保存配置。',
      'error.invalidFileName': '上传文件名无效。',
      'error.oneFileRequired': '请选择一个文件。',
      'error.fileField': '上传请求中的文件字段无效。',
      'error.missingFileName': '上传文件缺少文件名。',
      'error.oneFileOnly': '每次请求只能上传一个文件。',
      'error.retryCheckQueue': '请先检查 CUPS 队列，再使用确认过队列的重试请求。',
      'error.invalidHost': '请求的主机名或端口无效。',
      'error.missingRequestHeader': '请求缺少安全校验标记。',
      'error.crossOrigin': '已拒绝跨来源请求。',
      'error.printerMissing': 'CUPS 中不存在配置的打印机队列。',
      'history.jobStatus': '{status} / 尝试 {attempts} 次',
      'settings.statusTitle': '运行状态',
      'settings.statusHelp': '暂停后正在执行的任务会继续，目录排队任务等待再次启动；上传任务自动处理。服务重启后目录监控默认暂停。已有任务及人工重试始终使用入队时的原配置，重启也不会改写目标。',
      'settings.configTitle': '目录与打印配置',
      'settings.watchDir': '监控目录（绝对路径）',
      'settings.watchDirPlaceholder': '/home/user/print-inbox',
      'settings.printer': 'CUPS 打印队列',
      'settings.printerPlaceholder': 'Office',
      'settings.cupsServer': 'CUPS 服务端（留空使用系统默认）',
      'settings.cupsServerPlaceholder': 'print-server.local:631',
      'settings.lpPath': 'lp 可执行文件',
      'settings.libreOfficePath': 'LibreOffice 可执行文件',
      'settings.stableSeconds': '稳定等待（秒，2–300）',
      'settings.commandTimeout': '单个命令超时（秒，1–600）',
      'settings.save': '保存配置',
      'settings.configHelp': '此表单显示已保存配置；更改后需由管理员重启服务才生效。首次保存同样需要重启。修改前请暂停，并等待扫描或打印结束；保存不会取消或改写排队任务。启动监控会处理活动目录中已有的文件；仅扫描当前层的 PDF、DOCX、DOC、PNG、JPG、JPEG。',
      'settings.running': '监控运行中',
      'settings.paused': '监控已暂停',
      'settings.scanning': '正在扫描目录或创建快照',
      'settings.processing': '正在处理任务 #{id}',
      'settings.scanError': '目录检查失败：{error}',
      'settings.lastScan': '最近扫描：{time}',
      'settings.neverScanned': '最近扫描：尚未扫描',
      'settings.counts': '{items}',
      'settings.noCounts': '暂无打印任务',
      'settings.activeConfig': '当前活动打印队列：{printer}；监控目录：{path}',
      'settings.noActiveConfig': '当前进程尚无活动配置。请保存配置并重启服务。',
      'settings.restartActive': '配置已保存，需要重启服务后生效。重启前启动监控和新上传继续使用上方活动配置。',
      'settings.restartNoActive': '配置已保存，需要重启服务后生效。重启前无法启动监控或上传。',
      'settings.restartNotNeeded': '已保存配置与活动配置一致，无需重启。',
      'settings.notConfigured': '尚未保存配置。',
      'settings.start': '启动监控',
      'settings.pause': '暂停监控',
      'settings.configNeedsRestart': '配置已保存，需要重启服务后生效。排队任务保留原配置。',
      'settings.configSame': '配置已保存，与当前活动配置一致，无需重启。',
      'settings.started': '监控已启动。',
      'settings.pausedNotice': '已暂停；等待当前扫描或打印结束后，可修改配置或重新启动。'
    },
    en: {
      'page.homeTitle': 'Lanprint Print Console',
      'page.settingsTitle': 'Lanprint Settings',
      'language.label': 'Language',
      'nav.aria': 'Main navigation',
      'nav.home': 'Home',
      'nav.settings': 'Settings',
      'home.title': 'Lanprint Print Console',
      'settings.title': 'Lanprint Settings',
      'upload.title': 'Upload and print',
      'upload.file': 'Choose a PDF, DOC, DOCX, PNG, JPG, or JPEG file',
      'upload.submit': 'Upload and print',
      'upload.help': 'PDF, DOC, DOCX, PNG, JPG, and JPEG files up to 50 MiB are accepted. Uploads enter the print queue automatically, even when folder monitoring is paused.',
      'upload.helpConfigured': 'PDF, DOC, DOCX, PNG, JPG, and JPEG files up to {limit} MiB are accepted. Uploads enter the queue automatically, even when monitoring is paused. Active printer queue: {printer}. {restart}Existing jobs keep the configuration saved with each job.',
      'upload.helpRestart': 'New settings are saved and will apply after a service restart. The active settings remain in use until then. ',
      'upload.helpUnconfigured': 'Save a watch folder and printer in Settings first.',
      'upload.helpNeedsRestart': 'Settings are saved. Restart the service before you upload a file.',
      'upload.result': 'Job #{id} · {name} · {status}{error}',
      'history.title': 'Print history',
      'history.help': 'Submitted means that CUPS accepted the job. Check the printer or CUPS queue for the final print result. A retry uses the original file snapshot and print settings.',
      'history.file': 'File',
      'history.created': 'Created',
      'history.status': 'Status / attempts',
      'history.pdfPages': 'PDF pages',
      'history.result': 'Result',
      'history.actions': 'Actions',
      'history.empty': 'No print history yet.',
      'history.waiting': 'Waiting for processing',
      'history.unknown': 'Unknown',
      'history.notRecorded': 'Not recorded',
      'history.retry': 'Retry',
      'history.retryConfirm': 'Check the CUPS queue first. A failed or timed out job may have been accepted, so a retry can print it twice. Retry with the original snapshot and print settings?',
      'chart.title': 'Last 7 days · Submitted to CUPS',
      'chart.help': 'Uses the service local calendar: today and the previous 6 days. Page counts are PDF document pages, not physical sheets.',
      'chart.files': 'Files',
      'chart.pages': 'Pages',
      'chart.loading': 'Loading statistics…',
      'chart.details': 'View daily values as text',
      'chart.submittedFiles': '{count} files submitted to CUPS',
      'chart.none': 'No files submitted to CUPS',
      'chart.incompletePages': 'Page total is incomplete: {known} known pages; {unknown} files with unknown page counts',
      'chart.totalPages': '{count} pages total',
      'chart.pageCount': '{count} pages',
      'chart.atLeast': 'At least {count} pages, ',
      'chart.unknownFiles': '{count} files with unknown page counts (incomplete)',
      'chart.daySummary': '{date}: {files} files submitted to CUPS, {pages}',
      'chart.rangeSummary': '{start} to {end} (service local date): {files}; {pages}.',
      'chart.aria': 'Bar chart of files and pages submitted to CUPS during the last 7 days. {days}.',
      'chart.unavailable': 'Statistics for the last 7 days are not available',
      'chart.failed': 'Statistics are not available: {error}',
      'pagination.previous': 'Previous page',
      'pagination.next': 'Next page',
      'pagination.page': 'Page {number}',
      'common.refresh': 'Refresh',
      'common.connecting': 'Connecting…',
      'common.noJobs': 'No print jobs',
      'common.networkError': 'Cannot connect to Lanprint. Check that the service is running and try again.',
      'job.queued': 'Queued',
      'job.converting': 'Converting',
      'job.submitting': 'Submitting',
      'job.submitted': 'Submitted to CUPS',
      'job.failed': 'Failed',
      'job.cancelled': 'Cancelled after settings changed',
      'notice.uploading': 'Uploading…',
      'notice.uploadAccepted': 'Job #{id} is queued and will print automatically.',
      'notice.uploadRetry': 'The uploaded job is queued and will print automatically.',
      'notice.watchRetry': 'The job is queued. Start folder monitoring in Settings to process it.',
      'notice.monitorStarted': 'Folder monitoring started.',
      'notice.monitorPaused': 'Monitoring is paused. Wait for the current scan or print job to finish before changing settings or starting again.',
      'notice.configNeedsRestart': 'Settings are saved and will apply after a service restart. Queued jobs keep their original settings.',
      'notice.configUnchanged': 'Settings are saved and match the active settings. No restart is needed.',
      'error.fileType': 'Only PDF, DOC, DOCX, PNG, JPG, and JPEG files are supported.',
      'error.emptyFile': 'An empty file cannot be printed.',
      'error.fileSize': 'The file exceeds the 50 MiB limit.',
      'error.unsupported': 'Only PDF, DOC, DOCX, PNG, JPG, and JPEG files are supported.',
      'error.fileTooLarge': 'The file exceeds the 50 MiB upload limit.',
      'error.emptyUpload': 'An empty file cannot be printed.',
      'error.saveConfig': 'Save printer settings in Settings first.',
      'error.pauseFirst': 'Pause monitoring before you change settings.',
      'error.waitIdle': 'Wait for the current scan or print job to finish before you change settings.',
      'error.waitIdleStart': 'Wait for the current scan or print job to finish before starting monitoring.',
      'error.saveBeforeStart': 'Save settings first.',
      'error.invalidFileName': 'The upload file name is invalid.',
      'error.oneFileRequired': 'Choose one file.',
      'error.fileField': 'The upload request has an invalid file field.',
      'error.missingFileName': 'The uploaded file has no name.',
      'error.oneFileOnly': 'Upload one file per request.',
      'error.retryCheckQueue': 'Check the CUPS queue before retrying with queue confirmation.',
      'error.invalidHost': 'The request host or port is not allowed.',
      'error.missingRequestHeader': 'The request is missing its security header.',
      'error.crossOrigin': 'Cross-origin requests are not allowed.',
      'error.printerMissing': 'The configured printer queue does not exist in CUPS.',
      'history.jobStatus': '{status} · Attempts: {attempts}',
      'settings.statusTitle': 'Service status',
      'settings.statusHelp': 'Pausing stops new folder scans. Current work continues and queued folder jobs wait until monitoring starts again. Upload jobs continue automatically. Monitoring is paused after a service restart. Existing jobs and retries always use the settings saved with each job.',
      'settings.configTitle': 'Folder and printer settings',
      'settings.watchDir': 'Watch folder (absolute path)',
      'settings.watchDirPlaceholder': '/home/user/print-inbox',
      'settings.printer': 'CUPS printer queue',
      'settings.printerPlaceholder': 'Office',
      'settings.cupsServer': 'CUPS server (leave blank for the system default)',
      'settings.cupsServerPlaceholder': 'print-server.local:631',
      'settings.lpPath': 'Path to lp',
      'settings.libreOfficePath': 'Path to LibreOffice',
      'settings.stableSeconds': 'File stability wait (seconds, 2–300)',
      'settings.commandTimeout': 'Command timeout (seconds, 1–600)',
      'settings.save': 'Save settings',
      'settings.configHelp': 'This form shows saved settings. A service administrator must restart Lanprint to apply changes, including the first saved settings. Pause monitoring and wait for scans and print jobs to finish first. Saving does not cancel or change queued jobs. Starting monitoring scans existing files in the active folder. Only top-level PDF, DOCX, DOC, PNG, JPG, and JPEG files are scanned.',
      'settings.running': 'Monitoring is running',
      'settings.paused': 'Monitoring is paused',
      'settings.scanning': 'Scanning the folder or creating a file snapshot',
      'settings.processing': 'Processing job #{id}',
      'settings.scanError': 'Folder scan failed: {error}',
      'settings.lastScan': 'Last scan: {time}',
      'settings.neverScanned': 'Last scan: not scanned yet',
      'settings.counts': '{items}',
      'settings.noCounts': 'No print jobs',
      'settings.activeConfig': 'Active printer queue: {printer}; watch folder: {path}',
      'settings.noActiveConfig': 'This process has no active settings. Save settings and restart the service.',
      'settings.restartActive': 'Saved settings need a service restart. Before restart, monitoring and uploads continue to use the active settings above.',
      'settings.restartNoActive': 'Saved settings need a service restart. Monitoring and uploads are not available until restart.',
      'settings.restartNotNeeded': 'Saved settings match the active settings. No restart is needed.',
      'settings.notConfigured': 'No settings have been saved.',
      'settings.start': 'Start monitoring',
      'settings.pause': 'Pause monitoring',
      'settings.configNeedsRestart': 'Settings are saved and will apply after a service restart. Queued jobs keep their original settings.',
      'settings.configSame': 'Settings are saved and match the active settings. No restart is needed.',
      'settings.started': 'Folder monitoring started.',
      'settings.pausedNotice': 'Monitoring is paused. Wait for the current scan or print job to finish before changing settings or starting again.'
    }
  };

  function readLanguage() {
    try {
      const saved = localStorage.getItem('lanprint-language');
      if (saved === 'zh' || saved === 'en') return saved;
    } catch (_) { /* Storage may be disabled by the browser. */ }
    return navigator.language && navigator.language.toLowerCase().startsWith('en') ? 'en' : 'zh';
  }

  let language = readLanguage();

  function t(key, values = {}) {
    const template = messages[language][key] || messages.zh[key] || key;
    return template.replace(/\{([a-zA-Z0-9_]+)\}/g, (_, name) => String(values[name] ?? ''));
  }

  function translateError(message) {
    const direct = {
      'Only PDF, DOC, DOCX, PNG, JPG and JPEG files are supported': 'error.unsupported',
      'File exceeds the 50 MiB upload limit': 'error.fileTooLarge',
      'Empty file uploads are not allowed': 'error.emptyUpload',
      'Save a printer configuration before uploading': 'error.saveConfig',
      'Save a printer configuration and restart the service before uploading': 'error.saveConfig',
      'Pause monitoring before changing configuration': 'error.pauseFirst',
      'Save configuration first': 'error.saveBeforeStart',
      'Save configuration and restart the service first': 'error.saveBeforeStart',
      'Invalid upload file name': 'error.invalidFileName',
      'One file field is required': 'error.oneFileRequired',
      'Expected one multipart field named file': 'error.fileField',
      'Missing file name': 'error.missingFileName',
      'Empty files cannot be printed': 'error.emptyUpload',
      'Upload exactly one file per request': 'error.oneFileOnly',
      'Check the CUPS queue first; retry can duplicate an already accepted job. Use confirm=checked_queue': 'error.retryCheckQueue',
      'Invalid Host header': 'error.invalidHost',
      'Missing X-Lanprint-Request header': 'error.missingRequestHeader',
      'Cross-origin request denied': 'error.crossOrigin'
    };
    if (!message || message === 'Failed to fetch') return t('common.networkError');
    if (direct[message]) return t(direct[message]);
    if (message.includes('Wait for the active scan or print job to finish before changing configuration')) return t('error.waitIdle');
    if (message.includes('Wait for the active scan or print job to finish before starting monitoring')) return t('error.waitIdleStart');
    if (message.includes('The printer or class does not exist')) return `${t('error.printerMissing')}\n${message}`;
    return message;
  }

  function applyStaticText() {
    document.documentElement.lang = language === 'en' ? 'en' : 'zh-CN';
    document.querySelectorAll('[data-i18n]').forEach((element) => {
      element.textContent = t(element.dataset.i18n);
    });
    document.querySelectorAll('[data-i18n-aria-label]').forEach((element) => {
      element.setAttribute('aria-label', t(element.dataset.i18nAriaLabel));
    });
    document.querySelectorAll('[data-i18n-placeholder]').forEach((element) => {
      element.setAttribute('placeholder', t(element.dataset.i18nPlaceholder));
    });
    const titleKey = document.body.dataset.titleKey;
    if (titleKey) document.title = t(titleKey);
    const selector = document.getElementById('language');
    if (selector) selector.value = language;
  }

  const selector = document.getElementById('language');
  if (selector) {
    selector.value = language;
    selector.addEventListener('change', () => {
      language = selector.value === 'en' ? 'en' : 'zh';
      try { localStorage.setItem('lanprint-language', language); } catch (_) { /* Keep the current page language. */ }
      applyStaticText();
      document.dispatchEvent(new CustomEvent('lanprint-languagechange'));
    });
  }

  applyStaticText();
  window.LanprintI18n = {
    t,
    error: translateError,
    get language() { return language; },
    get dateLocale() { return language === 'en' ? 'en-US' : 'zh-CN'; }
  };
})();
