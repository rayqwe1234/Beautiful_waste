fn main() {
    println!("cargo:rerun-if-changed=icon.ico");

    #[cfg(target_os = "windows")]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("icon.ico");
        resource.set("ProductName", "Beautiful Waste");
        resource.set("FileDescription", "美丽的废物 · Beautiful Waste");
        resource.set("LegalCopyright", "Copyright © 2026 沈承睿");
        resource
            .compile()
            .expect("failed to embed Windows resources");
    }
}
