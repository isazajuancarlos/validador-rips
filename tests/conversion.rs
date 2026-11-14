// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
// SPDX-License-Identifier: LicenseRef-Propietario

//! El convertidor Excel→JSON, contra una plantilla SINTÉTICA fabricada en el
//! propio banco (nada de RIPS reales en el repo: llevan documentos y diagnósticos,
//! dato personal de la Ley 1581). La forma se calcó de la plantilla oficial del
//! SPConvertidorJSON, medida el 2026-09-23.
//!
//! Prueba las dos cosas que importan (directiva 33):
//!   * que **encaja** — un Excel bien lleno se convierte a un RIPS que el validador
//!     acepta sin faltas (converter y validador comparten el mismo spec);
//!   * que **discrimina** — una columna que la 948 no nombra sobrevive la
//!     conversión y el validador la caza como «no permitido».
//!
//! Cubre además los dos modos de enlace servicio↔usuario reales (por
//! `consecutivo` y por `numDocumentoIdentificacion`) y el aviso de huérfano.

#![cfg(feature = "xlsx")]

use rust_xlsxwriter::Workbook;
use validador_rips::{conversion::convertir_excel, Falta, Validador};

/// Escribe una hoja: `filas[0]` es la cabecera, el resto son datos. Todo como
/// texto (como la plantilla oficial, que preserva los ceros a la izquierda).
fn hoja(wb: &mut Workbook, nombre: &str, filas: &[&[&str]]) {
    let h = wb.add_worksheet();
    h.set_name(nombre).unwrap();
    for (r, fila) in filas.iter().enumerate() {
        for (c, val) in fila.iter().enumerate() {
            h.write_string(r as u32, c as u16, *val).unwrap();
        }
    }
}

/// Una plantilla sintética con envoltura, 2 usuarios y consultas que enlazan por
/// las DOS vías, más un procedimiento huérfano.
fn plantilla_sintetica() -> Vec<u8> {
    let mut wb = Workbook::new();

    hoja(&mut wb, "transaccion", &[
        &["numDocumentoIdObligado", "numFactura", "tipoNota", "numNota"],
        &["900123", "", "RS", "1"], // sin factura: numFactura vacío
    ]);

    hoja(&mut wb, "usuarios", &[
        &["tipoDocumentoIdentificacion", "numDocumentoIdentificacion", "num_DocumentoIdObligado",
          "tipoUsuario", "fechaNacimiento", "codSexo", "codPaisResidencia", "codMunicipioResidencia",
          "codZonaTerritorialResidencia", "incapacidad", "codPaisOrigen", "registroSIRAS", "consecutivo"],
        &["CC", "111", "900123", "12", "2000-01-01", "M", "170", "05001", "02", "NO", "170", "", "1"],
        &["CC", "222", "900123", "12", "1990-05-05", "F", "170", "05001", "02", "NO", "170", "", "2"],
    ]);

    // consultas: fila 1 enlaza por DOCUMENTO (111 = numDoc de u1); fila 2 por
    // CONSECUTIVO (2 = consecutivo de u2). Las dos vías, como en los archivos reales.
    let cab_c: &[&str] = &["num_DocumentoIdObligado", "consecutivoUsuario", "codPrestador",
        "fechaInicioAtencion", "codConsulta", "modalidadGrupoServicioTecSal", "grupoServicios",
        "codServicio", "finalidadTecnologiaSalud", "causaMotivoAtencion", "codDiagnosticoPrincipal",
        "tipoDiagnosticoPrincipal", "tipoDocumentoIdentificacion", "numDocumentoIdentificacion",
        "vrServicio", "conceptoRecaudo", "valorPagoModerador", "consecutivo"];
    hoja(&mut wb, "consultas", &[
        cab_c,
        &["900123", "111", "1", "2026-08-01 08:00", "890201", "01", "01", "334", "44", "38", "Z000", "01", "CC", "111", "0", "05", "0", "1"],
        &["900123", "2",   "1", "2026-08-02 09:00", "890201", "01", "01", "334", "44", "38", "Z001", "01", "CC", "222", "0", "05", "0", "2"],
    ]);

    // procedimientos: una fila HUÉRFANA (consecutivoUsuario=999, no existe) → aviso.
    hoja(&mut wb, "procedimientos", &[
        &["num_DocumentoIdObligado", "consecutivoUsuario", "codPrestador", "consecutivo"],
        &["900123", "999", "1", "1"],
    ]);

    wb.save_to_buffer().unwrap()
}

#[test]
fn convierte_la_envoltura_y_los_dos_usuarios() {
    let c = convertir_excel(&plantilla_sintetica()).unwrap();
    let rips = &c.rips;
    assert_eq!(rips["numDocumentoIdObligado"], "900123");
    assert_eq!(rips["tipoNota"], "RS");
    assert_eq!(rips["numNota"], "1");
    assert!(rips.get("numFactura").is_none(), "numFactura vacío debe quedar ausente, no \"\"");
    assert_eq!(rips["usuarios"].as_array().unwrap().len(), 2);
    // Las columnas de enlace NO viajan al JSON.
    let u0 = &rips["usuarios"][0];
    assert!(u0.get("num_DocumentoIdObligado").is_none(), "columna de enlace en el JSON");
}

#[test]
fn enlaza_por_documento_y_por_consecutivo() {
    let c = convertir_excel(&plantilla_sintetica()).unwrap();
    let us = c.rips["usuarios"].as_array().unwrap();
    // u1 (numDoc 111): su consulta llegó por enlace de DOCUMENTO.
    let c1 = us[0]["servicios"]["consultas"].as_array().unwrap();
    assert_eq!(c1.len(), 1, "u1 debe tener 1 consulta (enlace por documento)");
    assert_eq!(c1[0]["codDiagnosticoPrincipal"], "Z000");
    assert!(c1[0].get("consecutivoUsuario").is_none(), "el enlace no debe quedar en el servicio");
    // u2 (consecutivo 2): su consulta llegó por enlace de CONSECUTIVO.
    let c2 = us[1]["servicios"]["consultas"].as_array().unwrap();
    assert_eq!(c2.len(), 1, "u2 debe tener 1 consulta (enlace por consecutivo)");
    assert_eq!(c2[0]["codDiagnosticoPrincipal"], "Z001");
}

#[test]
fn un_servicio_huerfano_se_avisa_no_se_traga() {
    let c = convertir_excel(&plantilla_sintetica()).unwrap();
    assert!(
        c.avisos.iter().any(|a| a.contains("procedimientos") && a.contains("1 servicio")),
        "el procedimiento huérfano debe avisarse: {:?}",
        c.avisos
    );
}

#[test]
fn encaja_el_excel_convertido_pasa_el_validador_sin_faltas() {
    // La prueba de que converter y validador comparten spec: un Excel bien lleno
    // produce un RIPS que el validador acepta LIMPIO.
    let c = convertir_excel(&plantilla_sintetica()).unwrap();
    let faltas = Validador::de_la_948().validar(&c.rips);
    assert!(faltas.is_empty(), "el RIPS convertido no debería tener faltas: {faltas:?}");
}

#[test]
fn discrimina_una_columna_que_la_948_no_nombra() {
    // ROJO de todo el flujo: una columna intrusa sobrevive la conversión y el
    // validador la caza. Sin esto, «encaja» pasaría igual con un validador ciego.
    let mut wb = Workbook::new();
    hoja(&mut wb, "transaccion", &[
        &["numDocumentoIdObligado", "tipoNota", "numNota"],
        &["900123", "RS", "1"],
    ]);
    hoja(&mut wb, "usuarios", &[
        &["tipoDocumentoIdentificacion", "numDocumentoIdentificacion", "tipoUsuario", "fechaNacimiento",
          "codSexo", "codPaisResidencia", "codMunicipioResidencia", "codZonaTerritorialResidencia",
          "incapacidad", "codPaisOrigen", "consecutivo"],
        &["CC", "111", "12", "2000-01-01", "M", "170", "05001", "02", "NO", "170", "1"],
    ]);
    hoja(&mut wb, "consultas", &[
        &["consecutivoUsuario", "codPrestador", "fechaInicioAtencion", "codConsulta",
          "modalidadGrupoServicioTecSal", "grupoServicios", "codServicio", "finalidadTecnologiaSalud",
          "causaMotivoAtencion", "codDiagnosticoPrincipal", "tipoDiagnosticoPrincipal",
          "tipoDocumentoIdentificacion", "numDocumentoIdentificacion", "vrServicio", "conceptoRecaudo",
          "valorPagoModerador", "consecutivo", "columnaInventada"],
        &["1", "1", "2026-08-01 08:00", "890201", "01", "01", "334", "44", "38", "Z000", "01", "CC",
          "111", "0", "05", "0", "1", "algo"],
    ]);
    let c = convertir_excel(&wb.save_to_buffer().unwrap()).unwrap();
    let faltas = Validador::de_la_948().validar(&c.rips);
    assert!(
        faltas.iter().any(|f| matches!(f, Falta::CampoNoPermitido { campo, .. } if campo == "columnaInventada")),
        "la columna intrusa debe salir como no permitida: {faltas:?}"
    );
}

#[test]
fn algo_que_no_es_excel_da_error_no_panico() {
    let e = convertir_excel(b"esto no es un xlsx");
    assert!(e.is_err(), "un no-Excel debe dar Err, no convertir");
}

#[test]
fn un_xlsx_corrupto_con_magia_pk_da_error_no_panico() {
    // La subida hostil clásica: empieza como un zip («PK\x03\x04») pero es basura.
    // calamine puede PANICAR con esto; el `catch_unwind` lo convierte en Err amable.
    // Si el panic escapara, este test abortaría el proceso de pruebas.
    let mut bytes = b"PK\x03\x04".to_vec();
    bytes.extend_from_slice(&[0u8; 200]);
    let e = convertir_excel(&bytes);
    assert!(e.is_err(), "un zip corrupto debe dar Err, no panic");
}

#[test]
fn dos_usuarios_con_el_mismo_documento_avisan_no_se_confunden_en_silencio() {
    // Directiva 20: una colisión de identificador de enlace no se traga.
    let mut wb = Workbook::new();
    hoja(&mut wb, "transaccion", &[
        &["numDocumentoIdObligado", "tipoNota", "numNota"],
        &["900123", "RS", "1"],
    ]);
    hoja(&mut wb, "usuarios", &[
        &["tipoDocumentoIdentificacion", "numDocumentoIdentificacion", "tipoUsuario", "fechaNacimiento",
          "codSexo", "codPaisResidencia", "codMunicipioResidencia", "codZonaTerritorialResidencia",
          "incapacidad", "codPaisOrigen", "consecutivo"],
        &["CC", "111", "12", "2000-01-01", "M", "170", "05001", "02", "NO", "170", "1"],
        &["CC", "111", "12", "1990-05-05", "F", "170", "05001", "02", "NO", "170", "2"], // MISMO documento
    ]);
    let c = convertir_excel(&wb.save_to_buffer().unwrap()).unwrap();
    assert!(
        c.avisos.iter().any(|a| a.contains("comparten") && a.contains("111")),
        "la colisión de documento debe avisarse: {:?}",
        c.avisos
    );
}
