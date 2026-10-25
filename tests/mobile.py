"""Supervised local browser verification. Needs Python Playwright and Chromium."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

from playwright.sync_api import sync_playwright, expect
from pdf_fixture import make_pdf

binary = str(Path(sys.argv[1]).resolve())
evidence = Path(sys.argv[2]).resolve()
evidence.mkdir(parents=True, exist_ok=True)
console_errors, network_errors, layouts = [], [], []

with tempfile.TemporaryDirectory(prefix='lanprint-mobile-') as root:
    root = Path(root)
    inbox = root / 'inbox'
    inbox.mkdir()
    hold, reject = root / 'hold', root / 'reject'
    lp, lo = root / 'lp', root / 'lo'
    lp.write_text(f'''#!/usr/bin/python3
import pathlib, sys, time
while pathlib.Path({str(hold)!r}).exists(): time.sleep(0.05)
if pathlib.Path({str(reject)!r}).exists():
    print('mobile simulated CUPS failure', file=sys.stderr)
    sys.exit(1)
print('request id is Mobile-1')
''')
    lo.write_text(f'''#!/usr/bin/python3
import pathlib, sys
source = pathlib.Path(sys.argv[-1])
out = pathlib.Path(sys.argv[sys.argv.index('--outdir') + 1])
(out / (source.stem + '.pdf')).write_bytes({make_pdf(3)!r})
''')
    lp.chmod(0o700)
    lo.chmod(0o700)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    base = f'http://127.0.0.1:{port}'
    with open(evidence / 'service.log', 'w') as log:
        process = subprocess.Popen([binary, '--data-dir', str(root / 'data'), '--bind', f'127.0.0.1:{port}'], stdout=log, stderr=log)
        def restart():
            global process
            process.terminate()
            process.wait(timeout=15)
            assert process.returncode == 0
            process = subprocess.Popen([binary, '--data-dir', str(root / 'data'), '--bind', f'127.0.0.1:{port}'], stdout=log, stderr=log)
            for attempt in range(50):
                assert process.poll() is None, 'service exited after restart'
                try:
                    urllib.request.urlopen(base + '/api/status', timeout=1).close()
                    return
                except OSError:
                    time.sleep(0.1)
            raise AssertionError('server did not restart')

        try:
            for attempt in range(50):
                assert process.poll() is None, 'service exited'
                try:
                    urllib.request.urlopen(base + '/api/status', timeout=1).close()
                    break
                except OSError:
                    time.sleep(0.1)
            else:
                raise AssertionError('server did not start')
            with sync_playwright() as playwright:
                browser = playwright.chromium.launch(headless=True, args=['--no-sandbox'],
                    executable_path=os.environ.get('LANPRINT_BROWSER_EXECUTABLE'))
                try:
                    page = browser.new_page(viewport={'width': 1366, 'height': 900}, device_scale_factor=1, is_mobile=True, has_touch=True)
                    page.on('console', lambda msg: console_errors.append(msg.text) if msg.type in ['warning', 'error'] else None)
                    page.on('pageerror', lambda error: console_errors.append(str(error)))
                    page.on('response', lambda response: network_errors.append({'url': response.url, 'status': response.status}) if response.status >= 400 else None)
                    page.set_default_timeout(15000)
                    page.goto(base)
                    expect(page.locator('#weekly-summary')).to_contain_text('暂无已提交至 CUPS 的记录')
                    expect(page.locator('#config')).to_have_count(0)
                    expect(page.locator('#status')).to_have_count(0)

                    def capture(name):
                        layout = page.evaluate('''() => ({width: innerWidth, scrollWidth: document.documentElement.scrollWidth,
                            overflowControls: [...document.querySelectorAll('input,button')].filter(el => {
                                const r = el.getBoundingClientRect(); return r.width && (r.left < 0 || r.right > innerWidth + 1);
                            }).map(el => el.id || el.name)})''')
                        layout['state'] = name
                        layouts.append(layout)
                        assert layout['scrollWidth'] <= layout['width'] and not layout['overflowControls'], layout
                        page.screenshot(path=str(evidence / f'{name}.png'), full_page=True)

                    capture('00-desktop-home-empty')
                    page.get_by_role('link', name='设置', exact=True).click()
                    expect(page.locator('#status')).to_contain_text('监控已暂停')
                    expect(page.locator('#upload')).to_have_count(0)
                    capture('00-desktop-settings')
                    page.set_viewport_size({'width': 360, 'height': 800})
                    capture('01-mobile-settings-unconfigured')
                    config = dict(watch_dir=str(inbox), printer='Mobile', cups_server='', lp_path=str(lp),
                        libreoffice_path=str(lo), stable_seconds=2, command_timeout_seconds=10)
                    for key, value in config.items():
                        page.locator(f'#config [name="{key}"]').fill(str(value))
                    page.locator('#config button[type=submit]').click()
                    expect(page.locator('#notice')).to_contain_text('配置已保存')
                    expect(page.locator('#restart')).to_contain_text('需要重启')
                    expect(page.locator('#start')).to_be_disabled()
                    capture('01-mobile-first-save-restart-required')
                    page.get_by_role('link', name='首页', exact=True).click()
                    expect(page.locator('#upload-submit')).to_be_disabled()
                    expect(page.locator('#upload-help')).to_contain_text('需要重启')
                    restart()
                    page.goto(base + '/settings')
                    expect(page.locator('#restart')).to_contain_text('无需重启')
                    expect(page.locator('#status')).to_contain_text('监控已暂停')
                    page.locator('#start').click()
                    expect(page.locator('#status')).to_contain_text('监控运行中')
                    page.locator('#pause').click()
                    expect(page.locator('#status')).to_contain_text('监控已暂停')
                    expect(page.locator('#config button[type=submit]')).to_be_enabled()
                    capture('01-mobile-settings-configured')
                    page.get_by_role('link', name='首页', exact=True).click()
                    expect(page.locator('#upload-submit')).to_be_enabled()
                    expect(page.locator('#weekly-chart .stat-day')).to_have_count(7)
                    hold.touch()
                    long_name = '手机上传文件' * 12 + '.PDF'
                    page.locator('#upload-file').set_input_files({'name': long_name, 'mimeType': 'application/pdf', 'buffer': make_pdf(2)})
                    capture('02-file-selected')
                    with page.expect_response(lambda response: response.url.endswith('/api/uploads') and response.request.method == 'POST') as response:
                        page.locator('#upload-submit').click()
                    assert response.value.status == 202
                    first = response.value.json()['job']
                    expect(page.locator(f'[data-job-id="{first["id"]}"]')).to_contain_text('提交中')
                    page.locator('#upload-file').set_input_files({'name': 'queued.docx', 'mimeType': 'application/octet-stream', 'buffer': b'word fixture'})
                    page.locator('#upload-submit').click()
                    expect(page.locator('#upload-result')).to_contain_text('queued.docx · 排队中')
                    capture('03-queued')
                    hold.unlink()
                    expect(page.locator('#upload-result')).to_contain_text('已提交 CUPS')
                    expect(page.locator('#weekly-summary')).to_contain_text('共 5 页')
                    expect(page.locator('#weekly-chart')).to_have_attribute('aria-label', __import__('re').compile('已提交至 CUPS 2 个文件'))
                    expect(page.locator('#jobs')).to_contain_text(long_name)
                    capture('04-submitted')

                    reject.touch()
                    page.locator('#upload-file').set_input_files({'name': 'failed.pdf', 'mimeType': 'application/pdf', 'buffer': make_pdf(1)})
                    page.locator('#upload-submit').click()
                    expect(page.locator('#upload-result')).to_contain_text('mobile simulated CUPS failure')
                    expect(page.locator('#weekly-summary')).to_contain_text('已提交至 CUPS 2 个文件')
                    capture('05-failed')
                    reject.unlink()
                    page.once('dialog', lambda dialog: dialog.accept())
                    page.get_by_role('button', name='重试', exact=True).click()
                    expect(page.locator('#upload-result')).to_contain_text('已提交 CUPS')
                    expect(page.locator('#weekly-summary')).to_contain_text('共 6 页')
                    capture('06-retried')
                    page.locator('#upload-file').set_input_files({'name': 'unknown.pdf', 'mimeType': 'application/pdf', 'buffer': b'not a PDF'})
                    page.locator('#upload-submit').click()
                    expect(page.locator('#upload-result')).to_contain_text('已提交 CUPS')
                    expect(page.locator('#weekly-summary')).to_contain_text('页数不完整：已知 6 页，1 个文件页数未知')
                    page.get_by_text('查看每日统计文本', exact=True).click()
                    expect(page.locator('#weekly-text')).to_contain_text('至少 6 页，1 个文件页数未知')
                    capture('07-incomplete-pages')
                    for width in [320, 390, 1366]:
                        page.set_viewport_size({'width': width, 'height': 900})
                        capture(f'08-home-{width}')
                    page.get_by_role('link', name='设置', exact=True).click()
                    expect(page.locator('#config [name=printer]')).to_have_value('Mobile')
                    # Save B through the form; active A remains visible and usable.
                    page.locator('#config [name=printer]').fill('MobileNext')
                    page.locator('#config button[type=submit]').click()
                    expect(page.locator('#restart')).to_contain_text('需要重启')
                    expect(page.locator('#counts')).to_contain_text('Mobile')
                    expect(page.locator('#counts')).not_to_contain_text('MobileNext')
                    expect(page.locator('#restart')).to_contain_text('继续使用上方活动配置')
                    expect(page.locator('#start')).to_be_enabled()
                    capture('09-desktop-settings-persisted')
                    for width in [320, 390]:
                        page.set_viewport_size({'width': width, 'height': 800})
                        capture(f'10-settings-{width}')
                    page.get_by_role('link', name='首页', exact=True).click()
                    expect(page.locator('#upload-help')).to_contain_text('需要重启')
                    expect(page.locator('#upload-help')).to_contain_text('活动打印队列 Mobile')
                    expect(page.locator('#upload-submit')).to_be_enabled()
                    capture('11-home-pending-restart')
                    restart()
                    page.goto(base + '/settings')
                    expect(page.locator('#restart')).to_contain_text('无需重启')
                    expect(page.locator('#counts')).to_contain_text('MobileNext')
                    expect(page.locator('#status')).to_contain_text('监控已暂停')
                    expect(page.locator('#config [name=printer]')).to_have_value('MobileNext')
                    capture('12-settings-after-restart')
                    assert not console_errors, console_errors
                    assert not network_errors, network_errors
                finally:
                    browser.close()
        finally:
            hold.unlink(missing_ok=True)
            process.terminate()
            process.wait(timeout=15)
            (evidence / 'browser-results.json').write_text(json.dumps(dict(viewports='320, 360, 390, 1366', layouts=layouts,
                console_errors=console_errors, network_errors=network_errors), ensure_ascii=False, indent=2))

print('PASS: 320/360/390/1366px home/settings navigation, first-save and changed-config restart notices, active/pending targets, external test-process restart and default pause, config persistence and start/pause, empty and incomplete weekly charts with text summaries, upload/queue/failure/retry; no horizontal overflow, console errors or failed HTTP responses')
