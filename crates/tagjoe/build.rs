//! Windows: embed the program icon (packaging/windows/tagjoe.rc, compiled with rc.exe from the
//! Windows SDK, which comes with the MSVC build tools). Without rc.exe the program is built
//! without an icon.

fn main() {
    let rc = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packaging/windows/tagjoe.rc");
    println!("cargo:rerun-if-changed={}", rc.display());
    println!("cargo:rerun-if-changed={}", rc.with_extension("ico").display());
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("tagjoe-icon.res");
    let Some(rc_exe) = find_rc() else {
        println!("cargo:warning=rc.exe not found (Windows SDK): program built without an icon");
        return;
    };
    let ok = std::process::Command::new(&rc_exe).arg("/nologo").arg("/fo").arg(&out).arg(&rc).status().is_ok_and(|s| s.success());
    if ok {
        println!("cargo:rustc-link-arg-bins={}", out.display());
    } else {
        println!("cargo:warning=rc.exe failed: program built without an icon");
    }
}

/// `rc.exe` from PATH, else the newest Windows 10/11 SDK under Program Files (x86).
fn find_rc() -> Option<std::path::PathBuf> {
    if let Some(path) = std::env::var_os("PATH") {
        if let Some(p) = std::env::split_paths(&path).map(|d| d.join("rc.exe")).find(|p| p.is_file()) {
            return Some(p);
        }
    }
    let pf = std::env::var_os("ProgramFiles(x86)").map(std::path::PathBuf::from)?;
    let bin = pf.join("Windows Kits").join("10").join("bin");
    let mut versions: Vec<std::path::PathBuf> = std::fs::read_dir(&bin).ok()?.flatten().map(|e| e.path()).filter(|p| p.join("x64").join("rc.exe").is_file()).collect();
    versions.sort();
    versions.pop().map(|v| v.join("x64").join("rc.exe"))
}
