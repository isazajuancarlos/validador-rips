<!--
SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
SPDX-License-Identifier: LicenseRef-Propietario
-->
# validador-rips

**«Sube tu RIPS y te digo qué rechazaría MinSalud, antes de enviarlo.»**

Valida un **FEV-RIPS** (Resolución 948 de 2026) contra los campos que el
Ministerio exige, y devuelve la lista exacta de problemas de FORMA que hunden un
archivo entero en el MUV:

- **campo faltante** — la 948 lo nombra y el servicio no lo trae;
- **campo no permitido** — el servicio trae algo que la 948 no nombra, y el MUV
  rechaza el archivo **completo** con «Propiedades no permitidas en la estructura»;
- **estructura** — no tiene forma de RIPS (no es objeto, falta `usuarios`, un
  servicio no es una lista…);
- **valor fuera de tabla** — un campo enumerado trae un código que no está en la
  tabla cerrada que la 948 le fija (p. ej. una `finalidadTecnologiaSalud` `"10"`,
  código de la tabla anterior que ya no existe): el MUV lo rechaza contra el catálogo.

## Alcance honesto

El oráculo **definitivo** de rechazo es el propio MUV de MinSalud, que además
cruza catálogos y reglas RVC. Esto es el **pre-vuelo**: caza los errores de forma
—que son la mayoría de los rechazos, y los más tontos de descubrir el día del
cierre de mes—. No promete «soy MinSalud»; promete «no envíes esto todavía,
porque por estas razones te lo van a devolver».

La aguja no se escribe a mano: sale de `datos/campos_fev948.json` (los campos de
cada arreglo) y `datos/dominios948.json` (los códigos que admite cada campo
enumerado), extraídos del Documento Técnico 1 de la Resolución 948 y de las tablas
REFs del contenedor oficial del MUV (normativa pública). Ver `NOTICE`.

La validación de **valor** se limita a las **tablas cerradas y cortas** de la 948
(finalidad, causa/motivo, vía de ingreso, condición/destino, tipo de documento):
pertenencia a la lista, que es forma. **No** replica al MUV: no cruza
finalidad×CUPS, ni restringe por arreglo/sexo/edad, ni distingue usuario de
profesional en el tipo de documento —eso es regla, y solo lo dice el envío—. Los
catálogos grandes (CIE-11, CUPS, CUM) quedan fuera a propósito.

## Dos piezas: el núcleo puro y la web

El **núcleo** (`Validador`, `Falta`) es la unidad vendible: una rebanada limpia,
solo `serde`. Sin el `feature` `web` quien dependa de este crate no se traga ni
axum ni tokio.

```rust
use validador_rips::Validador;

let v = Validador::de_la_948();
let rips: serde_json::Value = serde_json::from_str(&archivo_subido)?;
for falta in v.validar(&rips) {
    // falta serializa a JSON: {"tipo":"campo_faltante","donde":"usuarios[0].servicios.consultas[0]","campo":"codConsulta"}
}
```

El **convertidor Excel→JSON** (`feature xlsx`) toma la **plantilla oficial** del
SPConvertidorJSON de SISPRO (`.xlsm`/`.xlsx`) y produce la estructura FEV-RIPS:
lee las cabeceras de cada hoja (`transaccion`, `usuarios`, los 7 arreglos), cae
las columnas de enlace (`num_DocumentoIdObligado`, `consecutivoUsuario`) y anida
cada servicio bajo su usuario. El enlace servicio↔usuario casa por `consecutivo`
**o** por `numDocumentoIdentificacion` (medido sobre archivos reales), y un
servicio huérfano se avisa. Lector `calamine` (puro Rust, offline); tope de filas
contra bombas de descompresión.

La **pantalla autoservicio** —«sube tu Excel o pega tu RIPS → las faltas»— es un
consumidor de ambos, con el stack del estándar (axum + maud + htmx + `boi`
vendorizado, directiva 26). Sube el Excel → lo convierte, lo valida y te da el
JSON para descargar. Server-rendered, sin build de JS, sin CDN; no guarda nada
(valida y descarta). Entra tras el `feature`:

```bash
cargo run --features web --bin web      # escucha en 127.0.0.1:8096 (VRIPS_ADDR lo cambia)
./construir.sh                          # el artefacto de release + la pantalla DIBUJADA en Chromium (D28)
```

El banco de navegador (`tests/pantalla.rs`, con `mirilla`) vive `#[ignore]` para
que `cargo test` no exija Chromium; lo corre `construir.sh`, que exige que se haya
ejecutado. Con htmx el E2E EXIGE navegador: un `hx-swap` correcto en el sitio
equivocado devuelve el mismo 200.

## Qué valida hoy, y qué no

Valida la **forma** del RIPS: estructura, campos **no permitidos** (rechazo total
del MUV), y campos **requeridos** que faltan — distinguiendo requerido de
**opcional** (los CIE-11 de transición, los diagnósticos relacionados, los
administrativos condicionales), verificado contra un RIPS real que el MUV aceptó
(0 faltas falsas). Cubre el **sobre**, el **usuario** y los 7 arreglos de servicios.

**Fuera del alcance, a propósito** (decisión de producto, 2026-11-09): la **validez
de catálogo** de los códigos (CUM, CIE-10, CUPS). El oráculo definitivo es el MUV;
un catálogo local solo sería una copia que **envejece** y produce rechazos falsos
—confianza falsa, peor que no validar—, y medico tampoco los valida localmente (su
procedencia de redistribución no está declarada). Si alguna vez entra, será una capa
**opcional, fechada y que falle ruidosamente** cuando el catálogo esté viejo, nunca
en el núcleo «pre-vuelo de forma».

**Producto** (decisión 0011, 2026-11-09): nombre comercial **Pre-RIPS** (el repo
sigue `validador-rips`); cobro **freemium** — gratis validar un RIPS JSON, y un tier
**Pro** (convertidor Excel→JSON, lote, historial) por suscripción mensual. Falta
construir cuentas + puerta de pago + gating para cobrarlo.

## Estado

Núcleo + convertidor Excel + web **hechos y probados**: 29 del núcleo + doctest,
8 de conversión (Excel sintético, ambos modos de enlace, huérfano, «encaja» y
discrimina), 8 de HTTP + 2 de navegador. clippy `-D warnings` limpio. Vivo en
`rips.xiliux.com`.
