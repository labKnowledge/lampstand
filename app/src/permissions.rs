#[cfg(target_os = "macos")]
mod mac {
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightScreenCaptureAccess() -> bool;
        fn CGRequestScreenCaptureAccess() -> bool;
    }

    pub fn screen_allowed() -> bool {
        unsafe { CGPreflightScreenCaptureAccess() }
    }

    pub fn ask_screen() -> bool {
        unsafe { CGRequestScreenCaptureAccess() }
    }
}

pub const BUNDLE_ID: &str = "app.lampstand.live";

pub fn screen_allowed() -> Option<bool> {
    #[cfg(target_os = "macos")]
    {
        Some(mac::screen_allowed())
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

pub fn ask_screen() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::ask_screen()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn reset_screen_permission() -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let out = std::process::Command::new("tccutil").args(["reset", "ScreenCapture", BUNDLE_ID]).output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

pub fn app_bundle() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let mut p = exe.as_path();
    while let Some(parent) = p.parent() {
        if p.extension().map(|e| e == "app").unwrap_or(false) {
            return Some(p.to_path_buf());
        }
        p = parent;
    }
    None
}

pub fn relaunch() -> bool {
    if let Some(bundle) = app_bundle() {
        return std::process::Command::new("open").arg("-n").arg(bundle).spawn().is_ok();
    }
    if let Ok(exe) = std::env::current_exe() {
        return std::process::Command::new(exe).spawn().is_ok();
    }
    false
}
