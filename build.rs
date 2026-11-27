use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn find_rc_exe() -> Option<PathBuf> {
    if let Ok(out) = Command::new("where").arg("rc.exe").output() {
        if out.status.success() {
            if let Some(first) = String::from_utf8_lossy(&out.stdout).lines().next() {
                let p = PathBuf::from(first.trim());
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    let kits_root = Path::new(r"C:\Program Files (x86)\Windows Kits\10\bin");
    if let Ok(entries) = std::fs::read_dir(kits_root) {
        let mut versions: Vec<PathBuf> = entries
            .filter_map(|e| e.ok().map(|x| x.path().join("x64").join("rc.exe")))
            .filter(|p| p.exists())
            .collect();
        versions.sort();
        if let Some(latest) = versions.pop() {
            return Some(latest);
        }
    }
    None
}

fn main() {
    println!("cargo:rerun-if-changed=assets/gpucap.rc");
    println!("cargo:rerun-if-changed=assets/gpucap.ico");
    println!("cargo:rerun-if-changed=assets/gpucap-256.png");

    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let rc_file = Path::new("assets/gpucap.rc");
    if !rc_file.exists() {
        return;
    }

    let Some(rc_exe) = find_rc_exe() else {
        println!("cargo:warning=rc.exe not found; skipping Windows icon embedding");
        return;
    };

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let res_path = out_dir.join("gpucap.res");
    let status = Command::new(&rc_exe)
        .args(["/nologo", "/fo"])
        .arg(&res_path)
        .arg(rc_file)
        .status();

    match status {
        Ok(st) if st.success() && res_path.exists() => {
            println!("cargo:rustc-link-arg-bins={}", res_path.display());
        }
        Ok(st) => {
            println!("cargo:warning=rc.exe exited with status {st}");
        }
        Err(e) => {
            println!("cargo:warning=failed to run rc.exe: {e}");
        }
    }
}
