// Pas de console supplémentaire sous Windows en version finale.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    #[cfg(target_os = "linux")]
    linux_gpu_workaround();
    tsuki_lib::run()
}

/// WebKitGTK n'affiche rien (fenêtre vide) avec son rendu DMA-BUF sur les cartes
/// graphiques virtuelles (VMware, VirtualBox, QEMU/virtio) et avec le pilote NVIDIA
/// propriétaire. On le désactive alors, sauf si l'utilisateur a déjà choisi.
#[cfg(target_os = "linux")]
fn linux_gpu_workaround() {
    const VAR: &str = "WEBKIT_DISABLE_DMABUF_RENDERER";
    if std::env::var_os(VAR).is_some() {
        return;
    }
    // 15ad VMware, 80ee VirtualBox, 1af4 virtio, 1234 QEMU, 1414 Hyper-V, 10de NVIDIA
    const VENDORS: [&str; 6] = ["0x15ad", "0x80ee", "0x1af4", "0x1234", "0x1414", "0x10de"];
    let risky = std::fs::read_dir("/sys/class/drm")
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| std::fs::read_to_string(e.path().join("device/vendor")).ok())
        .any(|v| VENDORS.contains(&v.trim()));
    if risky {
        std::env::set_var(VAR, "1");
    }
}
