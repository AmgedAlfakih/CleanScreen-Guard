fn main() {
    println!("cargo:rerun-if-changed=assets/app_icon.ico");
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/app_icon.ico");
        res.set("ProductName", "CleanScreen Guard");
        res.set("FileDescription", "CleanScreen Guard — Real-Time Desktop NSFW Blur Overlay");
        res.set("CompanyName", "CleanScreen Guard Project");
        res.set("LegalCopyright", "Open Source (AGPL-3.0)");
        res.set("OriginalFilename", "CleanScreenGuard.exe");
        res.set("InternalName", "CleanScreenGuard");
        if let Err(e) = res.compile() {
            println!("cargo:warning=winresource compile skipped: {e}");
        }
    }
}
