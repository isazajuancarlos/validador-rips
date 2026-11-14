// SPDX-FileCopyrightText: 2026 Juan Carlos Isaza
// SPDX-License-Identifier: LicenseRef-Propietario

//! # validador-rips
//!
//! Valida un **FEV-RIPS** (Resolución 948 de 2026) *antes* de enviarlo al MUV de
//! MinSalud, para cazar de una vez lo que el Ministerio rechazaría por
//! **estructura**, **campos faltantes**, **campos no permitidos** o un **valor
//! fuera de la tabla cerrada** de la norma —los motivos que hunden un archivo
//! entero sin decir cuál de sus miles de líneas tiene la culpa—.
//!
//! ## Qué caza, y qué NO
//!
//! El oráculo DEFINITIVO de rechazo es el propio MUV (el validador de MinSalud,
//! que además cruza catálogos y reglas RVC). Esto es el **pre-vuelo**: caza los
//! errores de FORMA, que son la mayoría de los rechazos y los más tontos de
//! descubrir el día del cierre de mes. No promete «soy MinSalud»; promete «no
//! envíes esto todavía, porque por estas razones te lo van a devolver».
//!
//! La aguja **no se escribe a mano**: sale de [`NORMA`], el artefacto
//! `campos_fev948.json` extraído del Documento Técnico 1 de la Resolución 948
//! —público, del Ministerio—. Si el JSON no se cargara, se compararía contra el
//! conjunto vacío y todo saldría verde: por eso [`Validador::de_la_948`] falla
//! ruidosamente si la norma viene vacía (directiva 30).
//!
//! ## Uso
//!
//! ```
//! use validador_rips::Validador;
//! let v = Validador::de_la_948();
//! let rips: serde_json::Value = serde_json::from_str(r#"{
//!     "numDocumentoIdObligado": "900000000",
//!     "usuarios": []
//! }"#).unwrap();
//! let faltas = v.validar(&rips);
//! assert!(faltas.is_empty()); // sin usuarios no hay servicios que revisar
//! ```

use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;
use serde_json::Value;

/// Convertidor de la plantilla Excel oficial (SPConvertidorJSON) a JSON RIPS.
/// Tras el `feature` `xlsx`: el núcleo de validación no lo necesita.
#[cfg(feature = "xlsx")]
pub mod conversion;

/// El artefacto de la norma, vendorizado: los campos que la Resolución 948 nombra
/// para cada arreglo de servicios. Extraído del Documento Técnico 1 (público).
pub const NORMA: &str = include_str!("../datos/campos_fev948.json");

/// Las tablas de dominio CERRADAS de la 948, vendorizadas: qué códigos admite cada
/// campo enumerado. Derivadas de las tablas REFs del contenedor oficial del MUV (vía
/// el artefacto verificado de `medico-dominio`). Solo pertenencia a lista cerrada
/// —NO cruces (finalidad×CUPS), NO restricción por arreglo/sexo/edad, NO la
/// distinción usuario/profesional del tipo de documento—: todo eso es regla del MUV,
/// no forma, y este validador es el pre-vuelo de FORMA.
pub const DOMINIOS: &str = include_str!("../datos/dominios948.json");

/// Campos que la norma nombra pero cuya AUSENCIA no es rechazo hoy.
///
/// `codigoVIDA` es el id único de la atención en el sistema IHCE; la norma dice
/// que «será obligatorio una vez entre en operación». Hoy es opcional: exigirlo
/// marcaría como error un archivo que el MUV acepta. Si está, se admite (es campo
/// de la norma); si falta, no se cuenta como falta.
pub const FALTAN_CON_MOTIVO: &[&str] = &["codigoVIDA"];

/// Una razón por la que el MUV rechazaría el archivo. Serializa a JSON para que la
/// capa web la muestre tal cual.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub enum Falta {
    /// El documento no tiene la forma de un RIPS (no es objeto, falta `usuarios`,
    /// un servicio no es una lista…). Sin esto, el resto de la validación mide
    /// sobre algo que no existe.
    Estructura {
        /// Ruta JSON donde falla (p. ej. `usuarios[0].servicios`).
        donde: String,
        detalle: String,
    },
    /// Un campo que la Resolución 948 nombra no está en el objeto de servicio. Un
    /// campo que se perdió: el MUV lo puede aceptar como opcional o rechazarlo,
    /// pero es el sitio más fácil por donde se cuela una atención incompleta.
    CampoFaltante {
        /// Ruta JSON del objeto de servicio (p. ej. `usuarios[0].servicios.consultas[2]`).
        donde: String,
        campo: String,
    },
    /// Un campo que la Resolución 948 NO nombra. El MUV rechaza el archivo
    /// **completo** con «Propiedades no permitidas en la estructura».
    CampoNoPermitido {
        /// Ruta JSON del objeto (p. ej. `usuarios[0].servicios.consultas[2]`).
        donde: String,
        campo: String,
    },
    /// El campo trae un valor que NO está en la tabla cerrada que la 948 fija para
    /// él (p. ej. una `finalidadTecnologiaSalud` `"10"`, que era el código de la
    /// tabla anterior y no existe en la del MUV). El MUV lo rechaza contra el
    /// catálogo. Solo se comprueba **pertenencia**: no se juzga si el código, aun
    /// siendo válido, aplica a ESE arreglo, sexo o edad —eso es regla del MUV—.
    ValorNoPermitido {
        /// Ruta JSON del objeto que trae el campo.
        donde: String,
        campo: String,
        /// El valor que traía y que no está en la tabla.
        valor: String,
    },
}

/// Comprueba un FEV-RIPS contra los campos de la Resolución 948.
pub struct Validador {
    /// Campos permitidos del SOBRE (nivel superior, hermanos de `usuarios`).
    sobre: BTreeSet<String>,
    /// Campos permitidos de cada objeto de `usuarios` (hermanos de `servicios`).
    usuario: BTreeSet<String>,
    /// Arreglo de servicios → conjunto de campos permitidos.
    arreglos: BTreeMap<String, BTreeSet<String>>,
    /// Campos cuya AUSENCIA es legítima (condicionales). Un campo requerido es el
    /// que está permitido y NO es opcional. Ver [`Validador::requerido`].
    opcionales: BTreeSet<String>,
    /// Campo enumerado → conjunto de códigos que admite su tabla cerrada de la 948.
    /// Un campo que no esté aquí no se valida por valor (solo por presencia).
    dominios: BTreeMap<String, BTreeSet<String>>,
}

/// El texto de un valor de código: la cadena tal cual, o los dígitos de un número.
///
/// Un código de la 948 es SIEMPRE una cadena (`"01"`, `"44"`), pero puede llegar
/// como número: Excel guarda una celda de código en formato numérico, y el
/// convertidor la emite como número JSON (ver `conversion::celda_a_json`). Comparar
/// el TEXTO en ambos casos evita el falso verde de dejar pasar en silencio un
/// código no-cadena —y de paso caza el cero a la izquierda perdido: `"01"` que se
/// volvió `1` da `"1"`, que no está en la tabla y el MUV rechaza—. `None` para
/// `null`, booleano, lista u objeto: eso no es un código que esta falta pueda juzgar
/// (su ausencia o su forma la miran las otras faltas).
fn texto_de_codigo(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

impl Validador {
    /// Construye el validador desde la norma vendorizada ([`NORMA`]).
    ///
    /// # Pánico
    ///
    /// Si la norma no es JSON, o le falta cualquiera de los tres bloques de campos
    /// (`sobre`, `usuario`, `arreglos`) o alguno viene vacío. Un validador
    /// construido sobre un conjunto vacío no distingue: marcaría TODO campo real
    /// como «no permitido», o no exigiría ninguno. Peor que no validar
    /// (directiva 30). El pánico es de arranque —la norma es un artefacto del
    /// binario, no un dato que suba nadie—.
    pub fn de_la_948() -> Self {
        let norma: Value = serde_json::from_str(NORMA).expect("campos_fev948.json no es JSON válido");
        let lista = |clave: &str| -> BTreeSet<String> {
            norma
                .get(clave)
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("la norma no trae la lista «{clave}»"))
                .iter()
                .map(|c| c.as_str().expect("campo de la norma que no es cadena").to_string())
                .collect()
        };
        let sobre = lista("sobre");
        let usuario = lista("usuario");
        assert!(!sobre.is_empty() && !usuario.is_empty(), "sobre/usuario vacíos (directiva 30)");

        let arr = norma
            .get("arreglos")
            .and_then(Value::as_object)
            .expect("la norma no trae el objeto «arreglos»");
        let mut arreglos = BTreeMap::new();
        for (tipo, l) in arr {
            let set: BTreeSet<String> = l
                .as_array()
                .unwrap_or_else(|| panic!("el arreglo «{tipo}» no es una lista"))
                .iter()
                .map(|c| c.as_str().expect("campo que no es cadena").to_string())
                .collect();
            arreglos.insert(tipo.clone(), set);
        }
        assert!(!arreglos.is_empty(), "cero arreglos: no validaría nada (directiva 30)");

        let mut opcionales: BTreeSet<String> = norma
            .get("opcionales")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        opcionales.extend(FALTAN_CON_MOTIVO.iter().map(|s| (*s).to_string()));

        let dominios = Self::cargar_dominios();

        Self { sobre, usuario, arreglos, opcionales, dominios }
    }

    /// Carga las tablas de dominio cerradas desde [`DOMINIOS`].
    ///
    /// # Pánico
    ///
    /// Si el artefacto no es JSON, le falta el objeto `dominios`, viene vacío o
    /// alguna tabla viene vacía. Igual que la norma: un dominio vacío no marcaría
    /// ningún valor como fuera de catálogo —comparar contra el conjunto vacío da
    /// verde a todo— y sería peor que no validar (directiva 30). Es artefacto del
    /// binario, no dato que suba nadie: el pánico es de arranque.
    fn cargar_dominios() -> BTreeMap<String, BTreeSet<String>> {
        let raiz: Value = serde_json::from_str(DOMINIOS).expect("dominios948.json no es JSON válido");
        let obj = raiz
            .get("dominios")
            .and_then(Value::as_object)
            .expect("el artefacto de dominios no trae el objeto «dominios»");
        let mut dominios = BTreeMap::new();
        for (campo, lista) in obj {
            let set: BTreeSet<String> = lista
                .as_array()
                .unwrap_or_else(|| panic!("el dominio «{campo}» no es una lista"))
                .iter()
                .map(|c| c.as_str().expect("código de dominio que no es cadena").to_string())
                .collect();
            assert!(!set.is_empty(), "el dominio «{campo}» viene vacío (directiva 30)");
            dominios.insert(campo.clone(), set);
        }
        assert!(!dominios.is_empty(), "cero dominios: no validaría ningún valor (directiva 30)");
        dominios
    }

    /// Un campo es REQUERIDO si está permitido y no es opcional. Además, TODO
    /// campo de la transición CIE-11 (`*CIE11` y su nombre) es opcional: es un
    /// dato derivado del CIE-10 que un RIPS real omite sin que el MUV lo rechace.
    /// Medido contra un archivo real (2026-09-23): exigirlos daba 468 faltas
    /// falsas sobre un RIPS que el MUV aceptó (directiva 23).
    fn requerido(&self, campo: &str) -> bool {
        !self.opcionales.contains(campo) && !campo.contains("CIE11")
    }

    /// Revisa un objeto contra su conjunto de campos permitidos: marca lo que
    /// FALTA y es requerido, y lo que SOBRA (no permitido → rechazo del MUV).
    /// `ignora` son claves estructurales del objeto que no son campos de dato
    /// (p. ej. `usuarios` en el sobre, `servicios` en el usuario).
    fn revisar(&self, donde: &str, obj: &serde_json::Map<String, Value>, permitidos: &BTreeSet<String>, ignora: &[&str], faltas: &mut Vec<Falta>) {
        let presentes: BTreeSet<String> =
            obj.keys().filter(|k| !ignora.contains(&k.as_str())).cloned().collect();
        for campo in permitidos {
            if !presentes.contains(campo) && self.requerido(campo) {
                faltas.push(Falta::CampoFaltante { donde: donde.to_string(), campo: campo.clone() });
            }
        }
        for campo in presentes.difference(permitidos) {
            faltas.push(Falta::CampoNoPermitido { donde: donde.to_string(), campo: campo.clone() });
        }
        // Valor fuera de tabla cerrada: solo de campos presentes Y permitidos (un
        // campo no permitido ya se marcó arriba; no se le mide además el valor).
        // El valor se compara por su TEXTO (`texto_de_codigo`), así que un código
        // que llegue como número JSON SÍ se juzga —y se caza el cero a la izquierda
        // perdido (`"01"`→`1`→`"1"`, que el MUV rechaza)—. Solo `null`, booleano,
        // lista u objeto quedan fuera: no son un código que esta falta pueda juzgar.
        for campo in presentes.intersection(permitidos) {
            let Some(codigos) = self.dominios.get(campo) else { continue };
            let Some(valor) = obj.get(campo).and_then(texto_de_codigo) else { continue };
            if !codigos.contains(&valor) {
                faltas.push(Falta::ValorNoPermitido {
                    donde: donde.to_string(),
                    campo: campo.clone(),
                    valor,
                });
            }
        }
    }

    /// Valida un RIPS ya parseado y devuelve TODAS las faltas encontradas (no se
    /// corta en la primera: el prestador quiere arreglarlas de una pasada).
    pub fn validar(&self, rips: &Value) -> Vec<Falta> {
        let mut faltas = Vec::new();
        let Some(raiz) = rips.as_object() else {
            faltas.push(Falta::Estructura {
                donde: "(raíz)".to_string(),
                detalle: "el RIPS no es un objeto JSON".to_string(),
            });
            return faltas;
        };
        // El SOBRE: campos de nivel superior (todo menos `usuarios`, que es la lista).
        self.revisar("(sobre)", raiz, &self.sobre, &["usuarios"], &mut faltas);

        let Some(usuarios) = raiz.get("usuarios") else {
            faltas.push(Falta::Estructura {
                donde: "usuarios".to_string(),
                detalle: "falta el arreglo «usuarios»".to_string(),
            });
            return faltas;
        };
        let Some(usuarios) = usuarios.as_array() else {
            faltas.push(Falta::Estructura {
                donde: "usuarios".to_string(),
                detalle: "«usuarios» no es una lista".to_string(),
            });
            return faltas;
        };
        for (i, usuario) in usuarios.iter().enumerate() {
            self.validar_usuario(i, usuario, &mut faltas);
        }
        faltas
    }

    fn validar_usuario(&self, i: usize, usuario: &Value, faltas: &mut Vec<Falta>) {
        let Some(usuario) = usuario.as_object() else {
            faltas.push(Falta::Estructura {
                donde: format!("usuarios[{i}]"),
                detalle: "el usuario no es un objeto JSON".to_string(),
            });
            return;
        };
        // Campos propios del usuario (todo menos `servicios`, que es el objeto anidado).
        self.revisar(&format!("usuarios[{i}]"), usuario, &self.usuario, &["servicios"], faltas);

        let Some(servicios) = usuario.get("servicios") else {
            faltas.push(Falta::Estructura {
                donde: format!("usuarios[{i}].servicios"),
                detalle: "el usuario no trae «servicios»".to_string(),
            });
            return;
        };
        let Some(servicios) = servicios.as_object() else {
            faltas.push(Falta::Estructura {
                donde: format!("usuarios[{i}].servicios"),
                detalle: "«servicios» no es un objeto".to_string(),
            });
            return;
        };
        for (tipo, valor) in servicios {
            let base = format!("usuarios[{i}].servicios.{tipo}");
            let Some(permitidos) = self.arreglos.get(tipo) else {
                faltas.push(Falta::CampoNoPermitido {
                    donde: format!("usuarios[{i}].servicios"),
                    campo: tipo.clone(),
                });
                continue;
            };
            let Some(lista) = valor.as_array() else {
                faltas.push(Falta::Estructura {
                    donde: base,
                    detalle: format!("«{tipo}» no es una lista de servicios"),
                });
                continue;
            };
            for (j, servicio) in lista.iter().enumerate() {
                let donde = format!("{base}[{j}]");
                let Some(obj) = servicio.as_object() else {
                    faltas.push(Falta::Estructura {
                        donde,
                        detalle: "el servicio no es un objeto JSON".to_string(),
                    });
                    continue;
                };
                self.revisar(&donde, obj, permitidos, &[], faltas);
            }
        }
    }
}

#[cfg(test)]
mod pruebas;
