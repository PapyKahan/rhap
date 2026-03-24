fn main() {
    if std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default() == "windows" {
        #[cfg(target_os = "windows")]
        {
            let mut res = winresource::WindowsResource::new();
            res.set("FileDescription", "rhap");
            res.set("ProductName", "rhap");
            res.set("InternalName", "rhap");
            res.compile().unwrap();
        }
    }
}
