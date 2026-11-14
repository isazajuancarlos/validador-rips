// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
// SPDX-License-Identifier: LicenseRef-Propietario

//! La pantalla autoservicio del validador: **«sube tu RIPS → qué rechazaría
//! MinSalud antes de enviarlo»**.
//!
//! Server-rendered con `maud` + htmx (directiva 26), servido por el mismo
//! binario. El núcleo que decide —[`validador_rips::Validador`]— no vive aquí:
//! esto solo le da forma de HTML a su `Vec<Falta>`. Copiar su razonamiento
//! habría creado una segunda copia que se queda atrás en el primer arreglo
//! (directiva 1); por eso la web es un CONSUMIDOR del crate, no una reescritura.
//!
//! Lo que la pantalla promete es exactamente lo que el crate caza: el
//! **pre-vuelo** de FORMA (estructura, campos faltantes, campos no permitidos).
//! No promete «soy MinSalud» —el oráculo definitivo de rechazo es el MUV, que
//! además cruza catálogos y reglas RVC—; promete «no envíes esto todavía, porque
//! por estas razones te lo van a devolver».

use std::sync::OnceLock;

use axum::{
    extract::{DefaultBodyLimit, Multipart},
    http::header,
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use maud::{html, Markup, PreEscaped, DOCTYPE};
use validador_rips::conversion::{convertir_excel, Convertido};
use validador_rips::{Falta, Validador};

// ── Lo vendorizado ──────────────────────────────────────────────────────────
// La MISMA versión de htmx y de `boi` que informes, medico e inventario
// (directiva 26), comprobada con `md5sum` al copiarla. Van LOCALES dentro del
// binario: un `<script src>` a un CDN es justo el vector que este stack existe
// para cerrar, y además no cargaría sin internet.
const HTMX_JS: &str = include_str!("../../estaticos/htmx.min.js");
const BOI_JS: &str = include_str!("../../estaticos/boi.js");
const BOI_CSS: &str = include_str!("../../estaticos/boi.css");

/// Dirección de escucha por defecto. El banco de pruebas y el despliegue la
/// cambian con `VRIPS_ADDR` (un puerto es una conveniencia del que arranca, no un
/// secreto: el defecto es legítimo). El VPS pondrá aquí su `127.0.0.1:<puerto>`
/// detrás de nginx.
const ADDR_DEFECTO: &str = "127.0.0.1:8096";

/// El validador se construye UNA vez (parsea la norma vendorizada) y se comparte.
/// `de_la_948` entra en pánico si la norma viniera vacía (directiva 30); ese
/// pánico es de arranque, no de una petición.
fn validador() -> &'static Validador {
    static V: OnceLock<Validador> = OnceLock::new();
    V.get_or_init(Validador::de_la_948)
}

#[tokio::main]
async fn main() {
    // Fuerza el pánico de arranque AQUÍ si la norma está rota, no en la primera
    // petición: un servicio que arranca «bien» y revienta al primer uso es peor.
    let _ = validador();

    let app = Router::new()
        .route("/", get(indice))
        .route("/validar", post(validar))
        .route("/htmx.min.js", get(htmx_js))
        .route("/boi.js", get(boi_js))
        .route("/boi.css", get(boi_css))
        .route("/salud", get(|| async { "ok" }))
        // Un RIPS de un mes puede pesar decenas de MiB (muchos usuarios): el tope
        // por defecto de axum (2 MiB) rechazaría archivos legítimos. 64 MiB.
        .layer(DefaultBodyLimit::max(64 * 1024 * 1024));

    let addr = std::env::var("VRIPS_ADDR").unwrap_or_else(|_| ADDR_DEFECTO.to_string());
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    // La dirección REAL (si se pidió el puerto 0, el SO eligió uno): el banco la
    // lee de aquí para saber a dónde hablar.
    println!("validador-rips en http://{}", listener.local_addr().unwrap());
    axum::serve(listener, app).await.unwrap();
}

// ── Los estáticos ───────────────────────────────────────────────────────────

fn recurso(cuerpo: &'static str, tipo: &'static str) -> Response {
    ([(header::CONTENT_TYPE, tipo)], cuerpo).into_response()
}
async fn htmx_js() -> Response {
    recurso(HTMX_JS, "application/javascript; charset=utf-8")
}
async fn boi_js() -> Response {
    recurso(BOI_JS, "application/javascript; charset=utf-8")
}
async fn boi_css() -> Response {
    recurso(BOI_CSS, "text/css; charset=utf-8")
}

// ── La envoltura ────────────────────────────────────────────────────────────

fn envoltura(titulo: &str, cuerpo: Markup) -> Markup {
    html! {
        (DOCTYPE)
        html lang="es" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (titulo) }
                link rel="stylesheet" href="/boi.css";
                style { (PreEscaped(ESTILO)) }
                // htmx clásico, boi como módulo. Los dos LOCALES: la CSP es
                // `script-src 'self'` y el producto corre sin internet.
                script src="/htmx.min.js" {}
                script type="module" src="/boi.js" {}
            }
            body {
                main.envoltura { (cuerpo) }
            }
        }
    }
}

/// Lo mínimo propio: centrar la columna y separar el encabezado. Todo lo demás
/// —colores, tipografía, tarjetas, avisos, tabla— sale de `boi.css` (directiva 26:
/// CSS propio, sin framework; lo compartido no se reescribe).
const ESTILO: &str = "\
.envoltura{max-width:52rem;margin:0 auto;padding:2rem 1rem 4rem}\
.cabecera{margin-bottom:1.5rem}\
.cabecera p{color:var(--boi-tinta-2);margin:.4rem 0 0}\
.acciones{display:flex;gap:.75rem;align-items:center;flex-wrap:wrap;margin-top:1rem}\
textarea.boi-in{min-height:9rem;font-family:var(--boi-mono);font-size:.85rem;resize:vertical}\
#resultado{margin-top:1.5rem}\
.faltas-cuenta{display:flex;gap:.5rem;flex-wrap:wrap;margin:.75rem 0 1rem}\
.o{color:var(--boi-tinta-2);font-size:.82rem}\
";

// ── La pantalla ─────────────────────────────────────────────────────────────

async fn indice() -> Markup {
    envoltura(
        "Validador de RIPS · pre-vuelo de la Resolución 948",
        html! {
            header.cabecera {
                h1.boi-h1 { "Valida tu RIPS antes de enviarlo" }
                p {
                    "Sube el "
                    strong { "Excel de la plantilla oficial" }
                    " (el del SPConvertidorJSON) o un "
                    strong { "FEV-RIPS" }
                    " en JSON, y te digo qué rechazaría MinSalud "
                    strong { "por forma" }
                    " —estructura, campos faltantes y campos no permitidos— "
                    "antes de que el MUV te devuelva el archivo entero el día del cierre. "
                    "Si subes el Excel, lo convierto a JSON y te lo doy para descargar."
                }
            }
            div.boi-tarjeta {
                div.boi-tarjeta__cuerpo {
                    // El formulario NO recarga la página: htmx postea y cambia
                    // solo #resultado. `multipart/form-data` porque lleva archivo.
                    form
                        hx-post="/validar"
                        hx-encoding="multipart/form-data"
                        hx-target="#resultado"
                        hx-swap="innerHTML"
                        hx-indicator="#cargando"
                    {
                        div.boi-campo {
                            label for="archivo" { "Archivo: Excel (.xlsm/.xlsx) o RIPS (.json)" }
                            input.boi-in #archivo type="file" name="archivo"
                                accept=".json,application/json,.xlsx,.xlsm,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/vnd.ms-excel.sheet.macroEnabled.12";
                        }
                        p.o { "…o pega el JSON:" }
                        div.boi-campo {
                            label for="texto" { "Contenido del RIPS" }
                            textarea.boi-in #texto name="texto"
                                placeholder="{ \"numDocumentoIdObligado\": \"...\", \"usuarios\": [ ... ] }" {}
                        }
                        div.acciones {
                            button.boi-btn.boi-btn--primario type="submit" { "Revisar" }
                            span #cargando.htmx-indicator.o { "revisando…" }
                        }
                    }
                }
                div.boi-tarjeta__pie {
                    span.o {
                        "Pre-vuelo, no MinSalud: caza la forma (la mayoría de los rechazos). "
                        "El oráculo definitivo sigue siendo el MUV. "
                        "No se guarda nada: el archivo se valida y se descarta."
                    }
                }
            }
            div #resultado {}
        },
    )
}

// ── La validación ───────────────────────────────────────────────────────────

/// Recibe el RIPS (archivo o texto pegado), lo valida y devuelve SOLO el fragmento
/// que htmx inserta en `#resultado`.
async fn validar(mut campos: Multipart) -> Markup {
    let mut archivo: Vec<u8> = Vec::new();
    let mut texto: Vec<u8> = Vec::new();

    while let Ok(Some(campo)) = campos.next_field().await {
        match campo.name() {
            Some("archivo") => archivo = campo.bytes().await.unwrap_or_default().to_vec(),
            // `.bytes()`, no `.text()`, para que `texto` y `archivo` recorran el
            // MISMO camino: bytes crudos → `serde_json::from_slice`. No arregla un
            // bug —medido el 2026-09-23, `.text()` de multer hace conversión lossy
            // (U+FFFD), así que un `texto` no-UTF8 ya salía «no es JSON válido», no
            // vacío—; solo quita la dependencia de ese detalle de la librería y
            // hace las dos entradas idénticas.
            Some("texto") => texto = campo.bytes().await.unwrap_or_default().to_vec(),
            _ => {}
        }
    }

    // El archivo manda sobre el texto pegado: si alguien sube un archivo, es lo
    // que quiere revisar.
    let crudo: &[u8] = if !archivo.is_empty() {
        &archivo
    } else {
        texto.trim_ascii()
    };

    // Un BOM de UTF-8 al inicio (EF BB BF) es lo que ponen Excel, el Bloc de notas
    // y varias herramientas del ecosistema RIPS al exportar el `RP_<fecha>.json`.
    // `serde_json` NO lo salta —se para en «expected value at line 1 column 1»— y
    // el MUV (.NET) SÍ lo tolera, así que rechazarlo aquí es un FALSO POSITIVO: el
    // archivo es JSON válido. Se quita antes de cualquier decisión. No toca el
    // carril del Excel (un `.xlsx` nunca empieza por BOM), ni el no-UTF8 (UTF-16
    // no se decodifica: serde no lo parsea y el MUV lo devuelve igual).
    // Caso real que lo destapó: RP_20261010174310.json, 2026-10-10.
    let crudo = crudo.strip_prefix(b"\xef\xbb\xbf").unwrap_or(crudo);

    if crudo.is_empty() {
        return aviso(
            "info",
            "Sin nada que revisar",
            html! { "Sube tu Excel (.xlsm/.xlsx) o un .json, o pega el contenido, y pulsa «Revisar»." },
        );
    }

    // Un `.xlsx`/`.xlsm` empieza por la magia de zip «PK\x03\x04». Si lo es, se
    // CONVIERTE con la plantilla oficial y se valida el resultado; si no, se trata
    // como JSON.
    if crudo.starts_with(b"PK\x03\x04") {
        return match convertir_excel(crudo) {
            Ok(conv) => {
                let faltas = validador().validar(&conv.rips);
                pinta_convertido(&conv, &faltas)
            }
            Err(e) => aviso(
                "mal",
                "No se pudo convertir el Excel",
                html! {
                    "El archivo parece un Excel pero no se pudo leer como la plantilla del RIPS: "
                    code.boi-mono { (e.to_string()) }
                },
            ),
        };
    }

    // Un archivo que no es JSON es el rechazo más tonto y más común: lo caza el
    // parser, y decirlo aquí ES pre-vuelo (el MUV lo devolvería igual).
    let rips: serde_json::Value = match serde_json::from_slice(crudo) {
        Ok(v) => v,
        Err(e) => {
            return aviso(
                "mal",
                "El archivo no es JSON válido",
                html! {
                    "El MUV rechaza un archivo que no puede leer. El parser se detuvo en: "
                    code.boi-mono { (e.to_string()) }
                },
            );
        }
    };

    let faltas = validador().validar(&rips);
    pinta_faltas(&faltas)
}

/// Tope del JSON que se ofrece como descarga incrustada (`data:`). Por encima, se
/// dice que es muy grande en vez de meter megas de base64 en la respuesta.
const DESCARGA_MAX: usize = 12 * 1024 * 1024;

/// El resultado de convertir un Excel: el resumen de la conversión, sus avisos, la
/// descarga del JSON y las faltas de validación.
fn pinta_convertido(conv: &Convertido, faltas: &[Falta]) -> Markup {
    let usuarios = conv.rips.get("usuarios").and_then(|u| u.as_array()).map_or(0, Vec::len);
    let servicios: usize = conv
        .rips
        .get("usuarios")
        .and_then(|u| u.as_array())
        .map(|us| {
            us.iter()
                .filter_map(|u| u.get("servicios").and_then(|s| s.as_object()))
                .flat_map(|s| s.values())
                .filter_map(|v| v.as_array())
                .map(Vec::len)
                .sum()
        })
        .unwrap_or(0);

    let json = serde_json::to_string(&conv.rips).unwrap_or_default();
    html! {
        (aviso(
            "info",
            "Excel convertido a JSON",
            html! {
                (usuarios) " usuario(s) y " (servicios) " servicio(s). "
                @if json.len() <= DESCARGA_MAX {
                    a.boi-btn.boi-btn--secundario download="rips.json"
                      href=(format!("data:application/json;base64,{}", base64(json.as_bytes()))) {
                        "Descargar JSON" }
                } @else {
                    "El JSON es muy grande para descargarlo aquí; revisa las faltas y corrige en el Excel."
                }
            },
        ))
        @for a in &conv.avisos {
            (aviso("info", "Aviso de conversión", html! { (a) }))
        }
        (pinta_faltas(faltas))
    }
}

/// base64 estándar (RFC 4648), sin dependencias: solo para incrustar el JSON
/// convertido en un enlace `data:` de descarga.
fn base64(datos: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(datos.len().div_ceil(3) * 4);
    for trozo in datos.chunks(3) {
        let b0 = trozo[0];
        let b1 = trozo.get(1).copied().unwrap_or(0);
        let b2 = trozo.get(2).copied().unwrap_or(0);
        let n = (u32::from(b0) << 16) | (u32::from(b1) << 8) | u32::from(b2);
        s.push(T[(n >> 18 & 63) as usize] as char);
        s.push(T[(n >> 12 & 63) as usize] as char);
        s.push(if trozo.len() > 1 { T[(n >> 6 & 63) as usize] as char } else { '=' });
        s.push(if trozo.len() > 2 { T[(n & 63) as usize] as char } else { '=' });
    }
    s
}

/// Un aviso de boi con marca de color. `clase` es `ok`, `mal` o `info`.
fn aviso(clase: &str, titulo: &str, detalle: Markup) -> Markup {
    html! {
        div class=(format!("boi-aviso boi-aviso--{clase}")) data-boi-listo="1" {
            span.boi-aviso__marca {}
            div {
                strong { (titulo) }
                div.o { (detalle) }
            }
        }
    }
}

/// El fragmento de resultados: el resumen y, si hay faltas, la tabla.
fn pinta_faltas(faltas: &[Falta]) -> Markup {
    if faltas.is_empty() {
        return aviso(
            "ok",
            "Sin faltas de forma",
            html! {
                "El MUV no lo rechazará por estructura, campos faltantes, campos no "
                "permitidos ni por un código fuera de las tablas cerradas de la 948. "
                "Los catálogos grandes (CIE-11, CUPS, CUM) y las reglas RVC quedan fuera "
                "a propósito —su oráculo definitivo es el MUV—: esto es pre-vuelo de forma, "
                "y la forma está limpia."
            },
        );
    }

    let (mut estructura, mut faltantes, mut no_permitidos, mut valor_malo) = (0u32, 0u32, 0u32, 0u32);
    for f in faltas {
        match f {
            Falta::Estructura { .. } => estructura += 1,
            Falta::CampoFaltante { .. } => faltantes += 1,
            Falta::CampoNoPermitido { .. } => no_permitidos += 1,
            Falta::ValorNoPermitido { .. } => valor_malo += 1,
        }
    }

    html! {
        (aviso(
            "mal",
            &format!("{} {} de forma", faltas.len(), if faltas.len() == 1 { "falta" } else { "faltas" }),
            html! { "Arregla esto antes de enviar: el MUV devolvería el archivo completo." },
        ))
        div.faltas-cuenta {
            @if no_permitidos > 0 {
                span.boi-pil.boi-pil--mal { (no_permitidos) " campo(s) no permitido(s)" }
            }
            @if valor_malo > 0 {
                span.boi-pil.boi-pil--mal { (valor_malo) " valor(es) fuera de tabla" }
            }
            @if faltantes > 0 {
                span.boi-pil.boi-pil--aviso { (faltantes) " campo(s) faltante(s)" }
            }
            @if estructura > 0 {
                span.boi-pil.boi-pil--info { (estructura) " de estructura" }
            }
        }
        div.boi-tabla-env {
            table.boi-tabla {
                thead {
                    tr { th { "Tipo" } th { "Dónde" } th { "Detalle" } }
                }
                tbody {
                    @for f in faltas {
                        (fila_falta(f))
                    }
                }
            }
        }
    }
}

fn fila_falta(f: &Falta) -> Markup {
    match f {
        Falta::Estructura { donde, detalle } => html! {
            tr {
                td { span.boi-pil.boi-pil--info { "estructura" } }
                td { code.boi-mono { (donde) } }
                td { (detalle) }
            }
        },
        Falta::CampoFaltante { donde, campo } => html! {
            tr {
                td { span.boi-pil.boi-pil--aviso { "faltante" } }
                td { code.boi-mono { (donde) } }
                td { "falta el campo " code.boi-mono { (campo) } }
            }
        },
        Falta::CampoNoPermitido { donde, campo } => html! {
            tr {
                td { span.boi-pil.boi-pil--mal { "no permitido" } }
                td { code.boi-mono { (donde) } }
                td {
                    "el campo " code.boi-mono { (campo) }
                    " no lo nombra la 948 → rechazo del archivo completo"
                }
            }
        },
        Falta::ValorNoPermitido { donde, campo, valor } => html! {
            tr {
                td { span.boi-pil.boi-pil--mal { "valor fuera de tabla" } }
                td { code.boi-mono { (donde) } }
                td {
                    "el campo " code.boi-mono { (campo) }
                    " trae " code.boi-mono { (valor) }
                    ", que no está en la tabla cerrada de la 948 → el MUV lo rechaza contra el catálogo"
                }
            }
        },
    }
}
