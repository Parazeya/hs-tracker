fn main() {
    // Windows links the vendored Npcap SDK import libs, and delay-loads
    // wpcap.dll so the app starts and explains itself when Npcap is missing.
    // Elsewhere pcap is plain libpcap and the linker needs nothing from us.
    // The build script itself always runs on the host, so the target platform
    // has to be read from the environment rather than from cfg!.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let sdk = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("npcap-sdk/Lib/x64");
        println!("cargo:rustc-link-search=native={}", sdk.display());
        println!("cargo:rustc-link-arg=/DELAYLOAD:wpcap.dll");
        println!("cargo:rustc-link-lib=delayimp");
    }
    tauri_build::build()
}
