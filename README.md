# Meister

A macOS desktop application built with Tauri 2, React, and TypeScript.

## Prerequisites

- [Node.js](https://nodejs.org/) (v18+)
- [Rust](https://www.rust-lang.org/tools/install)
- Xcode Command Line Tools (`xcode-select --install`)

## Setup

```bash
npm install
```

## Development

Run the app in development mode with hot reload:

```bash
npm run tauri dev
```

## Build for macOS

Build the production `.app` bundle and `.dmg` installer:

```bash
npm run tauri build
```

Output locations:

- **App bundle:** `src-tauri/target/release/bundle/macos/Meister.app`
- **DMG installer:** `src-tauri/target/release/bundle/dmg/`

### Launch the app

```bash
open src-tauri/target/release/bundle/macos/Meister.app
```

Or open the `.dmg` and drag Meister to your Applications folder.

> **Note:** The app is not code-signed by default. On first launch you may need to right-click the app and select **Open** to bypass Gatekeeper.
