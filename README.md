# 私有 GitHub 发布清单

本目录已整理好本次可直接上传到私有 GitHub 仓库或私有 Release 的资料。

## 文档
- `软件介绍文档.zh-CN.md`
  用于仓库首页、Release 说明或对外产品介绍，不展开技术实现细节。
- `GitHub仓库首页文案.zh-CN.md`
  可直接作为私有 GitHub 仓库首页 README 的主体内容。
- `GitHub Release 文案.zh-CN.md`
  可直接作为 GitHub Release 页面说明。
- `世界包开发文档.zh-CN.md`
  用于世界包作者和内部内容团队，说明目录结构、文件职责、字段用途与导入约束。

## 安装包
- `云朵梦境_0.1.0_windows_x64_zh-CN.msi`
  Windows MSI 安装包，适合标准安装分发。
- `云朵梦境_0.1.0_windows_x64_setup.exe`
  Windows EXE 安装包，适合直接发给终端用户。
- `云朵梦境_0.1.0_android_universal_debugsigned_20260523.apk`
  Android 通用 APK，来自 2026-05-23 当天最新构建结果。

## 说明
- Windows 安装包已于 2026-05-23 重新构建。
- Android 安装包为 `universal release` 产物，并使用调试签名二次签名，文件名中的 `debugsigned` 为真实状态，不是生产证书签名包。
- 世界包开发文档基于当前代码中的真实导入、导出与运行逻辑编写。
