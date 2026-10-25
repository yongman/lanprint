"""Real HTTP upload parsing, limits, queue persistence and simulated printing."""
import http.client
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

from pdf_fixture import make_pdf

LIMIT = 50 * 1024 * 1024
BOUNDARY = 'lanprint-upload-test'


def wait_for(check, message, timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.1)
    raise AssertionError(message)


def multipart(name, content, field='file'):
    return (f'--{BOUNDARY}\r\nContent-Disposition: form-data; name="{field}"; filename="{name}"\r\n'
            'Content-Type: application/octet-stream\r\n\r\n').encode() + content + f'\r\n--{BOUNDARY}--\r\n'.encode()


with tempfile.TemporaryDirectory(prefix='lanprint-upload-') as root:
    root = Path(root)
    inbox = root / 'inbox'
    inbox.mkdir()
    data = root / 'data'
    lp, lo = root / 'lp', root / 'lo'
    hold, reject, calls = root / 'hold', root / 'reject', root / 'calls.jsonl'
    lp.write_text(f'''#!/usr/bin/python3
import json, pathlib, sys, time
while pathlib.Path({str(hold)!r}).exists(): time.sleep(0.05)
if pathlib.Path({str(reject)!r}).exists():
    print('simulated CUPS failure', file=sys.stderr)
    sys.exit(1)
with open({str(calls)!r}, 'a') as output:
    output.write(json.dumps(sys.argv[1:]) + '\\n')
print('request id is Office-101')
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
    process = None
    log = open(root / 'service.log', 'w+')

    def api(path, method='GET', body=None, expected=200):
        req = urllib.request.Request(base + path, method=method,
            data=None if body is None else json.dumps(body).encode(),
            headers={'X-Lanprint-Request': '1', 'Content-Type': 'application/json'})
        try:
            with urllib.request.urlopen(req, timeout=5) as response:
                assert response.status == expected
                return json.load(response)
        except urllib.error.HTTPError as error:
            assert error.code == expected, (error.code, error.read())
            return json.load(error)

    def upload(name='a.pdf', content=make_pdf(2), expected=202, raw=None, headers=None):
        conn = http.client.HTTPConnection('127.0.0.1', port, timeout=15)
        body = multipart(name, content) if raw is None else raw
        req_headers = {'Content-Type': f'multipart/form-data; boundary={BOUNDARY}', 'X-Lanprint-Request': '1'}
        req_headers.update(headers or {})
        conn.request('POST', '/api/uploads', body, req_headers)
        response = conn.getresponse()
        result = response.read()
        conn.close()
        assert response.status == expected, (response.status, result)
        if expected == 202:
            return json.loads(result)['job']

    def jobs():
        return api('/api/jobs?limit=200')['jobs']

    def job(job_id, status):
        return next((j for j in jobs() if j['id'] == job_id and j['status'] == status), None)

    def launch():
        global process
        process = subprocess.Popen([sys.argv[1], '--data-dir', str(data), '--bind', f'127.0.0.1:{port}'], stdout=log, stderr=log)
        def ready():
            assert process.poll() is None, 'service exited'
            try:
                return api('/api/status')
            except (OSError, ConnectionError):
                return False
        wait_for(ready, 'service did not start')

    def stop():
        process.terminate()
        process.wait(timeout=15)
        assert process.returncode == 0

    config = dict(watch_dir=str(inbox), printer='Office', cups_server='', lp_path=str(lp),
        libreoffice_path=str(lo), stable_seconds=2, command_timeout_seconds=10)
    try:
        launch()
        upload(expected=409)
        status = api('/api/status')
        assert not status['configured'] and not status['restart_required'] and status['active_config'] is None
        for _ in range(2):
            saved = api('/api/config', 'PUT', config)
            assert saved['restart_required'] and saved['active_config'] is None
            assert api('/api/config')['config'] == config
            assert not api('/api/status')['configured']
            api('/api/monitor/start', 'POST', expected=409)
            upload(expected=409)
        stop()
        launch()
        status = api('/api/status')
        assert status['configured'] and status['active_config'] == config
        assert not status['restart_required'] and not status['monitoring']
        assert not api('/api/config', 'PUT', config)['restart_required']
        upload(expected=403, headers={'X-Lanprint-Request': ''})
        upload(expected=403, headers={'Origin': 'http://evil.test'})
        upload(expected=403, headers={'Host': 'evil.test'})
        upload('bad.txt', expected=415)
        upload('bad.pdf.exe', expected=415)
        upload('../', expected=400)
        upload('x' * 252 + '.pdf', expected=400)
        upload(content=b'', expected=400)
        upload(raw=multipart('x.pdf', b'x', field='other'), expected=400)
        upload(raw=b'broken multipart', expected=400)
        upload(raw=b'anything', headers={'Content-Type': 'text/plain'}, expected=400)
        two_files = multipart('a.pdf', b'one').replace(f'--{BOUNDARY}--\r\n'.encode(), b'') + multipart('b.pdf', b'two')
        upload(raw=two_files, expected=400)
        upload(content=b'x' * (LIMIT + 1), expected=413)
        assert not jobs(), 'rejected uploads entered history'
        assert not list((data / 'spool').iterdir()), 'rejected upload leaked a snapshot'

        # Exactly the documented limit is allowed and bypasses the JSON 16 KiB limit.
        large = upload('limit.pdf', b'x' * LIMIT)
        large = wait_for(lambda: job(large['id'], 'submitted'), '50 MiB upload did not submit')
        assert large['page_count'] is None and large['submitted_at']
        empty_days = api('/api/stats/weekly')['days']
        assert len(empty_days) == 7 and sum(d['submitted_files'] for d in empty_days) == 1
        assert sum(d['unknown_page_files'] for d in empty_days) == 1
        assert api('/api/status')['monitoring'] is False
        assert api('/api/status')['max_upload_bytes'] == LIMIT

        # Busy uploads queue behind the active job; identical requests are independent.
        hold.touch()
        first = upload('../../mobile.PDF')
        wait_for(lambda: api('/api/status')['runtime']['active_job'] == first['id'], 'first upload did not become active')
        second = upload(r'C:\fakepath\mobile.PDF')
        assert first['id'] != second['id']
        assert second['status'] == 'queued' and second['origin'] == 'upload'
        assert first['file_name'] == second['file_name'] == 'mobile.PDF'
        assert Path(second['spool_path']).parent == data / 'spool'
        assert not (root / 'mobile.PDF').exists()
        hold.unlink()
        second = wait_for(lambda: job(second['id'], 'submitted'), 'queued upload did not submit')
        assert second['page_count'] == 2 and second['submitted_at']
        for name in ['letter.DoCx', 'legacy.DOC']:
            uploaded = upload(name, b'word fixture')
            converted = wait_for(lambda: job(uploaded['id'], 'submitted'), f'{name} did not convert and submit')
            assert converted['page_count'] == 3 and converted['submitted_at']

        before = sum(d['submitted_files'] for d in api('/api/stats/weekly')['days'])
        reject.touch()
        failed = upload('failed.pdf')
        failed = wait_for(lambda: job(failed['id'], 'failed'), 'print error missing from history')
        assert 'simulated CUPS failure' in failed['error'] and failed['created_at'] and failed['attempts'] == 1
        assert failed['page_count'] is None and failed['submitted_at'] is None
        assert sum(d['submitted_files'] for d in api('/api/stats/weekly')['days']) == before
        reject.unlink()
        api(f'/api/jobs/{failed["id"]}/retry?confirm=checked_queue', 'POST')
        retried = wait_for(lambda: job(failed['id'], 'submitted'), 'paused upload retry did not execute')
        assert retried['attempts'] == 2 and retried['page_count'] == 2
        assert sum(d['submitted_files'] for d in api('/api/stats/weekly')['days']) == before + 1

        # Saving B during an upload cannot change A's immutable startup target.
        wait_for(lambda: not api('/api/status')['busy'], 'worker not idle')
        other_inbox = root / 'inbox-b'
        other_inbox.mkdir()
        config_b = dict(config, watch_dir=str(other_inbox), printer='NewPrinter', cups_server='cups-b.test:631')
        body = multipart('switch.pdf', b'%PDF-slow-client')
        conn = http.client.HTTPConnection('127.0.0.1', port, timeout=5)
        conn.putrequest('POST', '/api/uploads')
        conn.putheader('X-Lanprint-Request', '1')
        conn.putheader('Content-Type', f'multipart/form-data; boundary={BOUNDARY}')
        conn.putheader('Content-Length', str(len(body)))
        conn.endheaders()
        conn.send(body[:len(body) // 2])
        for _ in range(2):
            saved = api('/api/config', 'PUT', config_b)
            assert saved['restart_required'] and saved['active_config'] == config
        status = api('/api/status')
        assert status['restart_required'] and status['active_config'] == config
        assert api('/api/config')['config'] == config_b
        conn.send(body[len(body) // 2:])
        response = conn.getresponse()
        switched = json.loads(response.read())['job']
        conn.close()
        assert response.status == 202 and switched['config'] == config
        wait_for(lambda: job(switched['id'], 'submitted'), 'switched upload did not submit')
        assert json.loads(calls.read_text().splitlines()[-1])[:2] == ['-d', 'Office']
        following = upload('after-save.pdf')
        assert following['config'] == config
        wait_for(lambda: job(following['id'], 'submitted'), 'active-config upload did not submit')
        wait_for(lambda: not api('/api/status')['busy'], 'worker not idle')
        # Returning saved settings to A clears the pending change without restart.
        assert not api('/api/config', 'PUT', config)['restart_required']
        assert api('/api/config', 'PUT', config_b)['restart_required']
        api('/api/monitor/start', 'POST')
        assert api('/api/status')['monitoring']
        api('/api/monitor/pause', 'POST')
        assert all(j['config'] == config for j in jobs())
        assert all('NewPrinter' not in line and 'cups-b.test' not in line for line in calls.read_text().splitlines())

        # Persist a queued upload across restart while keeping monitoring paused.
        hold.touch()
        active = upload('active.pdf')
        wait_for(lambda: api('/api/status')['runtime']['active_job'] == active['id'], 'restart barrier not active')
        pending = upload('restart.pdf')
        assert pending['config'] == config
        process.terminate()
        hold.unlink()
        process.wait(timeout=15)
        assert process.returncode == 0
        launch()
        status = api('/api/status')
        assert not status['monitoring'] and not status['restart_required']
        assert status['active_config'] == config_b
        persisted = wait_for(lambda: job(pending['id'], 'submitted'), 'persisted upload did not resume while paused')
        assert persisted['config'] == config
        assert json.loads(calls.read_text().splitlines()[-1])[:2] == ['-d', 'Office']
        fresh = upload('new-process.pdf')
        assert fresh['config'] == config_b
        wait_for(lambda: job(fresh['id'], 'submitted'), 'new active config did not submit')
        assert json.loads(calls.read_text().splitlines()[-1])[:4] == ['-h', 'cups-b.test:631', '-d', 'NewPrinter']
        assert len(calls.read_text().splitlines()) == len(jobs()), 'missing or duplicate print submissions'
        stats = api('/api/stats/weekly')
        assert stats['metric'] == 'submitted_to_cups' and stats['timezone'] == 'service_local'
        assert sum(d['submitted_files'] for d in stats['days']) == len(jobs())
        assert sum(d['known_pages'] for d in stats['days']) == sum(j['page_count'] or 0 for j in jobs())
        assert sum(d['unknown_page_files'] for d in stats['days']) == sum(j['page_count'] is None for j in jobs())
        assert any(d['pages'] is None for d in stats['days'])
        stop()
        print('PASS: PDF and converted DOC/DOCX page counts, incomplete statistics, acceptance time persistence, failed exclusion and single-count retry;  multipart parsing, Host/Origin/CSRF, empty/unsupported/multiple/malformed requests, 50 MiB boundary and cleanup, safe names, distinct explicit requests, busy queue, PDF/DOC/DOCX, failure/retry, first/repeated/unchanged/reverted saves, A retained during/after upload and monitor start, B active only after restart, queued A retained across restart while monitoring paused')
    except BaseException:
        if process and process.poll() is None:
            process.kill()
            process.wait()
        log.seek(0)
        print(log.read(), file=sys.stderr)
        raise
    finally:
        log.close()
