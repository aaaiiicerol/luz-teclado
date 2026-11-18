#!/usr/bin/env bash
# Envia uma nova versão para o GitHub e dispara a compilação dos pacotes.
# Uso (dentro da pasta do projeto):  ./atualizar.sh
set -e

VERSAO="v1.1.0"

[ -d .git ] || { echo "Rode este script dentro da pasta luz-teclado (onde está o .git)."; exit 1; }

git add .
git commit -m "Versão $VERSAO: ligar ao iniciar, bandeja, linha de comando, suporte e pacotes .deb/.rpm" || true
git push
git tag "$VERSAO"
git push origin "$VERSAO"

USUARIO=$(gh api user --jq .login 2>/dev/null || echo ezekielso)
echo
echo "Enviado! Acompanhe em: https://github.com/$USUARIO/luz-teclado/actions"
echo "Em uns 10 a 15 minutos os arquivos aparecem em: https://github.com/$USUARIO/luz-teclado/releases"
