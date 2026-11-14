// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
// SPDX-License-Identifier: LicenseRef-Propietario

//! Convierte la **plantilla Excel oficial** del FEV-RIPS (la del
//! `SPConvertidorJSON` de SISPRO, `plantilla_fev-rips_v1_0.xlsm`) a la estructura
//! JSON del RIPS, para que un prestador suba lo que de verdad tiene —una hoja de
//! cálculo— en vez de un JSON que nadie teclea a mano.
//!
//! ## Cómo mapea, y de dónde sale la regla
//!
//! El Excel tiene una hoja por pieza de la estructura: `transaccion` (una fila →
//! el sobre), `usuarios`, y una por cada arreglo de servicios. Cada hoja se
//! describe a sí misma: la **primera fila son los nombres de campo**, y cada fila
//! siguiente es un objeto. No hace falta un esquema aparte para convertir —el
//! Excel lo trae—; el validador (aparte) es quien juzga si esos campos son los
//! que la 948 permite.
//!
//! Dos columnas son de ENLACE, no de dato, y se caen del JSON:
//!   * `num_DocumentoIdObligado` (con guion bajo) — repite el NIT del sobre en
//!     cada hoja; el del sobre es `numDocumentoIdObligado`, sin guion.
//!   * `consecutivoUsuario` — liga cada servicio a su usuario.
//!
//! **La regla de enlace se midió sobre un archivo REAL** (2026-09-23,
//! `RIPS-082026.xlsm`, 37 usuarios): los `consecutivoUsuario` de los servicios
//! casaban **todos con `numDocumentoIdentificacion`**, ninguno con `consecutivo`,
//! pese al nombre de la columna. Así que se casa por CUALQUIERA de los dos
//! (`consecutivo` primero, documento después), y un servicio que no case con
//! ningún usuario se **avisa** —nunca se tira en silencio (directiva 20)—.

use std::io::Cursor;

use calamine::{Data, Reader, Xlsx};
use serde_json::{Map, Value};

/// Tope de filas por hoja. **Acota el PROCESAMIENTO, no la descompresión**: para
/// cuando se comprueba, `worksheet_range` ya materializó la hoja en memoria (el
/// tope corta la validación O(filas) posterior sobre una hoja absurda, no el pico
/// de RAM de cargarla). El pico lo acota **el límite de cuerpo de la web** (la
/// entrada comprimida) y la ráfaga la corta el rate-limit de nginx; un `.xlsm`
/// hostil dentro de ese límite puede aún hacer pico de memoria (DoS acotado: el
/// servicio no tiene estado y systemd lo reinicia). Un RIPS de un mes rara vez
/// pasa de unos miles de atenciones; 500 000 filas es holgado y finito.
const MAX_FILAS: usize = 500_000;

/// Las dos columnas que ligan y no son dato del JSON.
const COLUMNAS_ENLACE: &[&str] = &["num_DocumentoIdObligado", "consecutivoUsuario"];

/// Los siete arreglos de servicios, con el nombre de su hoja (= su clave JSON).
const HOJAS_SERVICIO: &[&str] = &[
    "consultas",
    "procedimientos",
    "medicamentos",
    "urgencias",
    "hospitalizacion",
    "recienNacidos",
    "otrosServicios",
];

/// Por qué NO se pudo convertir el Excel. Son fallos de forma del archivo, no del
/// RIPS que contiene; se le muestran al usuario tal cual.
#[derive(Debug)]
pub enum ErrorConversion {
    /// No es un `.xlsx`/`.xlsm` legible (no es Excel, o está corrupto).
    NoEsExcel(String),
    /// Falta una hoja imprescindible (`transaccion` o `usuarios`).
    FaltaHoja(&'static str),
    /// Una hoja superó [`MAX_FILAS`]: se rechaza para no correr la validación
    /// O(filas) sobre una hoja absurda (ver la nota de [`MAX_FILAS`]).
    DemasiadasFilas { hoja: String, filas: usize },
    /// La hoja `transaccion` no trae ninguna fila de datos.
    SinTransaccion,
    /// El lector de Excel (`calamine`) entró en pánico con un archivo malformado
    /// —un zip corrupto, un XML tramposo, dimensiones mentirosas—. Se captura para
    /// devolver un error amable en vez de tumbar el hilo de la petición.
    Ilegible,
}

impl std::fmt::Display for ErrorConversion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErrorConversion::NoEsExcel(e) => write!(f, "el archivo no es un Excel legible: {e}"),
            ErrorConversion::FaltaHoja(h) => write!(f, "falta la hoja obligatoria «{h}»"),
            ErrorConversion::DemasiadasFilas { hoja, filas } => {
                write!(f, "la hoja «{hoja}» tiene {filas} filas, más del máximo admitido")
            }
            ErrorConversion::SinTransaccion => {
                write!(f, "la hoja «transaccion» no tiene ninguna fila de datos")
            }
            ErrorConversion::Ilegible => {
                write!(f, "el archivo Excel está corrupto o malformado y no se pudo leer")
            }
        }
    }
}

/// El resultado de convertir: el RIPS en JSON y los avisos de conversión (cosas
/// que no impiden producir el JSON pero que el usuario debe saber, p. ej.
/// servicios que no casaron con ningún usuario).
pub struct Convertido {
    pub rips: Value,
    pub avisos: Vec<String>,
}

/// Convierte los bytes de un `.xlsx`/`.xlsm` a la estructura FEV-RIPS.
///
/// `calamine` maneja como `Err` lo que espera, pero NO es libre de pánico ante
/// entrada adversaria (un zip corrupto, un XML malformado, dimensiones mentirosas
/// pueden disparar un `unwrap`/desbordamiento interno). Como esto recibe subidas
/// anónimas, la conversión se corre bajo `catch_unwind` y un pánico se mapea al
/// mismo camino de error amable, en vez de tumbar el hilo de la petición.
pub fn convertir_excel(bytes: &[u8]) -> Result<Convertido, ErrorConversion> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| convertir_interno(bytes))) {
        Ok(r) => r,
        Err(_) => Err(ErrorConversion::Ilegible),
    }
}

fn convertir_interno(bytes: &[u8]) -> Result<Convertido, ErrorConversion> {
    let mut libro: Xlsx<_> = calamine::open_workbook_from_rs(Cursor::new(bytes))
        .map_err(|e: calamine::XlsxError| ErrorConversion::NoEsExcel(e.to_string()))?;

    let mut avisos = Vec::new();

    // ── El sobre (hoja `transaccion`, una fila de datos) ──────────────────────
    let trans = leer_hoja(&mut libro, "transaccion")?.ok_or(ErrorConversion::FaltaHoja("transaccion"))?;
    let (cab_t, filas_t) = trans;
    let primera = filas_t.into_iter().next().ok_or(ErrorConversion::SinTransaccion)?;
    let mut raiz = objeto_de_fila(&cab_t, &primera);

    // ── Usuarios ──────────────────────────────────────────────────────────────
    let usuarios = leer_hoja(&mut libro, "usuarios")?.ok_or(ErrorConversion::FaltaHoja("usuarios"))?;
    let (cab_u, filas_u) = usuarios;
    // Cada usuario: su objeto JSON, y sus dos claves de enlace posibles.
    let mut objetos_usuario: Vec<Map<String, Value>> = Vec::new();
    // clave (consecutivo o numDocumentoIdentificacion, como texto) → índice de usuario
    let mut indice: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for fila in &filas_u {
        let obj = objeto_de_fila(&cab_u, fila);
        if obj.is_empty() {
            continue;
        }
        let idx = objetos_usuario.len();
        for clave in ["consecutivo", "numDocumentoIdentificacion"] {
            if let Some(v) = obj.get(clave) {
                let k = texto_de_valor(v);
                // NO sobrescribir en silencio: si dos usuarios comparten un
                // consecutivo o un documento, los servicios de uno se adjuntarían
                // al otro sin que nadie lo note (directiva 20). Se conserva el
                // PRIMERO y se avisa.
                match indice.entry(k.clone()) {
                    std::collections::hash_map::Entry::Vacant(e) => {
                        e.insert(idx);
                    }
                    std::collections::hash_map::Entry::Occupied(e) if *e.get() != idx => {
                        avisos.push(format!(
                            "dos usuarios comparten el identificador de enlace «{k}» ({clave}); \
                             sus servicios se adjuntaron al primero. Revisa que cada usuario tenga \
                             consecutivo y documento únicos."
                        ));
                    }
                    _ => {}
                }
            }
        }
        objetos_usuario.push(obj);
    }

    // Cada usuario lleva su bolsa de servicios (se rellena abajo y se sella al final).
    let mut servicios: Vec<Map<String, Value>> = vec![Map::new(); objetos_usuario.len()];

    // ── Servicios ─────────────────────────────────────────────────────────────
    for hoja in HOJAS_SERVICIO {
        let Some((cab_s, filas_s)) = leer_hoja(&mut libro, hoja)? else {
            continue; // la hoja puede no existir en un Excel recortado
        };
        let mut huerfanos = 0usize;
        for fila in &filas_s {
            let liga = valor_columna(&cab_s, fila, "consecutivoUsuario").map(texto_de_celda);
            let obj = objeto_de_fila(&cab_s, fila);
            if obj.is_empty() {
                continue;
            }
            match liga.as_deref().and_then(|k| indice.get(k)) {
                Some(&idx) => {
                    servicios[idx]
                        .entry((*hoja).to_string())
                        .or_insert_with(|| Value::Array(Vec::new()))
                        .as_array_mut()
                        .expect("recién insertado como arreglo")
                        .push(Value::Object(obj));
                }
                None => huerfanos += 1,
            }
        }
        if huerfanos > 0 {
            avisos.push(format!(
                "{huerfanos} servicio(s) de «{hoja}» no casaron con ningún usuario (su consecutivoUsuario no coincide con el consecutivo ni el documento de nadie) y se omitieron."
            ));
        }
    }

    // ── Ensamblar: cada usuario con su objeto `servicios` ────────────────────
    let arreglo_usuarios: Vec<Value> = objetos_usuario
        .into_iter()
        .zip(servicios)
        .map(|(mut u, s)| {
            u.insert("servicios".to_string(), Value::Object(s));
            Value::Object(u)
        })
        .collect();
    raiz.insert("usuarios".to_string(), Value::Array(arreglo_usuarios));

    Ok(Convertido { rips: Value::Object(raiz), avisos })
}

/// Lee una hoja: devuelve `(cabeceras, filas_de_datos)`, o `None` si la hoja no
/// existe. Aplica el tope de filas.
type Hoja = (Vec<String>, Vec<Vec<Data>>);
fn leer_hoja(libro: &mut Xlsx<Cursor<&[u8]>>, nombre: &str) -> Result<Option<Hoja>, ErrorConversion> {
    let rango = match libro.worksheet_range(nombre) {
        Ok(r) => r,
        Err(_) => return Ok(None), // hoja ausente
    };
    let total = rango.rows().count();
    if total > MAX_FILAS {
        return Err(ErrorConversion::DemasiadasFilas { hoja: nombre.to_string(), filas: total });
    }
    let mut filas = rango.rows();
    let Some(cab_row) = filas.next() else {
        return Ok(Some((Vec::new(), Vec::new())));
    };
    let cabeceras: Vec<String> = cab_row.iter().map(texto_de_celda).collect();
    let datos: Vec<Vec<Data>> = filas.map(<[Data]>::to_vec).collect();
    Ok(Some((cabeceras, datos)))
}

/// Construye el objeto JSON de una fila: empareja cada celda con su cabecera,
/// omite las columnas de enlace y las celdas vacías (una celda vacía es un dato
/// AUSENTE, no una cadena vacía —que el validador leería como presente—).
fn objeto_de_fila(cabeceras: &[String], fila: &[Data]) -> Map<String, Value> {
    let mut obj = Map::new();
    for (i, cab) in cabeceras.iter().enumerate() {
        if COLUMNAS_ENLACE.contains(&cab.as_str()) {
            continue;
        }
        let Some(celda) = fila.get(i) else { continue };
        if let Some(v) = celda_a_json(celda) {
            obj.insert(cab.clone(), v);
        }
    }
    obj
}

fn valor_columna<'a>(cabeceras: &[String], fila: &'a [Data], nombre: &str) -> Option<&'a Data> {
    let i = cabeceras.iter().position(|c| c == nombre)?;
    fila.get(i)
}

/// Una celda a JSON, o `None` si está vacía. Los códigos van como los guardó el
/// Excel (texto si el prestador respetó el formato de la plantilla, que preserva
/// los ceros a la izquierda); los números como números. El validador comprueba la
/// FORMA (qué campos hay), no el tipo —de eso el oráculo definitivo es el MUV—.
fn celda_a_json(d: &Data) -> Option<Value> {
    match d {
        Data::Empty => None,
        Data::String(s) if s.trim().is_empty() => None,
        Data::String(s) => Some(Value::String(s.trim().to_string())),
        Data::Float(f) => Some(if f.fract() == 0.0 && f.abs() < 9e15 {
            Value::from(*f as i64)
        } else {
            Value::from(*f)
        }),
        Data::Int(i) => Some(Value::from(*i)),
        Data::Bool(b) => Some(Value::String(if *b { "true" } else { "false" }.to_string())),
        Data::DateTime(dt) => Some(Value::String(dt.to_string())),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Some(Value::String(s.clone())),
        Data::Error(_) => None,
    }
}

fn texto_de_celda(d: &Data) -> String {
    match d {
        Data::String(s) => s.trim().to_string(),
        Data::Float(f) if f.fract() == 0.0 && f.abs() < 9e15 => (*f as i64).to_string(),
        Data::Float(f) => f.to_string(),
        Data::Int(i) => i.to_string(),
        Data::Bool(b) => b.to_string(),
        Data::DateTime(dt) => dt.to_string(),
        Data::DateTimeIso(s) | Data::DurationIso(s) => s.clone(),
        Data::Empty | Data::Error(_) => String::new(),
    }
}

fn texto_de_valor(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}
