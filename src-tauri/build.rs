fn main() {
    // tauri-build 默认把 comctl32 v6 manifest 只嵌入 bin 目标（rustc-link-arg-bins），
    // cargo test 的 lib 测试二进制拿不到 manifest，运行时加载 comctl32 v5，
    // TaskDialogIndirect 入口缺失（0xc0000139），全部测试无法启动。
    // 因此这里让 tauri-build 不内嵌 manifest，改由 app-manifest.rc 经
    // cargo:rustc-link-arg 链入所有产物（bin + 测试二进制），内容与默认一致。
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(
        tauri_build::WindowsAttributes::new_without_app_manifest(),
    ))
    .expect("tauri build");

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let _ = embed_resource::compile_for_everything("app-manifest.rc", embed_resource::NONE);
    }
}
