"""Use real HTTP, filesystem and child processes; no printer or office install needed."""
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

binary = sys.argv[1]


def wait_for(check, message, timeout=20):
    end = time.monotonic() + timeout
    while time.monotonic() < end:
        value = check()
        if value:
            return value
        time.sleep(0.15)
    raise AssertionError(message)


def executable(path, body):
    path.write_text('#!/usr/bin/python3\n' + body)
    path.chmod(0o700)


with tempfile.TemporaryDirectory(prefix='lanprint-test-') as root:
    root = Path(root)
    inbox = root / 'inbox'
    inbox.mkdir()
    data = root / 'data'
    lp = root / 'fake-lp'
    lo = root / 'fake-lo'
    calls = root / 'calls.jsonl'
    conversions = root / 'conversions.jsonl'
    reject = root / 'reject'
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    base = f'http://127.0.0.1:{port}'
    process = None
    log = open(root / 'service.log', 'w+')

    def request(path, method='GET', body=None, expected=200, headers=None):
        req = urllib.request.Request(base + path, method=method,
            data=None if body is None else json.dumps(body).encode(),
            headers=headers if headers is not None else {'Content-Type': 'application/json', 'X-Lanprint-Request': '1'})
        try:
            with urllib.request.urlopen(req, timeout=3) as response:
                assert response.status == expected
                raw = response.read()
                return json.loads(raw) if path.startswith('/api/') else raw
        except urllib.error.HTTPError as error:
            assert error.code == expected, (error.code, error.read())
            return None

    def launch():
        global process
        process = subprocess.Popen([binary, '--data-dir', str(data), '--bind', f'127.0.0.1:{port}'], stdout=log, stderr=log)
        def ready():
            assert process.poll() is None, 'server exited'
            try:
                return request('/api/status')
            except (urllib.error.URLError, ConnectionError):
                return False
        wait_for(ready, 'server did not start')

    def stop():
        process.terminate()
        process.wait(timeout=10)
        assert process.returncode == 0

    def jobs():
        return request('/api/jobs')['jobs']

    def job(name, status):
        return next((j for j in jobs() if j['file_name'] == name and j['status'] == status), None)

    config = dict(watch_dir=str(inbox), printer='Office', cups_server='cups.test:631',
        lp_path=str(lp), libreoffice_path=str(lo), stable_seconds=2, command_timeout_seconds=3)
    try:
        launch()
        assert b'Lanprint' in request('/')
        assert b'fetch(' in request('/app.js')
        request('/api/monitor/start', 'POST', expected=409)
        request('/api/monitor/pause', 'POST', expected=403, headers={})
        request('/api/monitor/pause', 'POST', expected=403, headers={'X-Lanprint-Request': '1', 'Origin': 'http://evil.example'})
        request('/api/config', 'PUT', dict(config, printer='-o'), expected=400)
        assert request('/api/config', 'PUT', config)['restart_required']
        request('/api/monitor/start', 'POST', expected=409)
        stop()
        launch()
        assert not request('/api/status')['restart_required']
        request('/api/monitor/start', 'POST')
        request('/api/config', 'PUT', config, expected=400)
        source = inbox / 'slow.PdF'
        source.touch()
        time.sleep(3.3)
        assert not jobs(), 'queued an empty download placeholder'
        source.write_bytes(b'%PDF-part-1')
        # Extend the file before its quiet period has elapsed.
        time.sleep(1.5)
        source.write_bytes(b'%PDF-part-1-part-2')
        time.sleep(1.2)
        assert not jobs(), 'printed a file before the stability window elapsed'
        failed = wait_for(lambda: job(source.name, 'failed'), 'missing lp was not recorded')
        assert 'install the dependency' in failed['error']
        assert len(jobs()) == 1
        # Create the previously missing dependency, then retry the same durable snapshot.
        executable(lp, f'''import json, pathlib, sys
if pathlib.Path({str(reject)!r}).exists():
    print("CUPS service unavailable", file=sys.stderr)
    sys.exit(1)
with open({str(calls)!r}, 'a') as output:
    output.write(json.dumps({{"args":sys.argv[1:], "content":pathlib.Path(sys.argv[-1]).read_text()}}) + '\\n')
print('request id is Office-42 (1 file(s))')
''')
        request(f'/api/jobs/{failed["id"]}/retry', 'POST', expected=400)
        request(f'/api/jobs/{failed["id"]}/retry?confirm=checked_queue', 'POST')
        submitted = wait_for(lambda: job(source.name, 'submitted'), 'retry did not submit')
        assert submitted['attempts'] == 2
        assert submitted['cups_response'].startswith('request id is Office-42')
        request(f'/api/jobs/{failed["id"]}/retry?confirm=checked_queue', 'POST', expected=409)
        sent = json.loads(calls.read_text().splitlines()[0])
        assert sent['args'][:4] == ['-h', 'cups.test:631', '-d', 'Office']
        assert sent['content'] == '%PDF-part-1-part-2'
        (inbox / 'ignored.txt').write_text('not supported')
        (inbox / 'linked.pdf').symlink_to(source)
        # A missing converter is also a failed job, and cannot silently submit a DOCX.
        (inbox / 'letter.DoCx').write_text('word document')
        word = wait_for(lambda: job('letter.DoCx', 'failed'), 'missing LibreOffice was not recorded')
        assert 'LibreOffice conversion' in word['error']
        # LibreOffice can exit zero on missing import filters without producing output.
        executable(lo, 'print("Error: source file could not be loaded")\n')
        request(f'/api/jobs/{word["id"]}/retry?confirm=checked_queue', 'POST')
        no_pdf = wait_for(lambda: (j if (j := job('letter.DoCx', 'failed')) and j['attempts'] == 2 else None), 'converter silently accepted missing PDF')
        assert 'libreoffice-writer' in no_pdf['error']
        assert 'source file could not be loaded' in no_pdf['error']
        executable(lo, f'''import json, pathlib, sys
source = pathlib.Path(sys.argv[-1])
out = pathlib.Path(sys.argv[sys.argv.index('--outdir') + 1])
(out / (source.stem + '.pdf')).write_text('%PDF-converted-' + source.read_text())
with open({str(conversions)!r}, 'a') as output:
    output.write(json.dumps(sys.argv[1:]) + '\\n')
''')
        request(f'/api/jobs/{word["id"]}/retry?confirm=checked_queue', 'POST')
        wait_for(lambda: job('letter.DoCx', 'submitted'), 'DOCX conversion failed')
        (inbox / 'legacy.DOC').write_text('legacy word')
        wait_for(lambda: job('legacy.DOC', 'submitted'), 'DOC conversion failed')
        assert len(conversions.read_text().splitlines()) == 2
        assert len(calls.read_text().splitlines()) == 3
        reject.touch()
        (inbox / 'unavailable.pdf').write_text('%PDF-new')
        unavailable = wait_for(lambda: job('unavailable.pdf', 'failed'), 'CUPS failure not recorded')
        assert 'CUPS service unavailable' in unavailable['error']
        request('/api/monitor/pause', 'POST')
        reject.unlink()
        request(f'/api/jobs/{unavailable["id"]}/retry?confirm=checked_queue', 'POST')
        time.sleep(1.3)
        assert job('unavailable.pdf', 'queued'), 'paused queue was processed'
        # Config/history/queued jobs persist; restart never enables monitoring on its own.
        stop()
        launch()
        assert not request('/api/status')['monitoring']
        assert request('/api/config')['config'] == config
        assert len(jobs()) == 4
        request('/api/monitor/start', 'POST')
        wait_for(lambda: job('unavailable.pdf', 'submitted'), 'persisted queue did not resume')
        time.sleep(3.3)
        assert len(jobs()) == 4, 'same files requeued after restart'
        assert len(calls.read_text().splitlines()) == 4, 'duplicate submissions after restart'
        # Changed content at the same path is one new version, identical touch is ignored.
        os.utime(source, None)
        time.sleep(3.3)
        assert len(jobs()) == 4
        source.write_text('%PDF-revision-2')
        wait_for(lambda: len(jobs()) == 5 and all(j['status'] == 'submitted' for j in jobs()), 'changed file did not produce exactly one new job')
        assert len(calls.read_text().splitlines()) == 5
        assert len(request('/api/jobs?limit=2&offset=2')['jobs']) == 2
        stop()
        print('PASS: real HTTP config/control/history, stability, PDF/DOCX/DOC pipelines, missing dependencies, CUPS errors, retry, pause, restart persistence, symlink exclusion, version dedup, pagination')
    except BaseException:
        if process and process.poll() is None:
            process.kill()
            process.wait()
        log.seek(0)
        print(log.read(), file=sys.stderr)
        raise
    finally:
        log.close()
