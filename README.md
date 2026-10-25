# Lanprint

[简体中文](README.md) | [English](README.en.md)

Linux 上的 Rust 目录自动打印和 Web 上传打印工具。通过本机 Web 页面配置监控目录和 CUPS 队列，稳定写入的 PDF 直接提交，DOC/DOCX 与 PNG/JPG/JPEG 图片先由 LibreOffice headless 转成 PDF。SQLite 保存配置、文件指纹和任务历史，失败可手动重试。

界面支持简体中文和英文。首次访问时按浏览器语言选择；可通过页面顶部的语言菜单切换，选择保存在当前浏览器。

## 安装与首次运行

也可以从 [GitHub Releases](https://github.com/yongman/lanprint/releases) 下载 Linux x86_64 预编译包和 SHA-256 校验文件。解压后可将 `lanprint` 安装到 `~/.local/bin`；运行时仍需要本节列出的系统依赖。

需要 Linux、Rust 工具链（仓库固定为 1.93.0）、C 编译器，以及 CUPS 客户端。PDF 页数统计需要 Poppler 的 `/usr/bin/pdfinfo`（`poppler-utils`）。尚无 Rust 时请先按 <https://rustup.rs> 安装 rustup；进入项目执行 Cargo 命令会按 `rust-toolchain.toml` 选择工具链。Word 转换另需 LibreOffice Writer 和所用字体；图片转换需安装 LibreOffice Draw 与对应图像导入过滤器。SQLite 已内置，不需要安装 SQLite 开发库。

Debian / Ubuntu 示例：

```bash
sudo apt update
sudo apt install build-essential pkg-config cups-client poppler-utils libreoffice-writer libreoffice-draw fonts-noto-cjk
cd /path/to/lanprint
cargo build --release --locked
mkdir -p "$HOME/print-inbox" "$HOME/.local/share/lanprint"
./target/release/lanprint --data-dir "$HOME/.local/share/lanprint"
```

打开 <http://127.0.0.1:8787>，通过顶部导航进入“设置”（`/settings`）：

1. 填入监控目录的绝对路径、CUPS 队列名，例如 `Office`。目录必须存在，服务用户须有读取权限。
2. 本机 CUPS 保持“服务端”为空；远程 CUPS 可填 `print-server.local:631`。这是 CUPS 服务主机，不是打印机的 IPP URI。
3. 确认 `/usr/bin/lp`、`/usr/bin/libreoffice`，或设置实际的绝对可执行路径。可先配置而不安装，执行失败会记录依赖错误。
4. 保存配置后，由管理员停止并重新启动该服务进程，重新进入设置页确认无需重启，再点击“启动监控”。首次保存也必须重启。**首次启动会处理目录中已有的支持文件**。默认稳定等待 5 秒、每个命令超时 120 秒。
5. 将文件放入目录，在设置页查看监控状态，在首页查看打印历史。修改配置前须暂停并等待当前扫描/快照或打印任务结束（状态 `busy=false`）。

服务每次启动均默认暂停目录监控。配置和排队历史会保留；目录任务点击启动后继续，上传任务自动继续。`Ctrl-C` 或 `SIGTERM` 停止接收请求并等待当前打印操作结束。Word/图片转换和提交各有独立超时，最大各 600 秒。

## 打印机与 IPP

Lanprint 调用 CUPS 的 `lp`，由 CUPS 负责 IPP、驱动、认证和打印机能力协商，不直接实现 IPP 协议。

```bash
# 已有 CUPS 队列
lpstat -p -d
lpstat -h print-server.local:631 -p
# 独立检查提交链路；此命令会真实打印，请使用测试文件
lp -h print-server.local:631 -d Office -- test.pdf
```

如果只有打印机的 IPP 地址，可由管理员在本机建立队列（需要本机 CUPS 服务，Debian/Ubuntu 可安装 `cups`）：

```bash
sudo lpadmin -p Office -E -v ipp://printer.local/ipp/print -m everywhere
```

在页面将队列填为 `Office`，服务端留空。局域网 CUPS 需允许该服务用户/主机提交任务。MVP 不处理交互式密码提示，不在页面保存 CUPS 密码；需要认证的环境应先配置好操作系统凭据、证书及 CUPS 访问规则。

## 文件稳定性、去重与状态

- 轮询当前目录一层，不递归，不跟随文件符号链接；只处理 UTF-8 路径及大小写不敏感的 `.pdf`、`.docx`、`.doc`、`.png`、`.jpg`、`.jpeg`。空文件占位符跳过，其他文件不修改。
- 在连续观察到 inode、长度、mtime、ctime 均保持不变达到稳定等待时间后，复制并计算 SHA-256；复制前后再检查元数据。若发生变化，丢弃副本并重新等待。
- **稳定时间是静默窗口，不是生产程序已关闭文件的证明**。写入程序若暂停超过该窗口，仍可能被视为完成。可靠接入请先写 `.tmp` 文件，关闭后在同一文件系统原子重命名为最终扩展名；网络文件系统也应采用此方式。
- 快照先落盘，再以“完整源路径＋内容 SHA-256”的唯一约束入队。重复通知、只修改时间、服务重启、原路径删除后放回相同内容均不会自动重印。同路径内容变化视为新版本；不同路径或重命名后的文件视为新文件。恢复旧数据备份可能遗失近期指纹。
- 串行处理：`queued → converting（Word/图片）→ submitting → submitted`，任何执行错误进入 `failed`。页面的“已提交 CUPS”只代表 `lp` 返回成功，**不代表实际出纸**；CUPS 后续的缺纸、离线等问题需在 CUPS/打印机检查。
- 暂停停止后续目录扫描及目录队列执行；已在入队前暂停的快照会完成当前复制后丢弃，不再入队。已入队的目录快照留队；上传任务继续自动执行，不取消已开始的转换/提交。`busy` 包括配置读取、目录扫描/快照和转换/提交，暂停不会立即清除 `busy`。等待其变为 `false` 后才可保存配置或重新启动。转换期间扫描也等待；这是适合小型打印收件箱的串行 MVP。
- 活动配置在进程启动时加载并固定。保存只持久化下次启动使用的配置，设置页显示“需要重启”；重启前启动监控、新上传及传输中上传均继续使用活动配置。首次保存后尚无活动配置，启动和上传返回 409，必须先重启。保存相同活动配置无需重启；保存回活动配置可撤销待生效更改。
- 保存不会取消、改写排队任务。目录排队任务等待启动，上传自动继续；所有已入队任务和人工重试始终使用各自的原快照及原配置，跨重启同样保留。因此切换到 B 后，原先排队的 A 任务仍可能提交到 A；只有新任务使用 B。
- 失败不会自动重试。按钮先提示检查 CUPS 队列，再将该条历史重新入队，尝试次数累计。重试使用**原始快照和入队时的打印配置**，不读取原文件的新内容。修复依赖请安装到原配置路径，修复队列请恢复原目标；修改当前页面配置不会改写失败历史的重试目标，保存新配置不会取消或改写排队任务。
- 进程异常退出时，`queued` 任务保留；下次启动上传任务自动继续，目录任务等待启动监控。处于转换/提交状态的任务转成失败，提示人工核查后重试。CUPS 接收与 SQLite 更新之间无法原子提交，断电、超时或通信错误可能导致结果不确定；人工重试仍有重复出纸风险，不声称 exactly-once。
- 每个 Word 转换使用独立 LibreOffice 用户目录，避免连接已有桌面实例；命令不经过 shell，带超时和进程组终止，诊断输出每路最多保留 8 KiB。

## 手机上传打印

页面适配 360px 手机视口，历史在窄屏按任务纵向展示，无需横向滚动。可通过本机连接、经过认证的 SSH 隧道，或按下方访问控制说明启用的可信局域网访问。

1. 首次使用先在“设置”页保存目录与打印机配置，由管理员重启服务使其生效，再返回首页；无需启动目录监控。
2. 在“上传打印”选择一个 PDF、DOC、DOCX、PNG、JPG 或 JPEG，点击“上传并打印”。单文件固定上限 50 MiB（52,428,800 字节），不接受空文件；格式按大小写不敏感的最终扩展名检查。
3. 成功接受后显示任务编号和排队状态；串行工作线程约每秒选取一次任务，忙碌时等待前面的任务完成。上传任务与目录任务共用 SQLite、快照及转换/打印流程，但监控暂停、服务重启后也会自动处理。
4. 在上传结果及历史中查看文件名、创建时间、状态、尝试次数和错误。“已提交 CUPS”表示被 CUPS 接受，实际出纸仍需检查打印机。失败后检查 CUPS 队列，再按现有“重试”流程处理；上传重试无需启动监控。

每次上传都是独立的显式打印请求，即使文件名、内容完全相同也会再打印一次。客户端的目录部分（包括 Windows 路径）会被剥离，剩余名称仅用于显示和 CUPS 标题；服务生成随机磁盘名称，不将客户端名称拼入保存路径。拒绝缺失、过长或含控制字符的名称。上传内容按流写入私有 spool，完整收取且落盘后才入队；解析错误、超限或额外字段不会留下任务，并清理临时快照。强制终止时仍可能留下未引用文件。

上传始终采用**当前进程启动时的活动配置**，传输期间保存新配置也不切换目标。首页提示当前活动打印队列及待重启状态；后续保存不会取消或改写排队任务。重启后新上传才使用新配置，已入队上传仍自动使用原配置执行。上传只检查扩展名，损坏或与扩展名不符的文档可能在转换或打印时失败。没有新增文档内容沙箱。

### 上传 API

`POST /api/uploads` 使用 `multipart/form-data`，仅接受一个名为 `file` 的文件字段。与其他写接口一样要求有效 Host、`X-Lanprint-Request: 1` 和匹配的 Origin（如提供）；不开放跨域访问。服务限制文件内容至 50 MiB，并限制 multipart 总请求至 50 MiB + 16 KiB。其他 JSON 请求仍为 16 KiB 上限。

```bash
curl -H 'X-Lanprint-Request: 1' -F 'file=@example.pdf' http://127.0.0.1:8787/api/uploads
```

成功返回 HTTP 202 和 `{"job":{"id":1,"file_name":"example.pdf","status":"queued","origin":"upload",...}}`，表示持久化接受，转换/打印结果异步更新到 `GET /api/jobs`。未加载活动配置（含首次保存但尚未重启）返回 409；不支持的扩展名返回 415；文件或请求体超限返回 413；空文件、无效文件名、错误 multipart、缺少/多余字段返回 400；请求保护失败返回 403。执行时缺少 lp/LibreOffice、转换失败或 CUPS 错误会进入 `failed` 历史，沿用现有重试接口。

## 页面与最近 7 天统计

首页 `/` 只显示上传入口、上传结果/任务状态和打印历史；历史内提供最近 7 天的文件数与页数柱状图。独立设置页 `/settings` 放置监控状态、启停和原有配置表单，两页都有首页/设置导航。配置表单显示已保存值，状态区显示活动目录、打印队列及待重启提示。修改前仍须暂停并等待空闲；保存只持久化待生效值，重启前继续使用活动配置，队列保留原配置。没有进程重启接口，重启由管理员在服务外部执行。

统计对象为**已提交至 CUPS**（`lp` 成功返回）的任务，包含目录和上传来源。失败、取消、排队及执行中的任务均不计入；失败后重试成功只按同一任务计一次。时间按 CUPS 接受后记录的 `submitted_at`，而非上传或创建时间；以服务进程本地时区划分今天及之前 6 个自然日，按日期升序返回全部 7 天，空日文件数/页数为 0。浏览器位于其他时区时仍显示服务日期，不自行重新分桶。跨午夜的任务归到接受日。

每次提交前对实际传给 `lp` 的 PDF 调用 `/usr/bin/pdfinfo`；DOC/DOCX 和图片使用本次 LibreOffice 转换产物，而非原始文件名或 CUPS 作业默认页数。页数指生成 PDF 的文档页数，不乘份数，不等于双面/拼版后的纸张数，也不确认实际出纸。计数命令沿用受限输出和进程组超时机制，最多 30 秒或配置的较短超时。

SQLite 自动增加 nullable `jobs.page_count`（正整数）和 `jobs.submitted_at` 字段；新任务仅在 `lp` 成功后，将状态、响应、页数和接受时间一次更新落盘。旧的 submitted 记录原本不会再修改，故用其 `updated_at` 作为原接受时间补入 `submitted_at`；旧页数保留 `NULL`。不自动重跑历史 Word 转换或推测页数：旧转换产物已清理，当前字体/转换版本也可能不同，无法保证可靠回填。升级前按既有停服备份流程保存数据库与 spool。

若缺少 pdfinfo、超时、PDF 损坏/加密等导致无法读取页数，任务仍按原打印流程执行，计数错误记日志；CUPS 接受后，文件数照常计入、页数保留未知。该记录不会仅因计数失败变成可重试的打印失败，避免诱发重复打印。图表对不完整页数显示 `?` 或已知下限 `≥N`，摘要和逐日文本明确未知文件数；历史页数显示“未知”。真实空日才显示 0，不把未知冒充 0。图表具备 accessible label、文本摘要及可展开逐日数据，窄屏以每行 4 天排列。

### 统计 API

`GET /api/stats/weekly` 沿用 Host 白名单和只读接口的保护。响应例（`days` 总是 7 条，此处只展示一天）：

```json
{
  "metric": "submitted_to_cups",
  "timezone": "service_local",
  "generated_at": "2026-10-25T10:00:00+08:00",
  "days": [{
    "date": "2026-10-25",
    "submitted_files": 2,
    "known_pages": 5,
    "unknown_page_files": 1,
    "pages": null
  }]
}
```

`known_pages` 是可确认的部分和；只有 `unknown_page_files=0` 时 `pages` 才是完整总数，否则为 `null`。无提交时二者为 0、`pages=0`。`GET /api/jobs` 同时返回每项的 `page_count`、`submitted_at`（未成功提交为 null）。最近一周按服务当前本地时区解释时间戳；更改服务时区会改变日期桶。CUPS 接受与本地落盘之间仍存在原有的崩溃不确定窗口，结果不明的失败不计入已确认提交。

## 数据、日志与访问控制

`--data-dir` 是专用数据目录，会设为当前用户可访问的 `0700` 权限，不要指向家目录或项目根目录。内含 `lanprint.sqlite3`（及 WAL/SHM）、`service.lock`、`spool/`。源快照用于审计、重试及持久化去重，**暂不自动清理**；请为文档预留磁盘空间，服务停止后统一备份数据库和 spool。不要单独删除数据库，否则现有源文件可能重新入队；删除快照会令对应重试失败。强制终止可能留下未引用的转换临时目录/快照。目录访问/快照失败在设置页监控状态和日志中显示，转换/打印失败在任务历史中显示。

日志输出到 stdout/stderr，`RUST_LOG=lanprint=debug` 可调整级别。日志、数据库和 spool 包含文件名、路径、文档或错误信息，应仅供授权服务用户访问。

Web 默认绑定 `127.0.0.1:8787`，也支持显式 `--bind '[::1]:8787'` 或其他回环端口。需要局域网访问时，只允许明确指定的 RFC1918 私有 IPv4 地址（`10.0.0.0/8`、`172.16.0.0/12`、`192.168.0.0/16`）；拒绝 `0.0.0.0`、`[::]`、公网、链路本地、组播及其他非回环 IPv6 地址。绑定地址须属于本机网卡。

接口继续精确检查 Host（监听 IP 加端口，保留 `localhost:端口`），写操作要求 `X-Lanprint-Request: 1`，提供 Origin 时必须与 Host 的 HTTP 来源一致；不启用 CORS。页面输出使用 `textContent`，配有 CSP。这些检查不能替代身份认证。MVP **没有账户认证，使用 HTTP 明文传输**：任何能连接该端口的设备都能查看配置、历史及路径、修改配置、上传文件并发起打印；网络中的攻击者可能窃听或篡改传输的文档和管理操作。本机其他用户/进程也可访问。应使用专用低权限账户并仅接收可信文档；LibreOffice 不是文档安全沙箱。

如确需局域网访问，请先确认网络隔离与风险。将下面的示例地址替换为分配给本机网卡的 RFC1918 私有 IPv4 地址：

```bash
./target/release/lanprint --data-dir "$HOME/.local/share/lanprint" --bind 192.168.1.20:8787
```

请仅在接受无登录、明文风险的可信局域网启用，并用网络隔离或防火墙将可达客户端限制到该可信范围。绑定一个私有网卡地址不会自动限制客户端来源子网，经路由或 VPN 能到达该地址的设备仍可能访问；不要配置公网端口转发。DHCP 改变主机地址后，必须同步更新启动参数/服务模板并重新部署，否则绑定会失败；可配置 DHCP 地址保留以保持固定地址。

跨越可信局域网的远程管理继续使用回环绑定及经过 SSH 认证的隧道：

```bash
ssh -N -L 8787:127.0.0.1:8787 user@linux-host
```

随后访问本机 <http://127.0.0.1:8787>。不提供公网反向代理部署模式。

## systemd 用户服务

提供 [deploy/lanprint.service](deploy/lanprint.service)，默认仅绑定回环地址 `127.0.0.1:8787`。如需局域网访问，请先确认可信网络边界和上述无认证、HTTP 明文风险，再将模板中的 bind 地址改为本机明确指定的 RFC1918 私有 IPv4 地址，并通过网络隔离或防火墙限制可达客户端：

```bash
install -Dm755 target/release/lanprint "$HOME/.local/bin/lanprint"
install -Dm644 deploy/lanprint.service "$HOME/.config/systemd/user/lanprint.service"
systemctl --user daemon-reload
systemctl --user enable --now lanprint
journalctl --user -u lanprint -f
```

需要注销后运行时，由系统管理员按部署需求设置 `loginctl enable-linger 用户名`。服务自动重启后，目录任务仍须在 Web 点击“启动监控”；已接受的上传请求会自动处理。停止服务用 `systemctl --user stop lanprint`。

## API 与验证

无前端构建步骤，HTML/JS/CSS 编译进 Rust 二进制。API：`GET/PUT /api/config`、`GET /api/status`、`POST /api/monitor/start`、`POST /api/monitor/pause`、`GET /api/jobs?limit=50&offset=0`、`GET /api/stats/weekly`、`POST /api/jobs/{id}/retry?confirm=checked_queue`。写操作需 `X-Lanprint-Request: 1` 和 JSON Content-Type（配置请求）。历史每页最多 200 条；每个任务包含 `origin=watch|upload` 来源字段。

`GET /api/status` 还返回 `max_upload_bytes=52428800`。`busy=true` 表示一次工作周期尚未结束，包含扫描/快照；此时 `runtime.active_job=null` 表示尚未选取打印任务。`POST /api/monitor/pause` 立即关闭目录监控，响应区分未入队快照、已入队目录任务和自动执行的上传任务；须等当前扫描/快照或打印退出（`busy=false`）才能再次保存或启动。监控仍开启时 `PUT /api/config` 返回 400；已暂停但仍忙碌时保存返回 409，忙碌时启动也返回 409，错误信息明确要求等待扫描或打印结束。`GET/PUT /api/config` 返回 `{"config":已保存值或null,"active_config":启动时活动值或null,"restart_required":布尔值}`，不再返回 `cancelled_jobs`。`GET /api/status` 同样返回 `active_config`、`restart_required`；`configured` 表示当前进程有活动配置，而非仅在数据库保存过。待重启标志由保存值与活动值比较得出，重启加载后自然清除。正确切换顺序是：pause → 等待 `monitoring=false` 且 `busy=false` → 保存新配置 → 管理员停止并重启服务 → 确认活动配置和 `restart_required=false` → 按需 start。重启后监控默认暂停，已有上传仍自动按原配置执行；旧版本已取消的历史任务维持取消状态。

```bash
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
node --check web/app.js
node --check web/settings.js
# 可选真实浏览器验证（需先安装 Python Playwright 及 Chromium）
python3 tests/mobile.py target/release/lanprint /tmp/lanprint-mobile-evidence
```

测试需 Python 3（标准库）及 `/usr/bin/pdfinfo`（真实 PDF 页数验证），不要求 CUPS 或 LibreOffice。单元和端到端测试覆盖配置、监控、历史、上传边界、文件安全、队列恢复、重试和访问限制；浏览器脚本覆盖手机与桌面视口。运行验证命令后可在本机查看结果。安装 LibreOffice 后，可另运行 `python3 tests/real_office.py target/release/lanprint`，用真实 DOCX/二进制 DOC 验证转换；打印端仍使用模拟 `lp`，不会接触打印机。

代码：`src/model.rs` 配置与文件类型；`src/watcher.rs` 稳定检测与快照；`src/store.rs` SQLite；`src/printing.rs` 转换与 CUPS；`src/web.rs` API/访问限制；`src/lib.rs` 队列工作线程；`web/` 页面；`tests/` 集成验证。

## 开源参考

2026-10-25 调研 [apirkle/zpl-autoprint](https://github.com/apirkle/zpl-autoprint)，MIT，参考版本 [`8ddcbf2`](https://github.com/apirkle/zpl-autoprint/tree/8ddcbf25a2ed3267004e510bcd78cb8203797df0)。该项目通过 `inotifywait` 监听 `close_write`/`moved_to`，短暂等待后用 `lp` 提交 ZPL，以会话内 inode/大小/mtime 指纹抑制重复，并提供 systemd 用户服务。参考点是使用标准 CUPS 路径、处理浏览器多次文件事件以及提供低权限用户服务的部署方式。

本项目独立实现 Rust 服务，使用轮询以兼容无需 inotify 工具的部署，加入 Web 配置、PDF/Word 转换、持久化内容指纹、稳定快照、历史及人工重试。不同于参考项目仅处理运行期间新增的 ZPL，本项目会处理启动扫描时已有文件，并保留历史指纹跨重启去重。未复制其实现代码。
