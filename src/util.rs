use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 获取 node 版本号（返回主版本号，如 24）
pub fn get_node_version(node_path: &Path) -> Option<u32> {
    let output = Command::new(node_path)
        .arg("--version")
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let version_str = String::from_utf8_lossy(&output.stdout);
    // 版本格式: v24.0.0
    let version = version_str.trim().trim_start_matches('v');
    let major: u32 = version.split('.').next()?.parse().ok()?;
    Some(major)
}

/// 从系统 PATH 中查找 node
pub fn find_node_in_path() -> Option<PathBuf> {
    let node_name = get_exe_name("node");

    #[cfg(target_os = "windows")]
    {
        // Windows 使用 where 命令
        if let Ok(output) = Command::new("where").arg(&node_name).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                if let Some(first_line) = stdout.lines().next() {
                    let path = PathBuf::from(first_line.trim());
                    if path.exists() {
                        return Some(path);
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Unix 使用 which 命令
        if let Ok(output) = Command::new("which").arg(&node_name).output() {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let path = PathBuf::from(stdout.trim());
                if path.exists() {
                    return Some(path);
                }
            }
        }
    }

    None
}

/// 查找可用的 node（优先本地目录，其次系统 PATH，需要版本 >= 24）
pub fn find_usable_node(exe_dir: &Path) -> Option<PathBuf> {
    let llbot_dir = exe_dir.join("bin/llbot");
    let node_exe = get_exe_name("node");
    let local_node = llbot_dir.join(&node_exe);

    // 1. 先检查本地目录的 node
    if local_node.exists() {
        if let Some(version) = get_node_version(&local_node) {
            if version >= 24 {
                return Some(local_node);
            }
            println!("本地 node 版本 {} 过低（需要 >= 24）", version);
        }
    }

    // 2. 检查系统 PATH 中的 node
    if let Some(system_node) = find_node_in_path() {
        if let Some(version) = get_node_version(&system_node) {
            if version >= 24 {
                println!("使用系统 node: {} (v{})", system_node.display(), version);
                return Some(system_node);
            }
            println!("系统 node 版本 {} 过低（需要 >= 24）", version);
        }
    }

    None
}

pub fn get_exe_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{}.exe", base)
    } else {
        base.to_string()
    }
}

pub fn find_pmhq_exe(exe_dir: &Path) -> Option<PathBuf> {
    let pmhq_dir = exe_dir.join("bin/pmhq");

    let platform_arch = if cfg!(target_os = "windows") {
        "win-x64"
    } else if cfg!(target_os = "linux") {
        if cfg!(target_arch = "x86_64") {
            "linux-x64"
        } else if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            ""
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-arm64"
        } else {
            "macos-x64"
        }
    } else {
        ""
    };

    if !platform_arch.is_empty() {
        let arch_specific = pmhq_dir.join(get_exe_name(&format!("pmhq-{}", platform_arch)));
        if arch_specific.exists() {
            return Some(arch_specific);
        }
    }

    let generic = pmhq_dir.join(get_exe_name("pmhq"));
    if generic.exists() {
        return Some(generic);
    }

    None
}

pub fn find_available_port(start: u16, end: u16) -> Option<u16> {
    for port in start..end {
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Some(port);
        }
    }
    None
}

/// 读取 Auth Token (bin/llbot/data/auth_token.txt); 文件不存在或内容为空返回 None
pub fn read_auth_token(exe_dir: &Path) -> Option<String> {
    let path = exe_dir.join("bin/llbot/data/auth_token.txt");
    let content = std::fs::read_to_string(path).ok()?;
    let token = content.trim();
    if token.is_empty() {
        None
    } else {
        Some(token.to_string())
    }
}

pub fn wait_exit(code: i32) -> ! {
    println!("\n按任意键退出...");
    let _ = std::io::stdin().read_line(&mut String::new());
    std::process::exit(code);
}

/// 归一化后的进程名 (品牌名), 与 Desktop 一致; 任务管理器/自启项显示品牌名而非 llbot.
#[cfg(target_os = "windows")]
pub const BRAND_EXE_NAME: &str = "LuckyLillia.exe";

/// Windows 下把进程名统一成 [`BRAND_EXE_NAME`]: 运行中的 exe 允许 rename (不允许 delete),
/// 同卷改名只改元数据、不中断当前执行; 改完以新名字重启、原进程退出.
/// 仅在当前名含 "llbot" 时动作 -- 该判断同时是防重启死循环的闸 (品牌名不含 llbot).
/// 任何一步失败都静默回退、按原名继续启动, 不阻断.
#[cfg(target_os = "windows")]
pub fn normalize_process_name(args: &[String]) {
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(_) => return,
    };
    let dir = match exe_path.parent() {
        Some(d) => d,
        None => return,
    };
    let cur_name = match exe_path.file_name().and_then(|n| n.to_str()) {
        Some(n) => n,
        None => return,
    };
    if !cur_name.to_ascii_lowercase().contains("llbot") {
        return;
    }

    let target = dir.join(BRAND_EXE_NAME);
    // 目标已存在 (上次改名残留): 未被占用才删; 删不掉多半是另一实例在跑, 放弃改名.
    if target.exists() && std::fs::remove_file(&target).is_err() {
        return;
    }
    // 改名失败 (如装在 Program Files 且非管理员, 无写权限): 不改名, 原名继续.
    if std::fs::rename(&exe_path, &target).is_err() {
        return;
    }

    println!("已重命名为 {}, 正在重新启动...", BRAND_EXE_NAME);
    // 重启失败: 文件已是品牌名, 下次启动即生效; 本次仍以当前进程跑完, 不阻断.
    if Command::new(&target)
        .args(args)
        .current_dir(dir)
        .spawn()
        .is_ok()
    {
        std::process::exit(0);
    }
}
