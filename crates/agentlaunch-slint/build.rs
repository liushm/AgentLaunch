fn main() {
    slint_build::compile("ui/app.slint").expect("failed to compile Slint UI");

    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../assets/icon.ico");
        res.set("ProductName", "AgentLaunch");
        res.set("FileDescription", "AgentLaunch - Per-Agent Proxy Launcher");
        res.compile().expect("failed to embed Windows icon resource");
    }
}
