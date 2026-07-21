fn main() {
    // Load .env file for build-time environment variables
    if let Ok(path) = dotenvy::dotenv() {
        println!("cargo:rerun-if-changed={}", path.display());
    }

    // screencapturekit 1.4.2 requires macOS 14.0+ for SCScreenshotConfiguration
    // Set deployment target to ensure proper symbol linking
    #[cfg(target_os = "macos")]
    {
        println!("cargo:rustc-env=MACOSX_DEPLOYMENT_TARGET=12.3");
    }
    tauri_build::build()
}
