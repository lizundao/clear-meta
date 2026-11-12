# 元数据管理

PDF、Office 与图片元数据的桌面管理工具（Tauri + React + Rust）。

## 支持格式

- **PDF**: `.pdf`
- **Office (OOXML)**: `.docx`, `.xlsx`, `.pptx`, `.docm`, `.xlsm`, `.pptm`
- **图片**: `.jpg`, `.jpeg`, `.png`, `.webp`

> 暂不支持旧版 Office（`.doc` / `.xls` / `.ppt`）及 TIFF/HEIC。

## 开发

```bash
npm install
npm run tauri dev
```

## 下载

正式安装包见 [GitHub Releases](https://github.com/lizundao/clear-meta/releases)，或从 [官网](https://inkdot.cn) 下载：

| 平台 | 文件 |
|------|------|
| Windows 安装版 | `ClearMeta-0.1.0-setup-win-x64.exe` |
| Windows 便携版 | `ClearMeta-0.1.0-portable-win-x64.zip`（解压即用） |
| macOS（Apple Silicon） | `ClearMeta-0.1.0-aarch64.dmg` / `.app.zip` |

## 构建

```bash
npm run tauri build                    # 三平台安装包（按当前系统）
npm run package:portable               # 另打 Windows 便携版 zip 到 release/
npm run typecheck                      # 类型检查
npm run test:regression                # 前端纯函数回归
cargo test --manifest-path src-tauri/Cargo.toml --lib   # Rust 文件处理回归
```

Tauri 原始产物在 `src-tauri/target/release/bundle/`；发布用的 ASCII 命名产物由 `.github/workflows/build.yml` 在打 tag 时自动构建并上传草稿 Release。产物统一 ASCII 命名，避免中文路径在下载站 / CI / 杀软白名单中的兼容问题（中文展示名放在窗口标题与 NSIS 显示名）。

> NSIS 为免管理员的 `currentUser` 模式；WebView2 采用内置引导器，离线环境也能安装。macOS 为未签名构建，首次打开请右键 →「打开」。

## 功能说明

- 读取并预览 PDF / Office / 图片元数据
- **选择性清除**：按字段勾选要删除的元数据
- **字段编辑**：修改作者、标题、公司等常用属性
- **快速预设**：投标脱敏、隐私清除、全部清除
- 处理前后对比、文件夹批量处理
- 支持导出 CSV / JSON 处理报告
- 支持覆盖原文件或另存到新目录
- 深色 / 浅色主题切换

## 许可

采用 MIT 开源许可，详见根目录 [LICENSE](LICENSE) 与 [中文译本](LICENSE.zh-CN.md)。

## 安全边界

- 新文件处理只作用于你主动选择并登记的文件，路径经规范化校验，不越界、不跟随符号链接
- 目标文件已存在时自动改名，绝不静默覆盖；「覆盖原文件」会先自动备份，可在结果里撤销
- 单次批量上限 500 个文件；Office 解压有大小与压缩比上限，防解压炸弹
- 所有处理在本机完成，不上传任何文件
