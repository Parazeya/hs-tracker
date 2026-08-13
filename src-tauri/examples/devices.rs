// On Windows the crate delay-loads wpcap.dll and an example has to ask for the
// helper itself; elsewhere libpcap links like any other library.
#[cfg(windows)]
#[link(name = "delayimp")]
extern "C" {}

// diagnostic: which adapters the capture library offers and what addresses they
// carry
fn main() {
    // Npcap installs beside the system DLLs or into a folder of its own, and
    // the loader has to be pointed at it before the first pcap call
    #[cfg(windows)]
    {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let dir = std::path::PathBuf::from(root).join("System32").join("Npcap");
        let path = std::env::var("PATH").unwrap_or_default();
        std::env::set_var("PATH", format!("{};{}", dir.display(), path));
    }
    for d in pcap::Device::list().expect("device list") {
        let addrs: Vec<String> = d.addresses.iter().map(|a| a.addr.to_string()).collect();
        println!(
            "{:?}
    name: {}
    addresses: {}",
            d.desc,
            d.name,
            addrs.join(", ")
        );
    }
}
