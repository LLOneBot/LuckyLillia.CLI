# LLBot CLI

LLBot 命令行启动器

## 功能

- 两种启动模式，默认 **PMHQ**：
  - **PMHQ 模式（默认）**：CLI 同时拉起 PMHQ（attach QQ）和 LLBot 两个子进程，LLBot 经 `--pmhq-port` 连 PMHQ 通信
  - **headless 模式**（加 `--headless`）：不启动 PMHQ，直接拉起 LLBot，由 LLBot 直连 QQ 协议
- headless 模式下登录二维码由 LLBot 自身打印到终端
- 登录成功后通过命名管道 (IPC) 从 LLBot 获取 QQ 号 / 昵称，并设置窗口标题（仅 Windows）
- 进程生命周期管理（Ctrl+C 自动清理）

## 目录结构

```
LuckyLillia.exe          # Windows; Linux / macOS 为 LuckyLillia
bin/
  llbot/                 # 必需 (headless / PMHQ 模式都用)
    node(.exe)
    llbot.js
  pmhq/                  # 默认 (PMHQ 模式) 需要; --headless 不下载/不使用
    pmhq(.exe)
    pmhq_config.json
```

> 组件 (PMHQ / Node.js / LLBot) 缺失时会在启动时自动下载；headless 模式不会下载 PMHQ。

> **Windows 首次启动会自我改名**：exe 名是分发名（含 `llbot` 或 `LuckyLillia` 但不等于 `LuckyLillia.exe`，
> 例如 release 下载到的 `LuckyLillia-win-x64.exe`、旧版的 `llbot-win-x64.exe`）时，会改名为
> `LuckyLillia.exe` 并以新名字重新启动，使任务管理器与开机自启项显示品牌名。
> 改名只发生一次（名字已是 `LuckyLillia.exe` 时直接跳过，该判断同时是防重启死循环的闸）；
> 用户自己改成的其它名字不会被动；若无写权限（如装在 `Program Files` 且非管理员）或同名文件
> 被占用，则跳过改名、按原名正常启动。

> **构建 / 分发产物名**：`cargo build` 产出 `LuckyLillia(.exe)`，GitHub Release 的下载文件为
> `LuckyLillia-win-x64.exe` / `LuckyLillia-linux-x64` / `LuckyLillia-linux-arm64` / `LuckyLillia-macos-arm64`。
> npm 包（`llbot-cli-<os>-<arch>`）内的文件名仍是 `llbot(.exe)`：已发布旧版本的 `--update` 只认这个名字
> （见 `src/updater.rs` 的 `find_exe_in_dir`），改掉会断掉它们的自更新链。

## 命令行参数

所有参数都是可选的。CLI 自身识别 `--headless` / `--pmhq` / `--no-headless` 切换模式（这几个 flag 不透传给子进程）；其余参数：headless 模式下只把 `--qq` / `--protocol` / `--cdn` / `--debug` 透传给 LLBot（其它参数忽略，带值的统一转成 `--xxx=value` 形式），PMHQ 模式下全部透传给 PMHQ，其中 `--debug` / `--cdn` 会同时传给 LLBot。

| 参数 | 说明 |
|------|------|
| `--headless` | 切换为 headless 直连模式（默认 PMHQ） |
| `--pmhq`, `--no-headless` | 显式指定 PMHQ 模式（默认，兼容旧用法） |
| `--qq=<number>` | 快速登录 QQ 号（两种模式均生效） |
| `--debug` | 调试模式（两种模式均生效；PMHQ 模式下 PMHQ 和 LLBot 同时开启） |
| `--cdn=<cf\|china>` | 接入点 CDN，默认 `cf`（两种模式均生效） |
| `--update` | 检查并执行更新 |
| `--help, -h` | 显示帮助信息 |
| `--version, -v` | 显示版本信息 |

以下参数仅在 **headless 模式** 下生效（透传给 LLBot）：

| 参数 | 说明 |
|------|------|
| `--protocol=<name>`, `-p` | 登录协议：`linux`（默认）/ `windows` / `macos` / `watch` |

以下参数仅在 **PMHQ 模式** 下生效（透传给 PMHQ）：

| 参数 | 说明 |
|------|------|
| `--qq-path=<path>` | QQ 可执行文件路径 |
| `--qq-console` | 启用 QQ 控制台日志 |
| `--debug-pb[=true/false]` | 显示 send/recv Protobuf 日志 |
| `--work-dir=<path>` | 工作目录 |
| `--no-exit-with-qq` | 禁用 QQ 退出时自动退出（默认启用） |
| `--qq-exit-delay=<秒>` | QQ 退出后的缓冲时间（默认 15 秒，0 为立即退出） |

## 使用示例

```bash
# 直接启动（PMHQ 模式，默认：启动 PMHQ + attach QQ）
./LuckyLillia

# 快速登录
./LuckyLillia --qq=123456789

# 指定 QQ 路径
./LuckyLillia --qq-path="/opt/QQ/qq"

# headless 直连（不启动 PMHQ，LLBot 直连 QQ 协议）
./LuckyLillia --headless

# 检查更新
./LuckyLillia --update
```

## 支持平台

- Windows x64
- Linux x64
- Linux arm64

> 进程模型见 [docs/process-model.md](docs/process-model.md)。

> 登录信息 (uin / 昵称) 的命名管道 IPC 仅 Windows 生效，用于登录后回填并设置窗口标题；其它平台依赖 LLBot 自身的终端输出（二维码、登录状态）。
