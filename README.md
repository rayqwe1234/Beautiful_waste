# 美丽的废物 · Beautiful Waste

> 一个只负责好看的 Windows 氛围时钟。没有生产力，只有时间、光和留白。

![Beautiful Waste screenshot](assets/screenshot.png)

Beautiful Waste 是用 Rust 原生构建的桌面待机应用。WGPU 着色器绘制持续流动的模糊光雾，Glyphon 负责文字排版；不依赖 Electron、WebView 或额外运行时。

## 功能

- 动态、柔和且无明显边缘的迷幻光雾
- 中央时钟、日期、星期与轮换短句
- 五种可切换的时钟字体
- 动画速度与中央时钟大小调节
- 秒数与短句显示开关
- 从左侧滑出的设置菜单
- Windows 系统媒体控制：上一首、播放／暂停、下一首
- 右上角独占全屏；按 `Esc` 退出
- 自动保存偏好设置，启动后恢复

## 下载与运行

从 [Releases](../../releases) 下载 `Beautiful-Waste-v1.0.0-windows-x64.zip`，解压后直接运行 `beautiful-waste.exe`。

支持 Windows 10/11，并需要可用的 DirectX 12 或 Vulkan 图形驱动。

设置保存在：`%APPDATA%\Beautiful Waste\settings.ini`。

## 开发

需要 Rust stable：

```powershell
cargo run
```

构建优化后的单文件 Windows 程序：

```powershell
cargo build --release
```

输出位于 `target/release/beautiful-waste.exe`。应用图标会嵌入 EXE，无需附带其他运行文件。

## 项目结构

```text
src/main.rs       # 窗口、排版、交互、设置与系统媒体控制
src/visual.wgsl   # 光雾、噪点、控件和自绘图标着色器
assets/           # README 展示图片
build.rs          # Windows 图标与版本资源
icon.ico          # 应用图标
```

## 许可证

[MIT](LICENSE) © 2026 沈承睿
