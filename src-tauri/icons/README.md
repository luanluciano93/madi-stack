# Icons

Placeholder directory. Before the first `cargo tauri build`, generate the icon set:

```bash
cargo tauri icon path/to/source.png
```

Required files for `bundle.icon` in `tauri.conf.json`:
- `32x32.png`
- `128x128.png`
- `128x128@2x.png`
- `icon.icns` (macOS — future)
- `icon.ico` (Windows installer)
