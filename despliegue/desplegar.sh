#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
# SPDX-License-Identifier: LicenseRef-Propietario
#
# Despliega validador-rips en el VPS. Se ejecuta DESDE la máquina de desarrollo.
#
#   ./despliegue/desplegar.sh debian@192.99.42.108
#
# POR QUÉ construye EN EL VPS (y no el binario nativo + scp de antes): la máquina
# de dev es Fedora (glibc 2.43) y el destino es Debian 13 (glibc 2.41). Un binario
# enlazado contra la glibc más nueva de Fedora NO arranca en Debian («GLIBC_2.43
# not found»). Subir el FUENTE y construir en el VPS da el binario correcto por
# construcción —es el mismo reparto que tenía el desplegar.ps1 del interludio
# Windows, portado a bash al volver a Linux—.
#
# El servicio es SIN ESTADO (no hay base ni secretos que respaldar) y SIN SALIDA
# a red, así que el guion es: archivar main → subir → construir en el VPS →
# verificar → respaldar el vivo → instalar (atómico) → reiniciar → comprobar con
# curl local y contra el dominio. Idempotente.
#
# Por defecto RETIRA el toolchain (rustup user-local) y el árbol de build al
# terminar, para dejar producción mínima (un compilador en el VPS es superficie de
# ataque, y contra la postura solo-local). Con --dejar-toolchain lo conserva para
# que el siguiente despliegue sea incremental y rápido.
#
# Lo que NO hace (una sola vez, ver el final):
#   - el registro DNS rips.xiliux.com  → ya creado por Juan (2026-09-23)
#   - el certificado TLS + vhost :443  → ./despliegue/desplegar-tls.sh (una vez)
set -euo pipefail

DESTINO=""
LIMPIAR="si"
for a in "$@"; do
    case "$a" in
        --dejar-toolchain) LIMPIAR="no" ;;
        -*) echo "opción desconocida: $a" >&2; exit 2 ;;
        *) DESTINO="$a" ;;
    esac
done
[ -z "$DESTINO" ] && { echo "Uso: $0 [--dejar-toolchain] usuario@servidor" >&2; exit 2; }

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"
SSH=(-o ControlMaster=no -o ControlPath=none)

echo "==> 0/6  comprobar acceso a $DESTINO"
ssh "${SSH[@]}" -o ConnectTimeout=10 "$DESTINO" true || { echo "sin acceso a $DESTINO" >&2; exit 3; }

echo "==> 1/6  archivar main y subir el fuente"
TAR="$(mktemp --suffix=.tar)"
git archive --format=tar -o "$TAR" main || { echo "git archive falló (¿estás en main con todo commiteado?)" >&2; exit 2; }
scp "${SSH[@]}" -q "$TAR" "$DESTINO:/tmp/vrips-deploy.tar"
rm -f "$TAR"

# --- Lo que corre EN EL VPS: construir, verificar, respaldar, instalar, reiniciar ---
REMOTO_SCRIPT="$(cat <<REMOTO
set -e
REMOTO=/opt/validador-rips
BUILD=\$HOME/build/validador-rips
echo '==> 2/6  desempaquetar y quitar el dev-dep de ruta Linux (no entra en --bin web)'
rm -rf "\$BUILD"; mkdir -p "\$BUILD"
tar -xf /tmp/vrips-deploy.tar -C "\$BUILD"; rm /tmp/vrips-deploy.tar
sed -i '/^mirilla = /d' "\$BUILD/Cargo.toml"

echo '==> 3/6  toolchain (rustup user-local) y build release'
if [ ! -x \$HOME/.cargo/bin/cargo ]; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable >/tmp/rustup.log 2>&1
fi
. \$HOME/.cargo/env
cd "\$BUILD"
cargo build --release --features web --bin web
BIN="\$BUILD/target/release/web"
grep -qa 'Valida tu RIPS antes de enviarlo' "\$BIN" || { echo 'el binario no lleva la marca; ¿construyó con --features web?' >&2; exit 4; }

echo '==> 4/6  respaldar el binario vivo e instalar el nuevo (atómico)'
TS=\$(date +%Y%m%d-%H%M%S)
sudo -n cp -a "\$REMOTO/web" "\$REMOTO/web.bak.\$TS"
echo "    respaldo -> \$REMOTO/web.bak.\$TS"
sudo -n install -o validador-rips -g validador-rips -m 0755 "\$BIN" "\$REMOTO/web.nuevo"
sudo -n mv -f "\$REMOTO/web.nuevo" "\$REMOTO/web"
L=\$(md5sum "\$BIN" | cut -d' ' -f1); A=\$(md5sum "\$REMOTO/web" | cut -d' ' -f1)
[ "\$L" = "\$A" ] || { echo "el binario no quedó igual (\$L vs \$A)" >&2; exit 5; }
echo "    md5 \$L  OK"
# Conservar solo los 3 respaldos más recientes (si no, crecen sin límite).
ls -1t "\$REMOTO"/web.bak.* 2>/dev/null | tail -n +4 | xargs -r sudo -n rm -f

echo '==> 5/6  instalar la unidad, reiniciar y comprobar por 127.0.0.1'
install -m 0644 "\$BUILD/despliegue/validador-rips.service" /tmp/validador-rips.service
sudo -n install -o root -g root -m 0644 /tmp/validador-rips.service /etc/systemd/system/validador-rips.service
rm -f /tmp/validador-rips.service
sudo -n systemctl daemon-reload
sudo -n systemctl enable --now validador-rips
sudo -n systemctl restart validador-rips
sleep 2
echo "    activo=\$(systemctl is-active validador-rips)"
printf '    salud=%s portada=%s\n' "\$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8096/salud)" "\$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8096/)"

echo '==> 6/6  limpieza'
rm -rf "\$BUILD"
if [ '$LIMPIAR' = 'si' ]; then
  rm -rf \$HOME/.cargo \$HOME/.rustup
  sed -i '/\.cargo\/env/d' \$HOME/.bashrc \$HOME/.profile 2>/dev/null || true
  echo '    toolchain retirado (producción mínima)'
else
  echo '    toolchain conservado (--dejar-toolchain)'
fi
REMOTO
)"

echo "==> construyendo y desplegando en el VPS (puede tardar unos minutos)..."
ssh "${SSH[@]}" "$DESTINO" "$REMOTO_SCRIPT"

echo "==> verificación final contra el dominio público"
R="$(curl -s -o /dev/null -w '%{http_code}' --max-time 15 https://rips.xiliux.com/)"
echo "    https://rips.xiliux.com/  ->  $R"
[ "$R" = "200" ] || echo "AVISO: el dominio no devolvió 200; revisa nginx/TLS" >&2
echo "== desplegado =="
