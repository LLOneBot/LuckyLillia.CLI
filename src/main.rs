//! LLBot CLI - 启动器

#[cfg(target_os = "windows")]
mod llbot_ipc;
mod updater;
mod migrate;
mod qq;
mod util;
mod windows_job;

use command_group::{CommandGroup, GroupChild};
use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const DEFAULT_PORT: u16 = 13000;
const PORT_RANGE_END: u16 = 14000;

// CLI 自身消费的模式切换 flag, 不透传给子进程.
const MODE_FLAGS: [&str; 3] = ["--headless", "--pmhq", "--no-headless"];

// 取选项值, 支持 <name>=<v> 和 <name> <v> 两种写法, names 可带别名 (如 ["--protocol", "-p"]).
// 名字精确匹配, --qq 不会误吃 --qq-path; 空格写法的下一个参数以 "-" 开头时视为缺值, 不吞别的选项.
fn extract_opt_value(args: &[String], names: &[&str]) -> Option<String> {
    for (i, a) in args.iter().enumerate() {
        for &name in names {
            match a.strip_prefix(name) {
                Some("") => {
                    if let Some(v) = args.get(i + 1).filter(|v| !v.starts_with('-')) {
                        return Some(v.clone());
                    }
                }
                Some(rest) => {
                    if let Some(v) = rest.strip_prefix('=') {
                        return Some(v.to_string());
                    }
                }
                None => {}
            }
        }
    }
    None
}

fn print_help() {
    println!("LuckyLillia {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("LLBot 命令行启动器");
    println!();
    println!("用法:");
    println!("  LuckyLillia [选项]");
    println!();
    println!("模式:");
    println!("  默认 PMHQ       启动 PMHQ attach QQ, LLBot 经 PMHQ 通信");
    println!("  --headless      不启动 PMHQ, 直接拉起 LLBot 直连 QQ 协议, 二维码由 LLBot 打印到终端");
    println!();
    println!("选项:");
    println!("  --headless              切换为 headless 直连模式 (默认 PMHQ)");
    println!("  --pmhq, --no-headless   显式指定 PMHQ 模式 (默认, 兼容旧用法)");
    println!("  --qq=<number>           快速登录 QQ 号 (两种模式均生效)");
    println!("  --debug                 调试模式 (两种模式均生效)");
    println!("  --cdn=<cf|china>        接入点 CDN, 默认 cf (两种模式均生效)");
    println!("  --update                检查并执行组件更新 (CLI / PMHQ / LLBot)");
    println!("  --help, -h              显示此帮助");
    println!("  --version, -v           显示版本");
    println!();
    println!("headless 模式额外参数 (透传给 LLBot, 其它参数忽略):");
    println!("  --protocol=<name>, -p   登录协议: linux (默认) / windows / macos / watch");
    println!();
    println!("PMHQ 模式额外参数 (透传给 PMHQ):");
    println!("  --qq-path=<path>        QQ 可执行文件路径");
    println!("  --qq-console            启用 QQ 控制台日志");
    println!("  --work-dir=<path>       工作目录");
    println!("  --no-exit-with-qq       禁用 QQ 退出时自动退出 (默认启用)");
    println!("  --qq-exit-delay=<秒>    QQ 退出后缓冲时间 (默认 15 秒)");
    println!();
    println!("示例:");
    println!("  LuckyLillia                 PMHQ 模式 (默认)");
    println!("  LuckyLillia --qq=123456789  快速登录");
    println!("  LuckyLillia --headless      headless 直连");
    println!("  LuckyLillia --update        检查更新");
}

fn main() {
    let exe_dir = env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."));

    let args: Vec<String> = env::args().skip(1).collect();

    // 分发名 (llbot.exe / llbot-win-x64.exe ...) 统一改成品牌名后以新名字重启; 命中才动, 失败按原名继续.
    #[cfg(target_os = "windows")]
    util::normalize_process_name(&args);

    // --update 检查并执行更新（不依赖 pmhq 已安装）
    if args.iter().any(|a| a == "--update") {
        updater::run_update(&exe_dir);
        util::wait_exit(0);
    }

    // --help / --version: 只依赖 CLI 自身, 不触发组件下载/不依赖 pmhq
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        std::process::exit(0);
    }
    if args.iter().any(|a| a == "--version" || a == "-v") {
        println!("LuckyLillia {}", env!("CARGO_PKG_VERSION"));
        std::process::exit(0);
    }

    // 模式: 默认 PMHQ (pmhq attach QQ, llbot 走 --pmhq-port);
    //       --headless 切到直连模式 (LLBot 直连 QQ 协议, 不启动 pmhq).
    // --pmhq / --no-headless 仍受理, 显式指定默认模式; 与 --headless 同时给出时以它们为准.
    let headless = args.iter().any(|a| a == "--headless")
        && !args.iter().any(|a| a == "--pmhq" || a == "--no-headless");

    // 启动时自动补齐必要组件（缺失才下载; headless 不下 pmhq）
    if let Err(e) = updater::ensure_required_components(&exe_dir, headless) {
        eprintln!("自动安装组件失败: {}", e);
        util::wait_exit(1);
    }

    migrate::migrate_old_files(&exe_dir);

    let llbot_dir = exe_dir.join("bin/llbot");

    // Auth Token 两种模式均必须存在; 放在 migrate 之后, 旧 data 目录可能刚被迁入
    let auth_token = match util::read_auth_token(&exe_dir) {
        Some(token) => token,
        None => {
            eprintln!(
                "错误: 没有 Auth Token ({} 不存在或内容为空)",
                llbot_dir.join("data/auth_token.txt").display()
            );
            eprintln!("请到 https://auth.luckylillia.com 获取");
            util::wait_exit(1);
        }
    };

    // 查找可用的 node（本地目录 -> 系统 PATH -> 自动下载）
    let node_path = match util::find_usable_node(&exe_dir) {
        Some(path) => path,
        None => {
            println!("未找到可用的 Node.js (需要版本 >= 24)");
            println!("正在自动下载 Node.js...");
            match updater::download_and_install_node(&exe_dir) {
                Ok(path) => path,
                Err(e) => {
                    eprintln!("下载 Node.js 失败: {}", e);
                    eprintln!("请手动安装 Node.js >= 24 或将其放置到 bin/llbot/ 目录");
                    util::wait_exit(1);
                }
            }
        }
    };

    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(metadata) = std::fs::metadata(&node_path) {
            let mut perms = metadata.permissions();
            if perms.mode() & 0o111 == 0 {
                perms.set_mode(perms.mode() | 0o755);
                if let Err(e) = std::fs::set_permissions(&node_path, perms) {
                    eprintln!("警告: 设置 node 执行权限失败: {}", e);
                }
            }
        }
    }
    if !llbot_dir.join("llbot.js").exists() {
        eprintln!(
            "错误: 未找到 llbot.js: {}",
            llbot_dir.join("llbot.js").display()
        );
        util::wait_exit(1);
    }

    // Windows: 生成命名管道名. LLBot 据此 listen, CLI 作为 client 轮询 get_login_state 拿 uin/nickname
    // (PMHQ 已不再提供 getSelfInfo). 两种模式下 LLBot 都是 CLI 的直接子进程, env 直接设在 node 上.
    #[cfg(target_os = "windows")]
    let ipc_pipe_name = llbot_ipc::generate_pipe_name();
    #[cfg(not(target_os = "windows"))]
    let ipc_pipe_name = String::new();

    println!("LLBot CLI 启动器");
    println!("================");

    let debug = args.iter().any(|a| a == "--debug");

    // 按模式构建待启动进程: headless 只有 LLBot; PMHQ 模式下 CLI 自己拉起 pmhq 和 LLBot 两个子进程.
    // 不需要等 pmhq 就绪再启动 LLBot -- LLBot 连不上 pmhq 会按 1s 间隔无限重连 (llbot.js
    // connectWebSocket 里 onerror/onclose 都走 reconnect), 顺序 spawn 即可.
    let mut specs: Vec<(Command, &'static str)> = Vec::new();

    if headless {
        println!("模式: headless (LLBot 直连)");
        println!();

        let mut cmd = llbot_command(&node_path, &llbot_dir, &ipc_pipe_name);
        // 只透传 LLBot 认的参数, 其余丢弃. 带值的统一转成 --xxx=value, 空格写法的值才不会被漏掉.
        if let Some(uin) = extract_opt_value(&args, &["--qq"]) {
            println!("快速登录 QQ: {}", uin);
            cmd.arg(format!("--qq={}", uin));
        }
        if let Some(protocol) = extract_opt_value(&args, &["--protocol", "-p"]) {
            cmd.arg(format!("--protocol={}", protocol));
        }
        if let Some(cdn) = extract_opt_value(&args, &["--cdn"]) {
            cmd.arg(format!("--cdn={}", cdn));
        }
        if debug {
            cmd.arg("--debug");
        }
        specs.push((cmd, "LLBot"));
    } else {
        let pmhq_exe = match util::find_pmhq_exe(&exe_dir) {
            Some(path) => path,
            None => {
                eprintln!("错误: 未找到 pmhq 可执行文件");
                eprintln!("请确保 bin/pmhq/ 目录下存在 pmhq 或 pmhq-<platform>-<arch> 文件");
                util::wait_exit(1);
            }
        };

        let detected_qq_path = qq::detect_qq_path(&exe_dir, &args);

        let port = util::find_available_port(DEFAULT_PORT, PORT_RANGE_END).unwrap_or_else(|| {
            eprintln!("错误: 无法找到可用端口 ({}-{})", DEFAULT_PORT, PORT_RANGE_END);
            util::wait_exit(1);
        });

        println!("模式: PMHQ");
        println!("端口: {}", port);
        println!();

        let mut pmhq_cmd = Command::new(&pmhq_exe);
        pmhq_cmd.env("NODE_SKIP_PLATFORM_CHECK", "1");
        pmhq_cmd.arg("--port").arg(port.to_string());
        pmhq_cmd.arg(format!("--auth-token={}", auth_token));

        if let Some(ref qq_path) = detected_qq_path {
            println!("检测到 QQ 路径: {}", qq_path);
            pmhq_cmd.arg(format!("--qq-path={}", qq_path));
        }

        // 透传用户参数 (去掉 CLI 自身的模式控制 flag)
        for a in args.iter().filter(|a| !MODE_FLAGS.contains(&a.as_str())) {
            pmhq_cmd.arg(a);
        }

        pmhq_cmd.arg("--exit-with-qq-delay").arg("15");

        // --pmhq-port 是 LLBot 走 PMHQ 模式而非直连的开关 (llbot.js getPmhqTarget).
        // --qq 不给 LLBot: PMHQ 模式的快速登录归 pmhq 管, 已随用户参数透传过去.
        let mut llbot_cmd = llbot_command(&node_path, &llbot_dir, &ipc_pipe_name);
        llbot_cmd.arg(format!("--pmhq-port={}", port));
        if let Some(cdn) = extract_opt_value(&args, &["--cdn"]) {
            llbot_cmd.arg(format!("--cdn={}", cdn));
        }
        if debug {
            llbot_cmd.arg("--debug");
        }

        specs.push((pmhq_cmd, "PMHQ"));
        specs.push((llbot_cmd, "LLBot"));
    }

    let procs: Procs = Arc::new(Mutex::new(Vec::new()));

    // 信号处理先注册再 spawn: 启动过程中 Ctrl+C 也能清掉已经起来的那个.
    let procs_for_handler = procs.clone();
    ctrlc::set_handler(move || {
        kill_all(&procs_for_handler);
        std::process::exit(0);
    })
    .ok();

    // 处理 SIGHUP（终端关闭）和 SIGTERM，确保子进程被清理
    #[cfg(unix)]
    {
        use signal_hook::consts::{SIGHUP, SIGTERM};
        use signal_hook::iterator::Signals;

        let procs_for_signal = procs.clone();
        let mut signals = Signals::new(&[SIGHUP, SIGTERM]).expect("无法注册信号处理");
        thread::spawn(move || {
            for _sig in signals.forever() {
                kill_all(&procs_for_signal);
                std::process::exit(0);
            }
        });
    }

    for (mut cmd, label) in specs {
        let mut child: GroupChild = match cmd
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .group_spawn()
        {
            Ok(child) => child,
            Err(e) => {
                eprintln!("启动 {} 失败: {}", label, e);
                kill_all(&procs);
                util::wait_exit(1);
            }
        };

        #[cfg(target_os = "windows")]
        if let Err(e) = windows_job::assign_to_job_object(&mut child) {
            eprintln!("警告: 无法设置 {} 进程保护: {}", label, e);
        }

        forward_lines(child.inner().stdout.take(), false);
        forward_lines(child.inner().stderr.take(), true);

        match procs.lock() {
            Ok(mut guard) => guard.push((label, child)),
            Err(_) => {
                let _ = child.kill();
                eprintln!("内部错误: 进程表不可用");
                util::wait_exit(1);
            }
        }
    }

    // 登录信息 (uin/昵称) 经 LLBot 命名管道获取并设置窗口标题; 二维码由 LLBot 自身打印到终端.
    #[cfg(target_os = "windows")]
    llbot_ipc::start_login_listener(ipc_pipe_name);

    // 等待子进程结束. 任一个退出就清掉其余的并跟着退 -- PMHQ 模式下两个进程是一套,
    // 少了任何一个都没法继续干活 (pmhq 没了 LLBot 连不上, LLBot 没了没人收消息).
    let exit_code = loop {
        thread::sleep(Duration::from_millis(100));

        let mut guard = match procs.lock() {
            Ok(guard) => guard,
            Err(_) => break 1,
        };
        if guard.is_empty() {
            break 1;
        }

        let mut exited: Option<(usize, i32)> = None;
        for (i, (label, c)) in guard.iter_mut().enumerate() {
            match c.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        eprintln!("{} 退出，状态码: {:?}", label, status.code());
                    }
                    exited = Some((i, status.code().unwrap_or(1)));
                    break;
                }
                Ok(None) => {}
                Err(e) => {
                    eprintln!("等待 {} 失败: {}", label, e);
                    exited = Some((i, 1));
                    break;
                }
            }
        }

        if let Some((exited_idx, code)) = exited {
            for (i, (label, c)) in guard.iter_mut().enumerate() {
                if i != exited_idx {
                    println!("正在关闭 {}...", label);
                    let _ = c.kill();
                }
            }
            break code;
        }
    };

    std::process::exit(exit_code);
}

/// 已启动的子进程 (label, child): PMHQ 模式下是 pmhq + LLBot, headless 只有 LLBot.
/// 信号 handler 和主等待循环共享它, 所以进 Arc<Mutex<_>>.
type Procs = Arc<Mutex<Vec<(&'static str, GroupChild)>>>;

fn kill_all(procs: &Procs) {
    if let Ok(mut guard) = procs.lock() {
        for (_, c) in guard.iter_mut() {
            let _ = c.kill();
        }
    }
}

/// node llbot.js 的公共部分 (cwd / env / node 自身参数); 两种模式只在后面追加的 LLBot 参数上不同.
fn llbot_command(node_path: &Path, llbot_dir: &Path, ipc_pipe_name: &str) -> Command {
    let mut cmd = Command::new(node_path);
    // llbot.js 是相对路径, 靠这个 cwd 解析 (原先 PMHQ 模式下由 pmhq 的 --sub-cmd-workdir 负责).
    cmd.current_dir(llbot_dir);
    cmd.env("NODE_SKIP_PLATFORM_CHECK", "1");
    #[cfg(target_os = "windows")]
    cmd.env("LL_IPC_PIPE", ipc_pipe_name);
    #[cfg(not(target_os = "windows"))]
    let _ = ipc_pipe_name;
    cmd.arg("--disable-warning=ExperimentalWarning")
        .arg("--enable-source-maps")
        .arg("llbot.js");
    cmd
}

/// 转发子进程的一路输出到 CLI 自己的 stdout/stderr. 不加进程名前缀 --
/// LLBot 的登录二维码是 ASCII art, 行首插前缀会撑坏排版.
fn forward_lines<R: std::io::Read + Send + 'static>(src: Option<R>, to_stderr: bool) {
    let src = match src {
        Some(src) => src,
        None => return,
    };
    thread::spawn(move || {
        use std::io::{BufRead, BufReader};
        let reader = BufReader::new(src);
        for line in reader.lines() {
            // 跳过非 UTF-8 的行继续转发 (Windows 下子进程可能吐 GBK 字节).
            // 不能换成 map_while(Result::ok) -- 那会在第一个坏行上停掉整条管道, 之后的日志全丢.
            let Ok(line) = line else { continue };
            if to_stderr {
                eprintln!("{}", line);
            } else {
                println!("{}", line);
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::extract_opt_value;

    fn get(args: &[&str], names: &[&str]) -> Option<String> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        extract_opt_value(&args, names)
    }

    #[test]
    fn both_forms_and_alias() {
        let p = &["--protocol", "-p"];
        assert_eq!(get(&["--protocol=windows"], p).as_deref(), Some("windows"));
        assert_eq!(get(&["--protocol", "macos"], p).as_deref(), Some("macos"));
        assert_eq!(get(&["-p=watch"], p).as_deref(), Some("watch"));
        assert_eq!(get(&["--debug", "-p", "win"], p).as_deref(), Some("win"));
        assert_eq!(get(&["--debug"], p), None);
    }

    #[test]
    fn exact_name_and_no_option_swallow() {
        assert_eq!(get(&["--qq-path=/opt/QQ/qq"], &["--qq"]), None);
        assert_eq!(get(&["--qq-path", "x", "--qq", "123"], &["--qq"]).as_deref(), Some("123"));
        assert_eq!(get(&["--cdn", "--debug"], &["--cdn"]), None);
        assert_eq!(get(&["--cdn"], &["--cdn"]), None);
    }
}
