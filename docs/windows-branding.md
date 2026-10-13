# Windows 品牌名

进程在任务管理器、文件属性、`--version` 里显示成什么，由三套互不相干的机制决定。改一处不会带动另外两处。

## 三个来源

| 显示位置 | 来源 | 在哪里改 |
|----------|------|----------|
| 任务管理器"进程"页名称列、文件属性的"文件说明" | PE 版本资源 `FileDescription` | `build.rs` 的 `res.set(...)` |
| 任务管理器"详细信息"页名称列 | exe 文件名 | `[[bin]] name` + `util::normalize_process_name` 的自我改名 |
| `--version` / `--help` 首行 | `src/main.rs` 里的硬编码字符串 | 同处 |

## winres 的默认值会漏

`winres::WindowsResource::new()` 默认用 **Cargo package name**（`llbot-cli`）填 `FileDescription` 和
`ProductName`，不是用 `description` 字段；`InternalName` / `OriginalFilename` 它**留空**。

所以 2.1.3 及之前的产物在任务管理器里显示 `llbot-cli`，哪怕文件名已经被改成 `LuckyLillia.exe`。
`build.rs` 现在四个字段全部显式设成品牌名。

不靠改 `[package] name` 解决的原因：

- winres 只从 package name 取两个字段，`InternalName` / `OriginalFilename` 仍要显式设，等于两处都得动
- 显式 set 不依赖 winres 的默认行为，以后谁动了 package name 也不会悄悄改掉任务管理器里的显示
- package name 和 npm 包名 `llbot-cli-<os>-<arch>` 现在是对齐的，而 npm 包名**不能改**
  （已发布旧版本的 `--update` 认它，见 `src/updater.rs` 的 `cli_package`）

`src/updater.rs` 里的 `llbot-cli-*` 字符串是 npm 包名和临时目录名，与显示无关，不要跟着改。
CI 里的 `llbot-cli-*`（`release.yml` 的 `npm_pkg` / artifact 名）同理，也不读 Cargo package name。

## 排查陷阱: 任务管理器按路径缓存显示名

**任务管理器在自己启动时解析 exe 路径 -> 显示名并缓存，之后同路径的新进程直接复用。**
改了版本资源重新构建后，开着的任务管理器仍然显示旧名字，看起来像是改动没生效。

实测时间线（2.1.4 开发时踩的）：任务管理器 10:29:42 启动，当时磁盘上的 exe 还是 `llbot-cli`；
10:38 重新构建成 `LuckyLillia`，10:41 启动的新进程在那个任务管理器里**仍然显示 `llbot-cli`**。

**关掉任务管理器重开**即可。只影响开发机，最终用户是全新路径没有缓存。

不依赖任务管理器确认改动是否生效，用这三条（第一条最硬，读的是进程实际加载的映像）：

```powershell
(Get-Process -Id <PID>).MainModule.FileVersionInfo | Format-List FileDescription,ProductName,FileVersion
(Get-Item 'target\release\LuckyLillia.exe').VersionInfo | Format-List FileDescription,ProductName
# PE 里是否还残留旧名 (版本资源是 UTF-16LE, 所以要按宽字符搜)
python -c "d=open(r'target\release\LuckyLillia.exe','rb').read(); print(d.count('llbot-cli'.encode('utf-16-le')))"
```

最后一条返回 0 就说明版本资源里没有旧名了；ASCII 搜 `llbot-cli` 会命中 `updater.rs` 的 npm 包名
字符串，那是正常的，别误判。

排查时还确认过几个**不是**元凶的地方，下次可以跳过：PE manifest（这个 exe 没有）、
指向 exe 的快捷方式、注册表 Run 自启项、`MuiCache` 的 `FriendlyAppName`
（`HKCU\Software\Classes\Local Settings\Software\Microsoft\Windows\Shell\MuiCache`，
里面只有已不存在的 `llbot.exe` 路径的残留条目）。

## exe 自我改名

`util::normalize_process_name`（Windows only）把分发名改成 `LuckyLillia.exe` 并以新名字重启，
细节见该函数的文档注释和 README。它改的是**文件名**，跟版本资源是两件事 —— 文件名对了，
任务管理器"进程"页照样可能显示 `llbot-cli`，因为那一列读的是 `FileDescription`。
