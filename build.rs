use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    // out_dir is something like target/debug/build/orbit-pdf-xxx/out
    // target_dir is target/debug
    let target_dir = out_dir.parent().unwrap().parent().unwrap().parent().unwrap();

    let pdfium_url = "https://github.com/bblanchon/pdfium-binaries/releases/latest/download/pdfium-linux-x64.tgz";
    let tar_path = out_dir.join("pdfium.tgz");

    if !tar_path.exists() {
        println!("cargo:warning=Downloading pdfium binary...");
        Command::new("curl")
            .args(&["-L", "-o", tar_path.to_str().unwrap(), pdfium_url])
            .status()
            .expect("Failed to download pdfium");

        Command::new("tar")
            .args(&["-xzf", tar_path.to_str().unwrap(), "-C", out_dir.to_str().unwrap()])
            .status()
            .expect("Failed to extract pdfium");
    }

    let lib_name = "libpdfium.so";
    let src_lib = out_dir.join("lib").join(lib_name);
    let dest_lib = target_dir.join(lib_name);
    
    if src_lib.exists() {
        // Copy to target directory so pdfium-render can find it at runtime
        std::fs::copy(&src_lib, &dest_lib).unwrap_or_else(|e| {
            println!("cargo:warning=Failed to copy libpdfium.so: {}", e);
            0
        });
    }

    // Also tell cargo to re-run this script only if build.rs changes
    println!("cargo:rerun-if-changed=build.rs");
}
