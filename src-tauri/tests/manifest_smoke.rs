//! 集成测试占位。
//!
//! 存在意义：cargo 要求包内至少有一个 `[[test]]` 目标，才接受 build.rs 发出的
//! `cargo:rustc-link-arg-tests` 指令（把 comctl32 v6 manifest 链入测试二进制）。
//! 没有它，Windows 上全部测试进程会在启动时崩溃（0xc0000139）。

#[test]
fn manifest_linkage_smoke() {
    // 能运行到这里，说明测试二进制已成功带 manifest 启动。
    assert!(true);
}
