#!/usr/bin/env bash
# Publica o projeto no GitHub e dispara a geração do AppImage.
# Uso: ./publicar.sh
set -e

REPO="luz-teclado"
VERSAO="v1.0.0"

if ! command -v gh >/dev/null; then
  echo "Instale o GitHub CLI primeiro:  sudo dnf install gh   (Mint/Ubuntu: sudo apt install gh)"
  exit 1
fi

# Login (abre o navegador, só na primeira vez)
gh auth status >/dev/null 2>&1 || gh auth login --web --git-protocol https
gh auth setup-git

git init -b main 2>/dev/null || true
git add .
git commit -m "Primeira versão do Luz do Teclado" || true

gh repo create "$REPO" --public --source=. --remote=origin --push \
  --description "Liga e desliga a luz de teclados genéricos no Linux (AppImage)"

git tag "$VERSAO"
git push origin "$VERSAO"

USUARIO=$(gh api user --jq .login)
echo
echo "Pronto! Acompanhe a compilação em: https://github.com/$USUARIO/$REPO/actions"
echo "Em uns 10 minutos o AppImage aparece em: https://github.com/$USUARIO/$REPO/releases"
