OmniNova CLI (omninova) is copied here during `cargo build` when the `omninova` binary exists in the workspace `target/` directory.

After installing the desktop app (similar to Ollama):
- The gateway runs in the background while the app stays in the system tray.
- Recommended: open the app → Settings → General → "Install / update omninova to PATH" (no admin). This installs to:
  - macOS / Linux: ~/.local/bin/omninova
  - Windows: %LOCALAPPDATA%\omninova\bin\omninova.exe
  and updates the user PATH (Windows) or appends a line to ~/.zshrc (macOS) / ~/.profile (Linux) when needed.

Manual symlink example (macOS/Linux, admin may be required for /usr/local/bin):
  sudo ln -sf "/path/to/App/.../Resources/resources/cli/omninova" /usr/local/bin/omninova

The exact Resources path may vary by bundle layout; use Finder → Show Package Contents → Contents/Resources/resources/cli/.
