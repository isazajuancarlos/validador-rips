#!/usr/bin/env bash
# Publica rips.xiliux.com: obtiene el cert TLS y pone el vhold final de nginx.
# Se corre DESPUÉS de desplegar.sh (el binario ya escucha en 127.0.0.1:8096) y con
# el registro DNS ya creado. Desde la máquina de desarrollo:
#
#   ./despliegue/desplegar-tls.sh debian@192.99.42.108
#
# IDEMPOTENTE y resuelve el huevo-gallina del certificado:
#   - si NO hay cert: instala el vhost de ARRANQUE (solo :80, sin referencia a
#     cert → nginx -t pasa), reload, y saca el cert con certbot --webroot;
#   - luego instala el vhost COMPLETO (:80 + :443) y reload.
# El symlink de sites-enabled apunta siempre al mismo nombre; solo cambia el
# CONTENIDO del archivo en sites-available (arranque → completo).
#
# Vuelta atrás si algo falla: el nginx en vivo conserva su última config buena
# ante un `nginx -t` rojo; para quitar rips por completo:
#   ssh <destino> 'sudo rm -f /etc/nginx/sites-enabled/rips.xiliux.com.nginx && sudo nginx -t && sudo systemctl reload nginx'
set -euo pipefail

DESTINO="${1:-}"
[ -z "$DESTINO" ] && { echo "Uso: $0 usuario@servidor" >&2; exit 2; }

RAIZ="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$RAIZ"
DOM=rips.xiliux.com
AVAIL=/etc/nginx/sites-available/rips.xiliux.com.nginx
ENABL=/etc/nginx/sites-enabled/rips.xiliux.com.nginx

echo "==> 0/4  acceso a $DESTINO"
ssh -o ConnectTimeout=10 "$DESTINO" true || { echo "sin acceso" >&2; exit 3; }

if ssh "$DESTINO" "sudo -n test -f /etc/letsencrypt/live/$DOM/fullchain.pem"; then
    echo "==> 1/4  cert ya existe → salto arranque + certbot"
else
    echo "==> 1/4  vhost de ARRANQUE (solo :80) para servir el reto"
    scp -q despliegue/rips.xiliux.com.bootstrap.nginx "$DESTINO:/tmp/rips.bootstrap"
    ssh "$DESTINO" "set -e
        sudo -n install -o root -g root -m 0644 /tmp/rips.bootstrap $AVAIL
        rm -f /tmp/rips.bootstrap
        sudo -n ln -sf $AVAIL $ENABL
        sudo -n nginx -t
        sudo -n systemctl reload nginx"
    echo "==> 2/4  certbot (webroot)"
    ssh "$DESTINO" "sudo -n certbot certonly --webroot -w /var/www/html -d $DOM --non-interactive --agree-tos -m isazajuancarlos@gmail.com"
fi

echo "==> 3/4  vhost COMPLETO (:80 + :443) y reload"
scp -q despliegue/rips.xiliux.com.nginx "$DESTINO:/tmp/rips.full"
ssh "$DESTINO" "set -e
    sudo -n install -o root -g root -m 0644 /tmp/rips.full $AVAIL
    rm -f /tmp/rips.full
    sudo -n ln -sf $AVAIL $ENABL
    sudo -n nginx -t
    sudo -n systemctl reload nginx"

echo "==> 4/4  comprobar contra el dominio (la verdad de lo desplegado)"
printf '    https salud   %s\n' "$(curl -s -o /dev/null -w '%{http_code}' https://$DOM/salud)"
printf '    https portada %s\n' "$(curl -s -o /dev/null -w '%{http_code}' https://$DOM/)"

echo "== publicado en https://$DOM =="
