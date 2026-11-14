# Luz do Teclado (AppImage)

Liga e desliga a luz de teclados genéricos no Linux (LED `*::scrolllock`).
Na primeira abertura o programa pede a senha **uma vez** (pkexec) e instala uma regra
udev em `/etc/udev/rules.d/99-luz-teclado.rules`. Depois disso funciona sem senha,
mesmo após reiniciar.

## Gerar o AppImage no Fedora

```bash
# 1. Dependências do sistema
sudo dnf install webkit2gtk4.1-devel openssl-devel curl wget file \
  libappindicator-gtk3-devel librsvg2-devel libxdo-devel fuse fuse-libs nodejs npm
sudo dnf group install "c-development"

# 2. Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"

# 3. Compilar
cd luz-teclado
npm install
NO_STRIP=true npm run tauri build
```

O arquivo sai em `src-tauri/target/release/bundle/appimage/`.

```bash
chmod +x "Luz do Teclado_1.0.0_amd64.AppImage"
./"Luz do Teclado_1.0.0_amd64.AppImage"
```

Para testar sem gerar o AppImage: `npm run tauri dev`.

## Se algo der errado
- **Erro do linuxdeploy / strip:** mantenha o `NO_STRIP=true` no comando.
- **AppImage não abre:** instale `fuse` e `fuse-libs`, ou rode com `--appimage-extract-and-run`.
- **Tela em branco no NVIDIA/Wayland:** `WEBKIT_DISABLE_DMABUF_RENDERER=1 ./arquivo.AppImage`.
- **Remover a permissão:** `sudo rm /etc/udev/rules.d/99-luz-teclado.rules`.
