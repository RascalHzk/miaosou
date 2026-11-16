fn main() {
    // 窗口/托盘图标是编译期由 `tauri::generate_context!` 读进二进制的
    // （tauri-build 只跟踪 tauri.conf.json，不跟踪图标文件）。
    // 这里显式声明依赖，避免换了图标后 cargo 认为无需重编、任务栏仍显示旧图标。
    for icon in [
        "icons/icon.png",
        "icons/32x32.png",
        "icons/128x128.png",
        "icons/128x128@2x.png",
        "icons/icon.icns",
        "icons/icon.ico",
    ] {
        println!("cargo:rerun-if-changed={icon}");
    }

    tauri_build::build()
}
