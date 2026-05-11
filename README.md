# ⚡ ATM Auto Runner

A modern, cross-platform desktop application built with **Tauri v2** and **Rust** to automate the Samsung ATM (Auto Test Module) testing sequence. Designed for industrial-scale firmware certification with parallel multi-device execution and real-time monitoring.

![ATM Auto Runner Banner](https://raw.githubusercontent.com/endrisusanto/ATM-Bersama/main/src/assets/logo.png)

## ✨ Key Features

- **🚀 Parallel Multi-Device Execution**: Run ATM test sequences (BVT, SVT, SDT, etc.) on multiple Android devices simultaneously via ADB.
- **🛡️ Industrial UI/UX**: Professional "Gray Dark Mode" design system with sharp aesthetics and high-contrast feedback.
- **📊 Real-time Dashboard**: Live status tracking for every test case with background-fill progress indicators.
- **🔍 Deep Device Info**: Automatically fetches model info, Android version, and Samsung-specific **PDA** build information.
- **📝 Live Console**: Integrated log streaming with severity filtering (Info, Success, Warning, Error).
- **🔄 Auto-Update**: Built-in tools update management via integrated modal view.
- **📂 Smart Configuration**: Easy ATM folder selection and automated test discovery from `TestInfo.xml`.

## 🛠️ Tech Stack

- **Backend**: Rust (Tokio, Serde, Regex)
- **Frontend**: Vanilla JavaScript, HTML5, CSS3 (Gray Design System)
- **Framework**: Tauri v2
- **Dev Tooling**: Vite, GitHub Actions (Automated CI/CD)

## 📦 Installation & Build

### Prerequisites
- [Rust](https://rustup.rs/) (Stable)
- [Node.js](https://nodejs.org/) (LTS)
- [ADB](https://developer.android.com/tools/adb) (In system PATH)
- Java Runtime (For executing ATM .jar files)

### Local Development
```bash
# Clone the repository
git clone https://github.com/endrisusanto/ATM-Bersama.git
cd ATM-Bersama

# Install dependencies
npm install

# Run in development mode
npm run tauri dev
```

### Build for Production
```bash
npm run tauri build
```

## 🚀 GitHub Actions CI/CD
This project uses GitHub Actions to automatically build and release the application. Simply push a tag starting with `v` (e.g., `v1.0.0`) to trigger a new release.

## 📄 License
Internal Samsung Testing Tool - All Rights Reserved.

---
Built with ❤️ by [Endri Susanto](https://github.com/endrisusanto)
