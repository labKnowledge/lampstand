fn main() {
    println!("cargo:rerun-if-changed=macos/Info.plist");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        let plist = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("macos").join("Info.plist");
        println!("cargo:rustc-link-arg-bins=-Wl,-sectcreate,__TEXT,__info_plist,{}", plist.display());
    }
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=windows/lampstand.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_icon("windows/lampstand.ico");
        res.set("ProductName", "Lampstand");
        res.set("FileDescription", "Lampstand: church live and projection");
        res.set("CompanyName", "Eligapris");
        res.compile().expect("embed the Windows icon");
    }
}
