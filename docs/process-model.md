# 进程模型

CLI 怎么拉起 PMHQ / LLBot，两者的生命周期怎么联动，以及 LLBot 侧对 CLI 的契约。

## 进程树

两种模式下 LLBot 都是 CLI 的**直接子进程**：

```
LuckyLillia.exe (CLI)
├── pmhq-win-x64.exe        # 仅 PMHQ 模式
│   └── QQ.exe              # pmhq 自己 attach / 拉起
└── node.exe llbot.js       # 两种模式都有
```

2.1.2 及之前 PMHQ 模式走的是 pmhq 的 `--sub-cmd`：CLI 只 spawn pmhq，把 `--sub-cmd-workdir`
+ `--sub-cmd <node> ... llbot.js -- --pmhq-port=<port>` 交给 pmhq，由 pmhq 把 LLBot 作为
自己的子进程（CLI 的孙进程）拉起。改成 CLI 直管两个进程的原因：

- LLBot 崩掉不必连带重启 QQ（孙进程模式下基本只能整棵树重来，QQ 重登成本高）
- `LL_IPC_PIPE` 不用跨两级继承
- 不依赖 pmhq 的 `--sub-cmd` 实现；那是另一个仓库的特性，且分发的 pmhq 是打包压缩的，
  字符串都 grep 不出来，行为无法从本仓库验证

## 启动顺序: 不需要等 pmhq 就绪

顺序 spawn 即可，**不用探测端口或解析 pmhq stdout**。LLBot 连 pmhq 的 WebSocket 自带
无限重连，`onerror` / `onclose` 都走 `reconnect()`，固定 1 秒退避（`llbot.js`
`connectWebSocket`）：

```js
const reconnect = () => {
  this.ws = void 0;
  if (this.reconnectTimer) clearTimeout(this.reconnectTimer);
  this.reconnectTimer = setTimeout(() => { this.connectWebSocket() }, 1e3);
};
```

连不上时它打的是 "PMHQ WebSocket 连接错误，可能 QQ 未启动，正在等待 QQ 启动后重新连接..."
—— LLBot 本来就按"可能比 pmhq/QQ 先起来"设计。CLI 仍按 pmhq -> LLBot 的顺序 spawn，
正常情况第一次就连上，不打这行 error。

`Command::group_spawn()` 本身不阻塞（创建进程后立即返回 `GroupChild`），所以两个进程
在主线程顺序 spawn 就行，不需要为 spawn 开线程。pmhq 是长驻前台进程、不会自己退出，
这跟 spawn 调用是否阻塞是两件事。线程只用在 I/O 转发上：每进程 stdout/stderr 各一个。

## LLBot 侧参数契约

`llbot.js` 的 `getPmhqTarget(process.argv)` 自己扫 argv，不走通用 arg parser：

| 参数 | 行为 |
|------|------|
| `--pmhq-port <p>` / `--pmhq-port=<p>` | 出现即进 PMHQ 模式；缺值时用默认 `13000` |
| `--pmhq-host <h>` / `--pmhq-host=<h>` | 默认 `127.0.0.1`。CLI 目前没用到，意味着可以连远程 pmhq |
| 完全没有 `--pmhq-port` | 返回 null，走直连 (headless) 模式 |

所以 `--pmhq-port` 是模式开关本身，不只是端口号。

## CLI 的参数分流

| 参数 | PMHQ 模式 | headless 模式 |
|------|-----------|---------------|
| `--qq` | 只给 pmhq（快速登录归它管） | 给 LLBot |
| `--protocol` / `-p` | 不适用 | 给 LLBot |
| `--cdn` | 给 pmhq（随用户参数透传）**和** LLBot | 给 LLBot |
| `--debug` | 给 pmhq（随用户参数透传）**和** LLBot | 给 LLBot |
| `--qq-path` 等 pmhq 参数 | 只给 pmhq | 忽略 |
| `--headless` / `--pmhq` / `--no-headless` | CLI 自己消费，不透传 | 同左 |

PMHQ 模式下用户参数是整包透传给 pmhq 的（过滤掉 `MODE_FLAGS`）；给 LLBot 的那份是
按上表挑出来单独拼的。

## 生命周期

- 两个 child 存在 `Procs = Arc<Mutex<Vec<(&'static str, GroupChild)>>>`，信号 handler
  和主等待循环共享
- 信号 handler 在 spawn **之前**注册，启动过程中 Ctrl+C 也能清掉已起来的那个
- 主循环 100ms 轮询所有 child 的 `try_wait()`；**任一个退出就 kill 其余的并跟着退**，
  退出码取先退的那个。PMHQ 模式下两个进程是一套，少任何一个都没法干活
- QQ 退出的正常路径：pmhq 按 `--exit-with-qq-delay 15` 等 15 秒退出 -> CLI 发现 pmhq
  退出 -> kill LLBot -> CLI 退出
- `src/windows_job.rs` 的 `assign_to_job_object` 每次调用都新建一个 job 并**故意不
  CloseHandle**：`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` 要靠 handle 保持打开，CLI 进程退出
  时 OS 关 handle 才触发 kill。所以两个进程各调一次即可，不用改成一个 job 装两个；
  Windows 8+ 的嵌套 job 跟 pmhq 自己的 job、`command_group` 的 job 都不冲突

## 输出转发

`forward_lines` 不加 `[PMHQ]` / `[LLBot]` 进程名前缀：headless 模式下 LLBot 会把登录
二维码当 ASCII art 打到终端，行首插前缀会撑坏排版。PMHQ 模式下不打二维码（QQ 有界面），
但两种模式共用这个转发函数。

改成 CLI 直管后 LLBot 日志少了 pmhq 转发这一层，输出样子跟 2.1.2 及之前不同。

## 登录信息 IPC (仅 Windows)

CLI 生成管道名 -> 经 env `LL_IPC_PIPE` 交给 LLBot（现在是直接子进程，一级继承）->
LLBot 作为 server listen -> CLI 作为 client 轮询 `get_login_state` 拿 uin / nickname ->
设置控制台标题。协议和 state 取值见 `src/llbot_ipc.rs` 头部注释。

PMHQ 已不再提供 `getSelfInfo`，所以登录信息只能从 LLBot 这条管道拿。
