fn main() {
    #[cfg(target_os = "windows")]
    {
        // 静态链接 VC 运行时
        static_vcruntime::metabuild();
        
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        // 任务管理器显示的是 FileDescription, 不是 exe 文件名; winres 默认把它和 ProductName
        // 都填成 Cargo package name (llbot-cli), 不显式覆盖进程在任务管理器里就叫 llbot-cli.
        res.set("ProductName", "LuckyLillia");
        res.set("FileDescription", "LuckyLillia");
        res.set("InternalName", "LuckyLillia");
        res.set("OriginalFilename", "LuckyLillia.exe");
        res.compile().unwrap();
    }
}
