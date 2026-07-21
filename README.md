<div align="center">
  <img src="public/logo.png" alt="Logia Logo" width="120" />
  <h1>Logia</h1>
</div>

> A powerful, AI-enhanced notes application built with Tauri and React, designed for technical content creators and knowledge workers.

Logia blends structured text editing with infinite visual canvases, ensuring your ideas are captured exactly how you envision them. With local-first performance and optional cloud capabilities, it is the perfect companion for your second brain.

## Features

- **AI-Powered Assistance**: Integrated with Google AI for intelligent note generation and editing.
- **Rich Text Editing**: Full-featured editor with BlockNote for structured content creation.
- **Visual Note-Taking**: Integrated **tldraw** canvas for infinite sketching, diagrams, and whiteboarding.
- **Audio Recording & Transcription**: Record and transcribe audio content directly in your notes.
- **File Management**: Hierarchical file tree with drag-and-drop organization.
- **Trash & Recovery**: Safely delete notes and folders with a 14-day recovery window.
- **Git Sync**: Optional git-based synchronization — connect any git remote to backup and sync your notes.
- **Search & Discovery**: Fast, fuzzy search across all your notes.
- **Kanban Boards**: Organize your tasks and workflows with visual boards.
- **Cross-Platform**: Native desktop app for Windows, macOS, and Linux.
- **Markdown Support**: Full markdown rendering with GitHub Flavored Markdown.

## Installation

### Prerequisites

- [Bun](https://bun.com/)
- [Rust](https://rustup.rs/) (latest stable)
- [Tauri CLI](https://tauri.app/v1/guides/getting-started/prerequisites)

### Setup

1. Clone the repository:
   ```bash
   git clone https://github.com/daschinmoy21/Logia.git
   cd logia
   ```

2. Install dependencies:
   ```bash
   bun install
   ```

3. Run in development mode:
   ```bash
   bun run tauri dev
   ```

4. Build for production:
   ```bash
   bun tauri build
   ```

   > **Note for Linux Users**: There is a known issue with linuxdeploy in Tauri which may cause build errors with AppImage creation. usage:
   > ```bash
   > NO_STRIP=true bun tauri build
   > ```

### NixOS / Nix Installation

Logia is available as a Nix flake for easy installation on NixOS or any system with Nix.

**Run without installing:**
```bash
nix run github:daschinmoy21/Logia
```

**Install to user profile:**
```bash
nix profile install github:daschinmoy21/Logia
```

**Add to NixOS configuration:**

1. Add to your flake inputs:
   ```nix
   inputs.logia = {
     url = "github:daschinmoy21/Logia";
     inputs.nixpkgs.follows = "nixpkgs";
   };
   ```

2. Pass it through `specialArgs` and use in your config:
   ```nix
   environment.systemPackages = [
     logia.packages.${pkgs.system}.default
   ];
   ```

#### Git Sync

Logia uses git for note synchronization. To enable sync:

1. Create a private git repository (GitHub, GitLab, etc.)
2. Open Logia Settings → Git Sync
3. Enter your remote URL (e.g., `git@github.com:user/logia-notes.git`) and branch name
4. Click **Connect Remote**
5. Use **Sync Now** to push/pull changes

**Authentication**: Logia uses your system's git credentials — SSH keys, `gh auth`, or git credential helper. No cloud OAuth tokens are stored.

**Development shell:**
```bash
nix develop  # Enters a shell with all dependencies
bun install
bun tauri dev
```

## Usage

- **Creating Notes**: Use the file tree sidebar to create new notes or organize existing ones.
- **AI Assistance**: Access AI features through the AI sidebar for content generation and editing.
- **Drawing**: Click the drawing tool to open the infinite canvas for visual notes.
- **Recording**: Use the recording feature to capture audio and automatically transcribe it.
- **Search**: Press `Alt+P` or `Meta/Cmd+P` (Mac) to open the command palette and search across all content.
- **Sync**: Configure a git remote in Settings and use Sync Now to push/pull your notes.

## Configuration

Create a `.env` file in the root directory with your API keys (optional, you can also set them in-app):

```env
OPENAI_API_KEY=your_openai_key
GROQ_API_KEY=your_groq_key
GOOGLE_GENERATIVE_AI_API_KEY=your_google_key
```

## Development

### Project Structure

- `src/` - React frontend application
- `src-tauri/` - Tauri backend and Rust code
- `src/components/` - Reusable UI components
- `src/store/` - State management with Zustand

### Available Scripts

- `bun run dev` - Start the Vite development server
- `bun run build` - Build the frontend for production
- `bun run tauri dev` - Start Tauri development mode
- `bun run tauri build` - Build the native application

### Updating Frontend for Nix Builds

The `dist/` folder is committed to the repository for reproducible Nix builds. When you make frontend changes, you must rebuild and commit the dist folder:

```bash
# Enter dev shell (if on NixOS)
nix develop

# Make your frontend changes...

# Rebuild the frontend
bun run build

# Commit the updated dist
git add dist/
git commit -m "Update frontend build"
git push
```

> **Note**: This is required because Nix builds are sandboxed and cannot access the network to run `bun install`. Pre-building ensures reproducible builds for all users.

## Packaging and Dependencies (Windows)

When distributing the Windows installer, include this checklist so users have the required runtime components for audio capture and transcription to work.

Required on target machine (or bundled in installer):

- Microsoft Visual C++ Redistributable (Visual Studio C++ 2015-2022 runtime)
- FFmpeg available on PATH (required by some audio libs and optional features)
- Python 3.12+ (if not bundling a portable Python)
  - pip available
  - If your installer does not bundle Python, the app will attempt to create a virtual environment and install dependencies at first run.
- Internet access for pip installs (unless you bundle prebuilt wheels)

Optional but recommended:

- Code-sign the EXE/MSI to avoid SmartScreen/AV false positives

Packaging tips:

- You can bundle a portable Python and a pre-created venv inside your installer to avoid first-run installs (tradeoff: larger installer size).
- Run installer tests on a clean Windows VM to reproduce missing-dependency issues.

## Using the preflight check

The app exposes a Tauri command `prereflight_check` that returns JSON with diagnostics (python presence, ffmpeg, VC++ runtime, network, resource paths). The UI can call this command at first run to show actionable errors to users.

## Contributing

DO NOT MAKE SPAM PRs.

## License

This project is licensed under the GNU Affero General Public License v3.0 (AGPL-3.0) - see the [LICENSE](LICENSE) file for details.
