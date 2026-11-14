// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
// SPDX-License-Identifier: LicenseRef-Propietario

//! Pruebas del validador, cada regla con su pareja (directiva 33): un caso que
//! TIENE que pasar y uno que TIENE que fallar por la vía real.
//!
//! Los fixtures (consulta, usuario, sobre) se escriben A MANO —los campos de la
//! 948, no derivados del propio archivo que se valida—: si el fixture saliera de
//! la misma lista contra la que se compara, un fallo del cargador de la norma no
//! se vería (los dos lados moverían juntos).

use serde_json::{json, Value};

use super::*;

/// Una consulta con los campos que la Resolución 948 nombra para `consultas`, con
/// valores de relleno (el validador mira las CLAVES, no los valores). Los CIE-11 y
/// los relacionados van a `null`/ausentes: son opcionales, y un RIPS real los omite.
fn consulta_completa() -> Value {
    json!({
        "codPrestador": "1",
        "fechaInicioAtencion": "2026-01-06 08:00",
        "codConsulta": "890201",
        "modalidadGrupoServicioTecSal": "01",
        "grupoServicios": "01",
        "codServicio": "334",
        "finalidadTecnologiaSalud": "44",
        "causaMotivoAtencion": "38",
        "codDiagnosticoPrincipal": "Z000",
        "tipoDiagnosticoPrincipal": "01",
        "tipoDocumentoIdentificacion": "CC",
        "numDocumentoIdentificacion": "123",
        "vrServicio": 0,
        "conceptoRecaudo": "05",
        "valorPagoModerador": 0,
        "consecutivo": 1
    })
}

/// Un usuario con sus campos de identidad requeridos por la 948 (todos menos los
/// opcionales como `registroSIRAS`), y la bolsa de servicios que se le pase.
fn usuario_completo(servicios: Value) -> Value {
    json!({
        "tipoDocumentoIdentificacion": "CC",
        "numDocumentoIdentificacion": "123",
        "tipoUsuario": "12",
        "fechaNacimiento": "2000-01-01",
        "codSexo": "M",
        "codPaisResidencia": "170",
        "codMunicipioResidencia": "05001",
        "codZonaTerritorialResidencia": "02",
        "incapacidad": "NO",
        "codPaisOrigen": "170",
        "consecutivo": 1,
        "servicios": servicios
    })
}

/// Un RIPS completo: sobre + un usuario con las consultas dadas.
fn rips_con_consultas(consultas: Value) -> Value {
    json!({
        "numDocumentoIdObligado": "900000000",
        "numNota": "1",
        "tipoNota": "RS",
        "usuarios": [ usuario_completo(json!({ "consultas": consultas })) ]
    })
}

// ───────────────────────── positivo de control de la NORMA ─────────────────────

#[test]
fn la_norma_carga_los_tres_bloques() {
    // Directiva 30: si esto no cargara, todo saldría verde por comparar contra
    // vacío. Es el positivo de control del artefacto.
    let v = Validador::de_la_948();
    assert_eq!(v.arreglos.len(), 7, "arreglos: {:?}", v.arreglos.keys());
    assert!(v.arreglos["consultas"].contains("codConsulta"));
    assert!(v.sobre.contains("numDocumentoIdObligado"));
    assert!(v.usuario.contains("numDocumentoIdentificacion"));
    assert!(!v.opcionales.is_empty());
}

/// Una hospitalización con sus 6 campos requeridos (948), con valores de dominio
/// VÁLIDOS. `condicionDestinoUsuarioEgreso` es opcional pero se incluye para poder
/// probar su dominio. Los códigos son reales: vía `01`, causa `21`, condición `01`.
fn hospitalizacion_completa() -> Value {
    json!({
        "codPrestador": "1",
        "viaIngresoServicioSalud": "01",
        "fechaInicioAtencion": "2026-01-06 08:00",
        "causaMotivoAtencion": "21",
        "codDiagnosticoPrincipal": "Z000",
        "condicionDestinoUsuarioEgreso": "01",
        "consecutivo": 1
    })
}

fn rips_con_hospitalizacion(h: Value) -> Value {
    json!({
        "numDocumentoIdObligado": "900000000",
        "usuarios": [ usuario_completo(json!({ "hospitalizacion": [h] })) ]
    })
}

// ───────────────────────── el par faltante / no permitido ──────────────────────

#[test]
fn un_rips_completo_no_tiene_faltas() {
    let v = Validador::de_la_948();
    let faltas = v.validar(&rips_con_consultas(json!([consulta_completa()])));
    assert!(faltas.is_empty(), "un RIPS completo no debería tener faltas: {faltas:?}");
}

#[test]
fn un_campo_de_servicio_que_falta_se_reporta() {
    // ROJO: quitar `codConsulta` (requerido, no opcional) → exactamente una falta.
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().remove("codConsulta");
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert_eq!(
        faltas,
        vec![Falta::CampoFaltante {
            donde: "usuarios[0].servicios.consultas[0]".to_string(),
            campo: "codConsulta".to_string(),
        }]
    );
}

#[test]
fn un_campo_de_mas_lo_rechaza_el_muv() {
    // ROJO: añadir un campo que la 948 no nombra → el MUV rechaza el archivo entero.
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().insert("campoInventado".to_string(), json!("x"));
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert_eq!(
        faltas,
        vec![Falta::CampoNoPermitido {
            donde: "usuarios[0].servicios.consultas[0]".to_string(),
            campo: "campoInventado".to_string(),
        }]
    );
}

#[test]
fn discrimina_faltante_de_no_permitido_a_la_vez() {
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().remove("codServicio");
    c.as_object_mut().unwrap().insert("extra".to_string(), json!(1));
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert!(faltas.contains(&Falta::CampoFaltante {
        donde: "usuarios[0].servicios.consultas[0]".to_string(),
        campo: "codServicio".to_string(),
    }));
    assert!(faltas.contains(&Falta::CampoNoPermitido {
        donde: "usuarios[0].servicios.consultas[0]".to_string(),
        campo: "extra".to_string(),
    }));
    assert_eq!(faltas.len(), 2);
}

// ───────────────────────── requerido vs OPCIONAL ───────────────────────────────

#[test]
fn los_campos_opcionales_ausentes_no_son_falta() {
    // La consulta completa NO trae CIE-11 ni relacionados ni numAutorizacion: son
    // opcionales. Un RIPS real (verificado 2026-09-23) los omite y el MUV lo acepta;
    // exigirlos era 468 faltas falsas (directiva 23).
    let v = Validador::de_la_948();
    assert!(v.validar(&rips_con_consultas(json!([consulta_completa()]))).is_empty());
    // Control: los opcionales SÍ están permitidos si aparecen (no dan no-permitido).
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().insert("codDiagnosticoPrincipalCIE11".to_string(), json!("QA00"));
    c.as_object_mut().unwrap().insert("codDiagnosticoRelacionado1".to_string(), json!("Z001"));
    c.as_object_mut().unwrap().insert("numAutorizacion".to_string(), json!("A1"));
    assert!(v.validar(&rips_con_consultas(json!([c]))).is_empty());
}

#[test]
fn codigo_vida_ausente_no_es_falta_pero_lo_demas_si() {
    let v = Validador::de_la_948();
    // La consulta completa ya viene sin codigoVIDA y aun así sale limpia.
    assert!(v.validar(&rips_con_consultas(json!([consulta_completa()]))).is_empty());
    // control: quitar además uno requerido sí produce falta.
    let mut c2 = consulta_completa();
    c2.as_object_mut().unwrap().remove("codConsulta");
    let faltas = v.validar(&rips_con_consultas(json!([c2])));
    assert_eq!(faltas.len(), 1);
    assert!(matches!(&faltas[0], Falta::CampoFaltante { campo, .. } if campo == "codConsulta"));
}

// ───────────────────────── el SOBRE (nivel superior) ───────────────────────────

#[test]
fn el_sobre_sin_num_documento_obligado_es_falta() {
    // ROJO: el NIT del obligado es lo único siempre requerido del sobre.
    let v = Validador::de_la_948();
    let mut rips = rips_con_consultas(json!([consulta_completa()]));
    rips.as_object_mut().unwrap().remove("numDocumentoIdObligado");
    let faltas = v.validar(&rips);
    assert!(faltas.contains(&Falta::CampoFaltante {
        donde: "(sobre)".to_string(),
        campo: "numDocumentoIdObligado".to_string(),
    }));
}

#[test]
fn un_campo_de_mas_en_el_sobre_no_se_permite() {
    let v = Validador::de_la_948();
    let mut rips = rips_con_consultas(json!([consulta_completa()]));
    rips.as_object_mut().unwrap().insert("campoRaro".to_string(), json!("x"));
    let faltas = v.validar(&rips);
    assert!(faltas.contains(&Falta::CampoNoPermitido {
        donde: "(sobre)".to_string(),
        campo: "campoRaro".to_string(),
    }));
}

#[test]
fn el_sobre_sin_factura_es_valido() {
    // PAREJA: numFactura/tipoNota/numNota son opcionales (RIPS es con-factura O
    // sin-factura): su ausencia NO se marca. El fixture es «sin factura» y pasa.
    let v = Validador::de_la_948();
    assert!(v.validar(&rips_con_consultas(json!([consulta_completa()]))).is_empty());
}

// ───────────────────────── el USUARIO ──────────────────────────────────────────

#[test]
fn un_usuario_sin_sexo_es_falta() {
    // ROJO: `codSexo` es requerido, sin fallback.
    let v = Validador::de_la_948();
    let mut u = usuario_completo(json!({ "consultas": [consulta_completa()] }));
    u.as_object_mut().unwrap().remove("codSexo");
    let rips = json!({ "numDocumentoIdObligado": "900", "usuarios": [u] });
    let faltas = v.validar(&rips);
    assert!(faltas.contains(&Falta::CampoFaltante {
        donde: "usuarios[0]".to_string(),
        campo: "codSexo".to_string(),
    }));
}

#[test]
fn un_campo_de_mas_en_el_usuario_no_se_permite() {
    let v = Validador::de_la_948();
    let mut u = usuario_completo(json!({ "consultas": [consulta_completa()] }));
    u.as_object_mut().unwrap().insert("apodo".to_string(), json!("x"));
    let rips = json!({ "numDocumentoIdObligado": "900", "usuarios": [u] });
    let faltas = v.validar(&rips);
    assert!(faltas.contains(&Falta::CampoNoPermitido {
        donde: "usuarios[0]".to_string(),
        campo: "apodo".to_string(),
    }));
}

#[test]
fn registro_siras_ausente_no_es_falta() {
    // PAREJA del de arriba: registroSIRAS es opcional (solo casos SIRAS). El
    // usuario_completo NO lo trae y aun así sale limpio.
    let v = Validador::de_la_948();
    assert!(v.validar(&rips_con_consultas(json!([consulta_completa()]))).is_empty());
}

// ───────────────────────── la estructura ───────────────────────────────────────

#[test]
fn un_rips_que_no_es_objeto_es_estructura() {
    let v = Validador::de_la_948();
    assert!(matches!(v.validar(&json!([]))[..], [Falta::Estructura { .. }]));
    assert!(matches!(v.validar(&json!("hola"))[..], [Falta::Estructura { .. }]));
}

#[test]
fn sin_usuarios_es_estructura() {
    let v = Validador::de_la_948();
    let faltas = v.validar(&json!({ "numDocumentoIdObligado": "900" }));
    assert!(matches!(&faltas[..], [Falta::Estructura { donde, .. }] if donde == "usuarios"));
}

#[test]
fn un_tipo_de_servicio_desconocido_no_se_permite() {
    let v = Validador::de_la_948();
    let rips = json!({
        "numDocumentoIdObligado": "900",
        "usuarios": [ usuario_completo(json!({ "teletransporte": [] })) ]
    });
    let faltas = v.validar(&rips);
    assert_eq!(
        faltas,
        vec![Falta::CampoNoPermitido {
            donde: "usuarios[0].servicios".to_string(),
            campo: "teletransporte".to_string(),
        }]
    );
}

#[test]
fn un_servicio_que_no_es_lista_es_estructura() {
    let v = Validador::de_la_948();
    let rips = json!({
        "numDocumentoIdObligado": "900",
        "usuarios": [ usuario_completo(json!({ "consultas": { "no": "soy lista" } })) ]
    });
    let faltas = v.validar(&rips);
    assert!(matches!(&faltas[..], [Falta::Estructura { donde, .. }]
        if donde == "usuarios[0].servicios.consultas"));
}

#[test]
fn un_usuario_sin_servicios_es_estructura() {
    // El usuario está completo salvo `servicios`: así la única falta es la
    // estructural, no un montón de faltantes de identidad.
    let v = Validador::de_la_948();
    let mut u = usuario_completo(json!({}));
    u.as_object_mut().unwrap().remove("servicios");
    let rips = json!({ "numDocumentoIdObligado": "900", "usuarios": [u] });
    let faltas = v.validar(&rips);
    assert!(matches!(&faltas[..], [Falta::Estructura { donde, .. }]
        if donde == "usuarios[0].servicios"));
}

// ───────────────────── VALOR fuera de tabla cerrada (dominios 948) ──────────────

#[test]
fn los_dominios_cargan_con_sus_longitudes() {
    // Positivo de control del artefacto (directiva 30): si `dominios948.json` no
    // cargara, todo valor saldría verde por comparar contra vacío. Longitudes de
    // las tablas REFs del MUV: finalidad 34, causa 29, vía 14, condición 8, tipoDoc 13.
    let v = Validador::de_la_948();
    assert_eq!(v.dominios.len(), 5, "dominios: {:?}", v.dominios.keys());
    assert_eq!(v.dominios["finalidadTecnologiaSalud"].len(), 34);
    assert_eq!(v.dominios["causaMotivoAtencion"].len(), 29);
    assert_eq!(v.dominios["viaIngresoServicioSalud"].len(), 14);
    assert_eq!(v.dominios["condicionDestinoUsuarioEgreso"].len(), 8);
    assert_eq!(v.dominios["tipoDocumentoIdentificacion"].len(), 13);
    // La aguja se valida contra el artefacto (directiva 33): `44` y `38` SÍ están
    // (son los del fixture limpio); `10` y `13` NO —los códigos de la tabla ANTERIOR
    // a la 948, que la migración 0038 de medico documenta como inexistentes en la del MUV—.
    assert!(v.dominios["finalidadTecnologiaSalud"].contains("44"));
    assert!(!v.dominios["finalidadTecnologiaSalud"].contains("10"));
    assert!(v.dominios["causaMotivoAtencion"].contains("38"));
    assert!(!v.dominios["causaMotivoAtencion"].contains("13"));
}

#[test]
fn finalidad_fuera_de_tabla_es_valor_no_permitido() {
    // ROJO por la vía real: `10` es el código de la tabla vieja, ausente del MUV.
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().insert("finalidadTecnologiaSalud".to_string(), json!("10"));
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert_eq!(
        faltas,
        vec![Falta::ValorNoPermitido {
            donde: "usuarios[0].servicios.consultas[0]".to_string(),
            campo: "finalidadTecnologiaSalud".to_string(),
            valor: "10".to_string(),
        }]
    );
    // PAREJA (control): `44` es válido → sin falta. El fixture ya lo trae, y el RIPS
    // completo sale limpio, así que un solo cambio del valor es lo que dispara.
    assert!(v.validar(&rips_con_consultas(json!([consulta_completa()]))).is_empty());
}

#[test]
fn causa_fuera_de_tabla_es_valor_no_permitido() {
    // ROJO: `13` (ENFERMEDAD GENERAL en la tabla vieja) no existe en REFs_CausaMotivoAtencion.
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().insert("causaMotivoAtencion".to_string(), json!("13"));
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert_eq!(
        faltas,
        vec![Falta::ValorNoPermitido {
            donde: "usuarios[0].servicios.consultas[0]".to_string(),
            campo: "causaMotivoAtencion".to_string(),
            valor: "13".to_string(),
        }]
    );
}

#[test]
fn tipo_documento_del_usuario_fuera_de_tabla_cae() {
    // El dominio también se comprueba a NIVEL USUARIO, no solo en servicios.
    let v = Validador::de_la_948();
    let mut u = usuario_completo(json!({ "consultas": [consulta_completa()] }));
    u.as_object_mut().unwrap().insert("tipoDocumentoIdentificacion".to_string(), json!("XX"));
    let rips = json!({ "numDocumentoIdObligado": "900", "usuarios": [u] });
    let faltas = v.validar(&rips);
    assert!(faltas.contains(&Falta::ValorNoPermitido {
        donde: "usuarios[0]".to_string(),
        campo: "tipoDocumentoIdentificacion".to_string(),
        valor: "XX".to_string(),
    }));
    // PAREJA: `CC` es válido (el fixture lo trae) → el usuario limpio no da esta falta.
    assert!(v.validar(&rips_con_consultas(json!([consulta_completa()]))).is_empty());
}

#[test]
fn hospitalizacion_con_via_y_condicion_validas_no_tiene_faltas() {
    // Positivo de control del fixture de hospitalización: vía `01` + condición `01`
    // + causa `21`, todos en tabla → limpio. Sin esto, los rojos de abajo no
    // discriminarían (podrían fallar por otro campo).
    let v = Validador::de_la_948();
    let faltas = v.validar(&rips_con_hospitalizacion(hospitalizacion_completa()));
    assert!(faltas.is_empty(), "hospitalización válida no debería tener faltas: {faltas:?}");
}

#[test]
fn via_de_ingreso_fuera_de_tabla_cae_por_la_via_real() {
    // ROJO: `99` no está en REFs_ViaIngresoServicioSalud (01..14).
    let v = Validador::de_la_948();
    let mut h = hospitalizacion_completa();
    h.as_object_mut().unwrap().insert("viaIngresoServicioSalud".to_string(), json!("99"));
    let faltas = v.validar(&rips_con_hospitalizacion(h));
    assert_eq!(
        faltas,
        vec![Falta::ValorNoPermitido {
            donde: "usuarios[0].servicios.hospitalizacion[0]".to_string(),
            campo: "viaIngresoServicioSalud".to_string(),
            valor: "99".to_string(),
        }]
    );
}

#[test]
fn condicion_destino_fuera_de_tabla_cae() {
    // ROJO: `09` no está en REFs_CondicionDestino (01..08). Y prueba de paso que un
    // campo OPCIONAL (condición se puede omitir) sí se valida por VALOR si está presente.
    let v = Validador::de_la_948();
    let mut h = hospitalizacion_completa();
    h.as_object_mut().unwrap().insert("condicionDestinoUsuarioEgreso".to_string(), json!("09"));
    let faltas = v.validar(&rips_con_hospitalizacion(h));
    assert_eq!(
        faltas,
        vec![Falta::ValorNoPermitido {
            donde: "usuarios[0].servicios.hospitalizacion[0]".to_string(),
            campo: "condicionDestinoUsuarioEgreso".to_string(),
            valor: "09".to_string(),
        }]
    );
}

#[test]
fn un_valor_solo_se_juzga_en_campo_permitido_no_doble() {
    // Un campo NO permitido con valor raro se marca UNA vez (no permitido), no dos:
    // la validación de valor es solo sobre presentes ∩ permitidos.
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    // `condicionDestinoUsuarioEgreso` NO es campo de `consultas` (es de hosp/urgencias).
    c.as_object_mut().unwrap().insert("condicionDestinoUsuarioEgreso".to_string(), json!("09"));
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert_eq!(
        faltas,
        vec![Falta::CampoNoPermitido {
            donde: "usuarios[0].servicios.consultas[0]".to_string(),
            campo: "condicionDestinoUsuarioEgreso".to_string(),
        }]
    );
}

#[test]
fn un_codigo_que_viene_como_numero_se_compara_por_su_texto() {
    // Un código guardado en Excel como número llega como número JSON (lo emite el
    // convertidor). NO se salta en silencio (eso sería el falso verde de la D30): se
    // compara su TEXTO. Un número inválido cae igual que la cadena inválida.
    let v = Validador::de_la_948();
    let mut c = consulta_completa();
    c.as_object_mut().unwrap().insert("finalidadTecnologiaSalud".to_string(), json!(10));
    let faltas = v.validar(&rips_con_consultas(json!([c])));
    assert_eq!(
        faltas,
        vec![Falta::ValorNoPermitido {
            donde: "usuarios[0].servicios.consultas[0]".to_string(),
            campo: "finalidadTecnologiaSalud".to_string(),
            valor: "10".to_string(),
        }],
        "un código numérico inválido debe caer por su texto, no saltarse"
    );
    // PAREJA: un número que representa un código VÁLIDO (44) pasa —es el mismo código—.
    let mut ok = consulta_completa();
    ok.as_object_mut().unwrap().insert("finalidadTecnologiaSalud".to_string(), json!(44));
    assert!(v.validar(&rips_con_consultas(json!([ok]))).is_empty());
}

#[test]
fn un_codigo_que_perdio_su_cero_a_la_izquierda_cae() {
    // El caso REAL de Excel: `viaIngresoServicioSalud` "01" guardado como número se
    // vuelve 1 → "1", que no está en la tabla (que trae "01"). El MUV lo rechaza; el
    // validador ya no lo deja pasar. Es el falso verde que motivó el arreglo.
    let v = Validador::de_la_948();
    let mut h = hospitalizacion_completa();
    h.as_object_mut().unwrap().insert("viaIngresoServicioSalud".to_string(), json!(1));
    let faltas = v.validar(&rips_con_hospitalizacion(h));
    assert_eq!(
        faltas,
        vec![Falta::ValorNoPermitido {
            donde: "usuarios[0].servicios.hospitalizacion[0]".to_string(),
            campo: "viaIngresoServicioSalud".to_string(),
            valor: "1".to_string(),
        }]
    );
}

#[test]
fn varios_usuarios_reportan_su_indice() {
    let v = Validador::de_la_948();
    let mut mala = consulta_completa();
    mala.as_object_mut().unwrap().remove("codConsulta");
    let rips = json!({
        "numDocumentoIdObligado": "900",
        "usuarios": [
            usuario_completo(json!({ "consultas": [consulta_completa()] })),
            usuario_completo(json!({ "consultas": [mala] }))
        ]
    });
    let faltas = v.validar(&rips);
    assert_eq!(
        faltas,
        vec![Falta::CampoFaltante {
            donde: "usuarios[1].servicios.consultas[0]".to_string(),
            campo: "codConsulta".to_string(),
        }]
    );
}

// ───────────────────── blindaje: la estrategia de pánico es unwind ──────────────

#[test]
fn la_estrategia_de_panico_es_unwind() {
    // BLINDAJE de la contención de calamine (ver conversion.rs:104-110): la
    // conversión de un `.xlsx` anónimo corre bajo `catch_unwind`
    // (conversion.rs:111-115) para que un pánico interno de calamine ante entrada
    // adversaria se mapee a un error amable en vez de tumbar el hilo/proceso de la
    // petición. Esa defensa SOLO funciona con `panic = "unwind"` (el defecto de
    // Rust): con `panic = "abort"` el proceso aborta ANTES de que `catch_unwind`
    // pueda recibir nada, y la contención se vuelve un no-op silencioso.
    //
    // Este test afirma que ESTE build desenrolla. Que se ponga ROJO (o que aborte
    // en vez de fallar limpio) significaría que alguien añadió `panic = "abort"` a
    // un `[profile.*]` de Cargo.toml, rompiendo la contención de calamine. Hoy
    // Cargo.toml NO declara ninguna clave `panic`, así que rige el defecto unwind.
    //
    // NOTA: el pánico atrapado imprime una línea en stderr durante `cargo test`; es
    // esperado y no cambia el resultado. No se silencia con `set_hook` a propósito:
    // el hook es estado GLOBAL y tocarlo introduciría una carrera con el resto de
    // tests en paralelo (directiva 33: la prueba no debe depender de estado frágil).
    let atrapado = std::panic::catch_unwind(|| panic!("pánico de prueba: debe ser atrapado"));
    assert!(
        atrapado.is_err(),
        "catch_unwind no atrapó el pánico: este build usa panic=abort, lo que ANULA \
         la contención de calamine ante un .xlsx malformado (conversion.rs:111-115)"
    );
}
