# Lanprint

[简体中文](README.md) | [English](README.en.md)

Lanprint is a Linux folder-to-CUPS print service written in Rust. It watches one folder for stable PDF, DOC, DOCX, PNG, JPG, and JPEG files, snapshots each accepted file, and submits it to a CUPS queue. Word documents and images are converted to PDF with LibreOffice. The local web console also supports explicit uploads, print history, retries, and a seven-day chart of jobs submitted to CUPS.

The interface supports Simplified Chinese and English. It follows the browser language on first use. Use the language selector in the page header to change it; the choice is saved in that browser.

## Requirements

- Linux and Rust 1.93.0, selected by `rust-toolchain.toml`
- A C compiler and `pkg-config`
- CUPS client tools, including `lp`
- Poppler `pdfinfo` for page counts
- LibreOffice Writer and suitable fonts for DOC and DOCX conversion; LibreOffice Draw and matching import filters for image conversion

On Debian or Ubuntu:

```bash
sudo apt update
sudo apt install build-essential pkg-config cups-client poppler-utils libreoffice-writer libreoffice-draw fonts-noto-cjk
```

## Build and first run

```bash
git clone https://github.com/yongman/lanprint.git
cd lanprint
cargo build --release --locked
mkdir -p "$HOME/print-inbox" "$HOME/.local/share/lanprint"
./target/release/lanprint --data-dir "$HOME/.local/share/lanprint"
```

Open <http://127.0.0.1:8787> and go to **Settings**:

1. Enter an existing watch folder, a CUPS queue name, and the optional CUPS server address. Leave the server blank to use the system default.
2. Check the paths to `lp` and `libreoffice`. The defaults are `/usr/bin/lp` and `/usr/bin/libreoffice`.
3. Save the settings. A service administrator must restart Lanprint before the saved settings become active. The first saved settings also need a restart.
4. After restart, confirm that no restart is needed. Start folder monitoring when you are ready. The first scan can submit supported files that already exist in the folder.

The service starts with folder monitoring paused. A restart does not start monitoring automatically. Upload jobs continue automatically, including after a restart. Configuration changes do not rewrite queued jobs or retries; each job keeps the printer settings saved when it entered the queue.

## CUPS setup

Lanprint calls CUPS `lp`; CUPS handles IPP, drivers, authentication, and printer capabilities. To inspect available queues:

```bash
lpstat -p -d
lpstat -h print-server.local:631 -p
```

An administrator can create a local queue for a network printer with:

```bash
sudo lpadmin -p Office -E -v ipp://printer.local/ipp/print -m everywhere
```

Set the queue name to `Office` in Lanprint. A direct `lp` command submits a real print job. Use a test document:

```bash
lp -h print-server.local:631 -d Office -- test.pdf
```

Lanprint does not prompt for CUPS passwords or store them in the web interface. Configure any required operating system credentials and CUPS access rules first.

## Folder monitoring and job history

- Lanprint polls only the top level of the configured folder. It does not recurse or follow symbolic links. It accepts case-insensitive `.pdf`, `.docx`, `.doc`, `.png`, `.jpg`, and `.jpeg` extensions.
- A file must keep the same inode, size, and timestamps for the configured quiet period before Lanprint copies it to a private spool. The quiet period cannot prove that a writer has closed the file. For reliable handoff, write a temporary file, close it, then atomically rename it to its final name on the same filesystem.
- Lanprint stores a SHA-256 fingerprint for the full source path and content. Repeated events and restarts do not print the same path and content again. Changed content at the same path is a new version. Explicit uploads are separate print requests and can print the same content more than once.
- Jobs are processed serially. Word documents and images move through PDF conversion and CUPS submission. Failed jobs do not retry automatically. The retry action uses the original snapshot and original printer settings.
- A `Submitted to CUPS` status means that `lp` returned success. It does not confirm that paper came out. Check CUPS and the printer for the final result.
- The history shows status, attempts, errors, and PDF page count when available. The seven-day chart counts jobs accepted by CUPS, grouped by the service's local calendar date. Page counts are PDF document pages, not physical sheets. Unknown counts remain marked as unknown and are not treated as zero.

## Uploads

The home page accepts one PDF, DOC, DOCX, PNG, JPG, or JPEG file at a time. The limit is 50 MiB. Uploads enter the queue even while folder monitoring is paused. Invalid, empty, unsupported, or oversized uploads are rejected. The service creates storage names itself and uses the supplied file name only for display and the CUPS job title.

Word and image conversion and CUPS submission run as separate commands with timeouts. Each conversion uses a separate LibreOffice user profile and produces a PDF for page counting and printing. The service does not claim exactly-once printing: if CUPS accepts a job but the local process fails before saving the result, a retry may print it again. Check the CUPS queue before retrying an interrupted or timed-out job.

## Network security

The web console has **no user authentication and no TLS**. Anyone who can connect can view paths and history, change settings, upload documents, and request printing. Use the default loopback binding for local access. Do not expose the service to the public Internet or configure public port forwarding.

Lanprint can bind to loopback addresses or an explicitly selected RFC1918 private IPv4 address. Wildcard, public, and other non-loopback IPv6 addresses are rejected. Binding to a private address does not restrict client source networks. If you enable LAN access, use only a trusted isolated network and restrict reachable clients with network controls or a firewall.

For remote administration, keep the service on loopback and use an authenticated SSH tunnel:

```bash
ssh -N -L 8787:127.0.0.1:8787 user@linux-host
```

Then open <http://127.0.0.1:8787> on the client. The data directory contains source document snapshots, the SQLite database, and retry data. It is created with owner-only permissions. Service logs go to stdout and stderr and can contain file names, paths, and document errors. Snapshots are not automatically cleaned up; allow disk space and back up the database and spool together while the service is stopped.

## systemd user service

The included [systemd unit](deploy/lanprint.service) binds to loopback by default. Review its `ExecStart` before installing it. Example:

```bash
install -Dm755 target/release/lanprint "$HOME/.local/bin/lanprint"
install -Dm644 deploy/lanprint.service "$HOME/.config/systemd/user/lanprint.service"
systemctl --user daemon-reload
systemctl --user enable --now lanprint
journalctl --user -u lanprint -f
```

After a service restart, folder monitoring remains paused. Stop the service with `systemctl --user stop lanprint`.

## API

All write requests require a valid Host, `X-Lanprint-Request: 1`, and a matching Origin when supplied. The server does not enable CORS.

- `GET /api/status`
- `GET` and `PUT /api/config`
- `POST /api/monitor/start` and `POST /api/monitor/pause`
- `GET /api/jobs?limit=50&offset=0`
- `GET /api/stats/weekly`
- `POST /api/uploads` with one multipart field named `file`
- `POST /api/jobs/{id}/retry?confirm=checked_queue`

## Development

The HTML, JavaScript, and CSS are embedded in the Rust binary; there is no frontend build step.

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node --check web/app.js
node --check web/settings.js
```

Tests use a simulated CUPS command and do not print to a physical printer. Real DOC or DOCX conversion can also be checked with `python3 tests/real_office.py target/release/lanprint` when LibreOffice Writer is installed. That script still uses a simulated `lp` command.

See [LICENSE](LICENSE) for the MIT License. The design was informed by the public project [apirkle/zpl-autoprint](https://github.com/apirkle/zpl-autoprint); Lanprint is an independent implementation and does not copy its source code.
