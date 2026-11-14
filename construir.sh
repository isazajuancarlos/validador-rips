#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
# SPDX-License-Identifier: LicenseRef-Propietario
#
# EL ARTEFACTO: el binario `web` de release + la pantalla DIBUJADA en un navegador
# de verdad (directiva 28). El banco de navegador vive `#[ignore]` para que
# `cargo test` no exija Chromium ni se cuelgue en un CI; ESTE guion es el único
# sitio donde se ejecuta, y exige que se haya EJECUTADO —no solo que salga 0—,
# porque un `--include-ignored` sin pruebas que corran también da código 0.
set -euo pipefail
cd "$(dirname "$0")"

echo "== 1/2  binario de release (con la pantalla dentro) =="
cargo build --release --features web --bin web
test -x target/release/web
echo "   OK: target/release/web"

echo "== 2/2  la pantalla, dibujada en Chromium =="
salida=$(cargo test --release --features web --test pantalla -- --include-ignored 2>&1)
echo "$salida" | tail -20
# La guarda: exige que el navegador haya CORRIDO. `1 passed`/`2 passed`… con el
# banco de pantalla; un `0 passed` significaría que nada se dibujó y no vale.
if ! echo "$salida" | grep -qE '[1-9][0-9]* passed'; then
    echo "FALLO: el banco de navegador no ejecutó ninguna prueba (¿Chromium?)." >&2
    exit 1
fi
echo "   OK: la pantalla se dibujó y htmx intercambió"
echo "== verde =="
