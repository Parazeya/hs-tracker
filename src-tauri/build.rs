fn main() {
    // Windows links the vendored Npcap SDK import libs, and delay-loads
    // wpcap.dll so the app starts and explains itself when Npcap is missing.
    // Elsewhere pcap is plain libpcap and the linker needs nothing from us.
    // The build script itself always runs on the host, so the target platform
    // has to be read from the environment rather than from cfg!.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        // Read when the build script RUNS, not baked in when it was compiled.
        // `env!` fixes the checkout the script binary was first built in, and
        // that binary is cached and reused: built once for a second copy of the
        // repository sharing this target directory, it went on pointing the
        // linker at that copy after it was deleted, and every build failed on
        // a wpcap.lib that was sitting right here.
        let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
        let sdk = std::path::Path::new(&manifest).join("npcap-sdk/Lib/x64");
        println!("cargo:rustc-link-search=native={}", sdk.display());
        println!("cargo:rustc-link-arg=/DELAYLOAD:wpcap.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
    tauri_build::build()
}
