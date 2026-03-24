fn main() {
    #[cfg(target_os = "windows")]
    {
        let mut res = winresource::WindowsResource::new();
        res.set("FileDescription", "rhap");
        res.set("ProductName", "rhap");
        res.set("InternalName", "rhap");
        res.compile().unwrap();
    }
}
