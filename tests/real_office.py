"""Optional real LibreOffice smoke test. The lp replacement never contacts CUPS."""
import json
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request
import zipfile

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'target/release/lanprint').resolve())
lo = shutil.which('libreoffice')
assert lo, 'Install LibreOffice Writer first'
with tempfile.TemporaryDirectory(prefix='lanprint-office-') as root:
    root = Path(root)
    inbox = root / 'inbox'
    inbox.mkdir()
    document = root / 'fixture.docx'
    with zipfile.ZipFile(document, 'w') as archive:
        archive.writestr('[Content_Types].xml', '<?xml version="1.0"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/></Types>')
        archive.writestr('_rels/.rels', '<?xml version="1.0"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>')
        archive.writestr('word/document.xml', '<?xml version="1.0"?><w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main"><w:body><w:p><w:r><w:t>Lanprint real office verification</w:t></w:r></w:p></w:body></w:document>')
    fixture = subprocess.run([lo, '-env:UserInstallation=' + (root / 'fixture-profile').as_uri(), '--headless', '--convert-to', 'doc:MS Word 97', '--outdir', str(root), str(document)], capture_output=True, text=True, timeout=60)
    legacy = root / 'fixture.doc'
    assert fixture.returncode == 0 and legacy.is_file(), ('Install LibreOffice Writer (libreoffice-writer) and verify DOCX import filters', fixture.stdout, fixture.stderr)
    assert legacy.read_bytes().startswith(bytes.fromhex('d0cf11e0a1b11ae1')), 'not a legacy Word OLE file'
    shutil.copyfile(document, inbox / 'document.DOCX')
    shutil.copyfile(legacy, inbox / 'legacy.DoC')
    accepted = root / 'accepted.jsonl'
    lp = root / 'fake-lp'
    lp.write_text(f'''#!/usr/bin/python3
import json,pathlib,sys
pdf=pathlib.Path(sys.argv[-1]).read_bytes()
assert pdf.startswith(b'%PDF-'), 'converter output is not a PDF'
with open({str(accepted)!r},'a') as output:
    output.write(json.dumps({{"title":sys.argv[sys.argv.index('-t')+1],"pdf_bytes":len(pdf)}})+'\\n')
print('request id is Office-smoke (1 file(s))')
''')
    lp.chmod(0o700)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    base = f'http://127.0.0.1:{port}'
    def api(path, method='GET', body=None):
        req = urllib.request.Request(base + path, method=method, data=None if body is None else json.dumps(body).encode(), headers={'Content-Type': 'application/json', 'X-Lanprint-Request': '1'})
        with urllib.request.urlopen(req, timeout=3) as response:
            return json.load(response)
    with open(root / 'service.log', 'w+') as log:
        server = subprocess.Popen([binary, '--data-dir', str(root / 'data'), '--bind', f'127.0.0.1:{port}'], stdout=log, stderr=log)
        try:
            end = time.monotonic() + 10
            while True:
                assert server.poll() is None, 'server exited'
                try:
                    api('/api/status')
                    break
                except (OSError, urllib.error.URLError):
                    assert time.monotonic() < end, 'server did not start'
                    time.sleep(0.1)
            api('/api/config', 'PUT', dict(watch_dir=str(inbox), printer='Office', cups_server='', lp_path=str(lp), libreoffice_path=lo, stable_seconds=2, command_timeout_seconds=60))
            # Only the isolated test process is restarted to load saved settings.
            server.terminate()
            server.wait(timeout=15)
            assert server.returncode == 0
            server = subprocess.Popen([binary, '--data-dir', str(root / 'data'), '--bind', f'127.0.0.1:{port}'], stdout=log, stderr=log)
            end = time.monotonic() + 10
            while True:
                assert server.poll() is None, 'server exited after restart'
                try:
                    status = api('/api/status')
                    assert not status['restart_required'] and not status['monitoring']
                    break
                except OSError:
                    assert time.monotonic() < end, 'server did not restart'
                    time.sleep(0.1)
            api('/api/monitor/start', 'POST')
            end = time.monotonic() + 150
            while True:
                jobs = api('/api/jobs')['jobs']
                assert not any(j['status'] == 'failed' for j in jobs), jobs
                if len(jobs) == 2 and all(j['status'] == 'submitted' for j in jobs):
                    break
                assert time.monotonic() < end, jobs
                time.sleep(0.2)
            results = [json.loads(line) for line in accepted.read_text().splitlines()]
            assert len(results) == 2 and all(r['pdf_bytes'] > 100 for r in results), results
            assert all(j['page_count'] == 1 and j['submitted_at'] for j in jobs), jobs
            stats = api('/api/stats/weekly')
            assert sum(d['submitted_files'] for d in stats['days']) == 2
            assert sum(d['known_pages'] for d in stats['days']) == 2
            print(subprocess.check_output([lo, '--version'], text=True).strip())
            print('PASS: real DOCX and binary DOC -> LibreOffice PDF -> mock lp:', json.dumps(results))
        except BaseException:
            log.flush()
            log.seek(0)
            print(log.read(), file=sys.stderr)
            raise
        finally:
            server.terminate()
            server.wait(timeout=125)
