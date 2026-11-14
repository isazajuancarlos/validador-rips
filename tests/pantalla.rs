// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
// SPDX-License-Identifier: LicenseRef-Propietario

//! **La pantalla del validador, DIBUJADA en un navegador de verdad** (directiva 28).
//!
//! El resto de este crate mide el núcleo con `serde_json::Value` a `Vec<Falta>`:
//! eso mide el CÓDEC. La pantalla la termina de construir el navegador —el
//! `hx-swap` que inserta las faltas lo ejecuta JavaScript, y un `<script
//! type="module">` bloqueado por la CSP, servido con el MIME equivocado o que
//! revienta al importar **devuelve el mismo 200 con el mismo cuerpo**—. Así que
//! esto arranca el binario REAL y le habla como le habla el mundo:
//!
//!   * el carril de **navegador** (`mirilla`, CDP por tubería) pega un RIPS,
//!     ENVÍA el formulario y comprueba que las faltas aparecen en el DOM tras el
//!     intercambio de htmx —y que `boi` cargó, con `data-boi=listo`—;
//!   * el carril de **HTTP crudo** cubre lo que el navegador no puede tocar sin
//!     agujeros: subir un ARCHIVO (JS no puede rellenar un `input[type=file]`),
//!     el MIME de los estáticos, y el archivo que no es JSON.
//!
//! `mirilla` es `dev-dependency`: no entra en el binario de producción.
//!
//! **Dibuja y ejecuta; no mira píxeles.** Que una tabla salga descuadrada o que
//! un color no contraste no lo ve nadie todavía.

// El banco entero vive tras el `feature` web: sin él no existe el binario `web`
// que arranca (`CARGO_BIN_EXE_web`), así que `cargo test` a secas corre solo el
// núcleo puro y no intenta compilar esto.
#![cfg(feature = "web")]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use mirilla::{buscar_chromium, Navegador};

const BIN: &str = env!("CARGO_BIN_EXE_web");

/// Arranca el binario en un puerto EFÍMERO (`:0`) y lee del stdout cuál le tocó.
/// Un puerto fijo haría chocar dos corridas en paralelo.
struct Servidor {
    hijo: Child,
    base: String,
}

impl Servidor {
    fn arrancar() -> Self {
        let mut hijo = Command::new(BIN)
            .env("VRIPS_ADDR", "127.0.0.1:0")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("no arrancó el binario web");

        // La primera línea es «validador-rips en http://127.0.0.1:PUERTO».
        let salida = hijo.stdout.take().expect("sin stdout");
        let mut linea = String::new();
        BufReader::new(salida)
            .read_line(&mut linea)
            .expect("no se pudo leer el anuncio de arranque");
        let base = linea
            .split("http://")
            .nth(1)
            .map(|s| format!("http://{}", s.trim()))
            .expect("el anuncio no trae la URL");

        let srv = Servidor { hijo, base };
        srv.esperar_salud();
        srv
    }

    fn esperar_salud(&self) {
        let fin = std::time::Instant::now() + Duration::from_secs(10);
        while std::time::Instant::now() < fin {
            if let Ok(cuerpo) = http_get(&self.base, "/salud")
                && cuerpo.contains("ok")
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!("el servidor no respondió /salud a tiempo");
    }
}

impl Drop for Servidor {
    fn drop(&mut self) {
        let _ = self.hijo.kill();
        let _ = self.hijo.wait();
    }
}

// ── HTTP crudo (sin dependencias): `Connection: close` para leer hasta EOF ──────

fn direccion(base: &str) -> String {
    base.trim_start_matches("http://").to_string()
}

/// Manda una petición ya formada y devuelve la respuesta entera (cabeceras +
/// cuerpo). `Connection: close` hace que hyper cierre al terminar, así que se lee
/// hasta EOF sin parsear `Content-Length` ni `chunked`.
fn http_crudo(base: &str, peticion: &[u8]) -> std::io::Result<String> {
    let mut flujo = TcpStream::connect(direccion(base))?;
    flujo.set_read_timeout(Some(Duration::from_secs(10)))?;
    flujo.write_all(peticion)?;
    flujo.flush()?;
    let mut buf = Vec::new();
    flujo.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

fn http_get(base: &str, ruta: &str) -> std::io::Result<String> {
    let host = direccion(base);
    let pet = format!("GET {ruta} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n");
    http_crudo(base, pet.as_bytes())
}

/// POST `multipart/form-data` con un solo campo `nombre` cuyo contenido es
/// `cuerpo`. Sirve tanto para el campo de texto como para un archivo.
fn http_post_multipart(base: &str, ruta: &str, nombre: &str, archivo: Option<&str>, cuerpo: &[u8]) -> String {
    let host = direccion(base);
    let frontera = "----vripsPruebaFrontera";
    let disp = match archivo {
        Some(f) => format!(
            "Content-Disposition: form-data; name=\"{nombre}\"; filename=\"{f}\"\r\nContent-Type: application/json\r\n"
        ),
        None => format!("Content-Disposition: form-data; name=\"{nombre}\"\r\n"),
    };
    let mut cuerpo_mp = Vec::new();
    cuerpo_mp.extend_from_slice(format!("--{frontera}\r\n{disp}\r\n").as_bytes());
    cuerpo_mp.extend_from_slice(cuerpo);
    cuerpo_mp.extend_from_slice(format!("\r\n--{frontera}--\r\n").as_bytes());

    let mut pet = Vec::new();
    pet.extend_from_slice(
        format!(
            "POST {ruta} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
             Content-Type: multipart/form-data; boundary={frontera}\r\n\
             Content-Length: {}\r\n\r\n",
            cuerpo_mp.len()
        )
        .as_bytes(),
    );
    pet.extend_from_slice(&cuerpo_mp);
    http_crudo(base, &pet).expect("POST multipart falló")
}

// ── Carril HTTP crudo ───────────────────────────────────────────────────────

#[test]
fn los_estaticos_viajan_dentro_con_su_mime() {
    let srv = Servidor::arrancar();
    // boi.js DEBE servirse como javascript, o el navegador lo rechaza como módulo
    // y la pantalla se queda muerta con el mismo 200.
    let resp = http_get(&srv.base, "/boi.js").unwrap();
    assert!(resp.starts_with("HTTP/1.1 200"), "boi.js no dio 200: {}", &resp[..resp.len().min(80)]);
    assert!(
        resp.to_lowercase().contains("content-type: application/javascript"),
        "boi.js sin el MIME de javascript"
    );
    // Y su contenido es el vendorizado, no un 404 disfrazado.
    assert!(resp.contains("data-boi"), "boi.js no trae su código");

    // htmx y boi.css también dentro.
    assert!(http_get(&srv.base, "/htmx.min.js").unwrap().starts_with("HTTP/1.1 200"));
    let css = http_get(&srv.base, "/boi.css").unwrap();
    assert!(css.to_lowercase().contains("content-type: text/css"), "boi.css sin MIME css");
}

#[test]
fn un_archivo_subido_con_campo_no_permitido_se_caza() {
    let srv = Servidor::arrancar();
    // Un RIPS con un campo que la 948 no nombra: rechazo TOTAL del MUV.
    let rips = br#"{"numDocumentoIdObligado":"900","usuarios":[{"servicios":{"consultas":[{"campoInventado":1}]}}]}"#;
    let resp = http_post_multipart(&srv.base, "/validar", "archivo", Some("rips.json"), rips);
    assert!(resp.contains("no permitido"), "no marcó el campo no permitido:\n{resp}");
    assert!(resp.contains("campoInventado"), "no nombró el campo culpable");
    // Y la MISMA subida caza los faltantes del arreglo `consultas`.
    assert!(resp.contains("faltante"), "no marcó los campos faltantes");
}

#[test]
fn el_control_positivo_un_rips_limpio_sale_verde() {
    // PAREJA del caso rojo de arriba (directiva 33): un RIPS sin faltas de forma
    // NO puede dar faltas. Sin esto, un validador que dijera «no permitido» a todo
    // pasaría el caso rojo igual.
    let srv = Servidor::arrancar();
    let rips = br#"{"numDocumentoIdObligado":"900","usuarios":[]}"#;
    let resp = http_post_multipart(&srv.base, "/validar", "texto", None, rips);
    assert!(resp.contains("Sin faltas de forma"), "un RIPS limpio no salió verde:\n{resp}");
    assert!(resp.contains("boi-aviso--ok"), "sin el aviso verde");
    // El marcador de falta es la píldora/tabla, no la prosa: el propio texto verde
    // menciona «campos no permitidos» al explicar qué NO encontró.
    assert!(!resp.contains("boi-pil--mal"), "inventó una falta sobre un RIPS limpio");
    assert!(!resp.contains("boi-tabla"), "pintó la tabla de faltas sobre un RIPS limpio");
}

#[test]
fn un_rips_valido_con_bom_utf8_no_se_rechaza() {
    // PAREJA del control limpio de arriba (directiva 33): el MISMO RIPS que sale
    // verde, pero con el BOM de UTF-8 (EF BB BF) que anteponen Excel, el Bloc de
    // notas y las herramientas del ecosistema RIPS al exportar el `RP_<fecha>.json`.
    // `serde_json` no lo salta y el MUV (.NET) sí, así que rechazarlo es un falso
    // positivo sobre un archivo que es JSON VÁLIDO.
    //
    // Mutante (D33): contra el código SIN el `strip_prefix` del BOM —la versión
    // anterior— este archivo daba «no es JSON válido» (el parser se paraba en
    // «expected value at line 1 column 1»), así que el rojo falla por la VÍA REAL.
    // Caso real que lo destapó: RP_20261010174310.json, 2026-10-10.
    let srv = Servidor::arrancar();
    let mut cuerpo = vec![0xef, 0xbb, 0xbf];
    cuerpo.extend_from_slice(br#"{"numDocumentoIdObligado":"900","usuarios":[]}"#);
    let resp = http_post_multipart(&srv.base, "/validar", "archivo", Some("RP.json"), &cuerpo);
    assert!(!resp.contains("no es JSON válido"), "rechazó un JSON válido por el BOM:\n{resp}");
    assert!(resp.contains("Sin faltas de forma"), "el RIPS con BOM no salió verde:\n{resp}");
    assert!(resp.contains("boi-aviso--ok"), "sin el aviso verde tras quitar el BOM");
}

#[test]
fn un_valor_fuera_de_tabla_se_dibuja_en_el_fragmento() {
    // Una consulta completa salvo `finalidadTecnologiaSalud`, con `10` (el código de
    // la tabla ANTERIOR a la 948, ausente del MUV). La pantalla debe pintar la falta
    // de VALOR, nombrando el campo y el valor culpables —no un genérico «no permitido»—.
    let srv = Servidor::arrancar();
    let rips = br#"{"numDocumentoIdObligado":"900","usuarios":[{"tipoDocumentoIdentificacion":"CC",
        "numDocumentoIdentificacion":"1","tipoUsuario":"12","fechaNacimiento":"2000-01-01","codSexo":"M",
        "codPaisResidencia":"170","codMunicipioResidencia":"05001","codZonaTerritorialResidencia":"02",
        "incapacidad":"NO","codPaisOrigen":"170","consecutivo":1,"servicios":{"consultas":[{
        "codPrestador":"1","fechaInicioAtencion":"2026-01-06 08:00","codConsulta":"890201",
        "modalidadGrupoServicioTecSal":"01","grupoServicios":"01","codServicio":"334",
        "finalidadTecnologiaSalud":"10","causaMotivoAtencion":"38","codDiagnosticoPrincipal":"Z000",
        "tipoDiagnosticoPrincipal":"01","tipoDocumentoIdentificacion":"CC","numDocumentoIdentificacion":"1",
        "vrServicio":0,"conceptoRecaudo":"05","valorPagoModerador":0,"consecutivo":1}]}}]}"#;
    let resp = http_post_multipart(&srv.base, "/validar", "texto", None, rips);
    assert!(resp.contains("valor fuera de tabla"), "no marcó el valor fuera de tabla:\n{resp}");
    assert!(resp.contains("finalidadTecnologiaSalud"), "no nombró el campo culpable");
    assert!(resp.contains("10"), "no mostró el valor culpable");
}

#[test]
fn un_texto_pegado_no_utf8_no_se_traga_en_silencio() {
    // GUARDA DE COMPORTAMIENTO, no de implementación: un `texto` con bytes no-UTF8
    // NUNCA debe acabar en «sin nada que revisar» (un dato ausente sustituido por
    // un defecto vacío, directiva 20). Tiene que salir con un error visible.
    //
    // NO discrimina entre `.bytes()` y `.text()` a propósito, y está medido: multer
    // hace conversión lossy, así que las DOS variantes dan «no es JSON válido»
    // (verificado por mutación el 2026-09-23; la observación del revisor sobre un
    // vaciado silencioso NO se materializa). Lo que la prueba fija es la invariante
    // que importa —el no-UTF8 no se traga—, sea cual sea la librería debajo.
    let srv = Servidor::arrancar();
    let resp = http_post_multipart(&srv.base, "/validar", "texto", None, &[0xff, 0xfe, 0x00, 0x7b]);
    assert!(resp.contains("no es JSON válido"), "el texto no-UTF8 se tragó en silencio:\n{resp}");
    assert!(!resp.contains("Sin nada que revisar"), "lo enmascaró como vacío (D20)");
}

#[test]
fn subir_un_excel_lo_convierte_valida_y_ofrece_el_json() {
    // El flujo entero por HTTP: sube la plantilla Excel → el binario la convierte,
    // valida el resultado y ofrece la descarga. Con un xlsx SINTÉTICO (sin PII).
    use rust_xlsxwriter::Workbook;
    let mut wb = Workbook::new();
    let escribir = |wb: &mut Workbook, nombre: &str, filas: &[&[&str]]| {
        let h = wb.add_worksheet();
        h.set_name(nombre).unwrap();
        for (r, fila) in filas.iter().enumerate() {
            for (c, v) in fila.iter().enumerate() {
                h.write_string(r as u32, c as u16, *v).unwrap();
            }
        }
    };
    escribir(&mut wb, "transaccion", &[
        &["numDocumentoIdObligado", "tipoNota", "numNota"],
        &["900123", "RS", "1"],
    ]);
    escribir(&mut wb, "usuarios", &[
        &["tipoDocumentoIdentificacion", "numDocumentoIdentificacion", "tipoUsuario", "fechaNacimiento",
          "codSexo", "codPaisResidencia", "codMunicipioResidencia", "codZonaTerritorialResidencia",
          "incapacidad", "codPaisOrigen", "consecutivo"],
        &["CC", "111", "12", "2000-01-01", "M", "170", "05001", "02", "NO", "170", "1"],
    ]);
    escribir(&mut wb, "consultas", &[
        &["consecutivoUsuario", "codPrestador", "fechaInicioAtencion", "codConsulta",
          "modalidadGrupoServicioTecSal", "grupoServicios", "codServicio", "finalidadTecnologiaSalud",
          "causaMotivoAtencion", "codDiagnosticoPrincipal", "tipoDiagnosticoPrincipal",
          "tipoDocumentoIdentificacion", "numDocumentoIdentificacion", "vrServicio", "conceptoRecaudo",
          "valorPagoModerador", "consecutivo"],
        &["1", "1", "2026-08-01 08:00", "890201", "01", "01", "334", "44", "38", "Z000", "01", "CC",
          "111", "0", "05", "0", "1"],
    ]);
    let xlsx = wb.save_to_buffer().unwrap();

    let srv = Servidor::arrancar();
    let resp = http_post_multipart(&srv.base, "/validar", "archivo", Some("rips.xlsx"), &xlsx);
    assert!(resp.starts_with("HTTP/1.1 200"), "el Excel tumbó la respuesta");
    assert!(resp.contains("Excel convertido a JSON"), "no convirtió el Excel:\n{resp}");
    assert!(resp.contains("1 usuario(s)"), "no contó el usuario convertido");
    assert!(resp.contains("Sin faltas de forma"), "un Excel bien lleno debe salir limpio");
    assert!(resp.contains("Descargar JSON"), "no ofreció la descarga del JSON");
    assert!(resp.contains("data:application/json;base64,"), "el enlace de descarga no es un data URI");
}

#[test]
fn un_archivo_que_no_es_json_lo_dice_no_revienta() {
    let srv = Servidor::arrancar();
    let resp = http_post_multipart(&srv.base, "/validar", "archivo", Some("roto.json"), b"esto no es json {");
    assert!(resp.starts_with("HTTP/1.1 200"), "el archivo roto tumbó la respuesta");
    assert!(resp.contains("no es JSON válido"), "no explicó que no es JSON:\n{resp}");
}

// ── Carril de NAVEGADOR (directiva 28): la pantalla, dibujada ────────────────

/// Pega un RIPS en el textarea, ENVÍA el formulario (que htmx intercepta) y
/// espera a que `#resultado` se llene. Devuelve el DOM ya intercambiado.
fn revisar_en_navegador(nav: &mut Navegador, base: &str, rips: &str) -> String {
    nav.ir(&format!("{base}/"));
    // La prueba de que el MÓDULO cargó: boi marca el <html> cuando engancha. Si la
    // CSP, el MIME o el import fallaran, esto nunca se cumpliría.
    nav.esperar(
        "document.documentElement.dataset.boi === 'listo' && !!window.htmx",
        "boi/htmx no cargaron: la pantalla está muerta con el mismo 200",
    );
    // Rellenar el textarea y enviar. `requestSubmit()` dispara el submit que htmx
    // intercepta (`hx-post`), no un POST nativo. El valor puesto por JS entra en
    // la serialización del formulario igual que si se hubiera tecleado.
    let js = format!(
        "(() => {{ const t = document.querySelector('#texto'); t.value = {}; \
          document.querySelector('form').requestSubmit(); return true; }})()",
        serde_json::to_string(rips).unwrap()
    );
    nav.evaluar(&js);
    // Esperar al intercambio: #resultado deja de estar vacío.
    nav.esperar_hasta(
        "document.querySelector('#resultado') && document.querySelector('#resultado').children.length > 0",
        "htmx no intercambió el resultado en #resultado",
        Duration::from_secs(8),
    );
    nav.dom()
}

#[test]
#[ignore = "necesita Chromium; se corre en construir.sh y a mano"]
fn la_pantalla_pinta_las_faltas_tras_el_swap_de_htmx() {
    let srv = Servidor::arrancar();
    let chromium = buscar_chromium();
    let mut nav = Navegador::abrir(&chromium);

    // Caso ROJO: un RIPS con campo no permitido → la pantalla lo muestra.
    let rips = r#"{"numDocumentoIdObligado":"900","usuarios":[{"servicios":{"consultas":[{"campoInventado":1}]}}]}"#;
    let dom = revisar_en_navegador(&mut nav, &srv.base, rips);
    assert!(dom.contains("no permitido"), "el DOM tras el swap no muestra el campo no permitido:\n{dom}");
    assert!(dom.contains("campoInventado"), "el DOM no nombra el campo culpable");
}

#[test]
#[ignore = "necesita Chromium; se corre en construir.sh y a mano"]
fn la_pantalla_dice_verde_cuando_no_hay_faltas() {
    // La PAREJA en el navegador (directiva 33): un RIPS limpio pinta el verde, no
    // la tabla de faltas. Discrimina de la de arriba por la vía real (el swap).
    let srv = Servidor::arrancar();
    let chromium = buscar_chromium();
    let mut nav = Navegador::abrir(&chromium);

    let rips = r#"{"numDocumentoIdObligado":"900","usuarios":[]}"#;
    let dom = revisar_en_navegador(&mut nav, &srv.base, rips);
    assert!(dom.contains("Sin faltas de forma"), "el DOM no muestra el verde:\n{dom}");
    // Marcador real de falta, no la prosa del aviso verde (que dice «no permitidos»).
    assert!(!dom.contains("boi-pil--mal"), "el DOM inventó una falta sobre un RIPS limpio");
}
