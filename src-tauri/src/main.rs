// 发布版使用 Windows 子系统，避免启动时弹出黑色命令行窗口；
// 调试版保留控制台以便查看日志。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    dream_narrative_engine_lib::run()
}
