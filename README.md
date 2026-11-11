# validador-rips

**"Upload your RIPS and I'll tell you what MinSalud would reject — before you
send it."**

**English** · [Español](#español)

---

## What it is

A pre-flight validator for Colombian health billing files (FEV-RIPS, Resolution
948 of 2026). You upload or paste a RIPS; it checks it field by field against the
norm and returns exactly what would bounce — a missing field, a field that isn't
allowed (which rejects the whole record), a structural problem — so you fix it
*before* the official rejection, not after.

It is **pre-flight, not "I am MinSalud"**: it catches shape and conformance; the
final authority is the MUV. It also converts the official Excel template to the
JSON the chain expects.

## Try it

It runs as a free self-serve tool — upload or paste, get the list of faults,
nothing stored. **[rips.xiliux.com](https://rips.xiliux.com)**

## This repository

Public home of the project. The source is not published here. More at
**[xiliux.com](https://xiliux.com)**.

---

## Español

**«Sube tu RIPS y te digo qué rechazaría MinSalud, antes de enviarlo.»**

### Qué es

Un validador de pre-vuelo para los archivos de facturación en salud de Colombia
(FEV-RIPS, Resolución 948 de 2026). Subes o pegas un RIPS; lo revisa campo por
campo contra la norma y te devuelve exactamente lo que rebotaría —un campo que
falta, un campo no permitido (que rechaza el registro entero), un problema de
estructura— para que lo arregles *antes* del rechazo oficial, no después.

Es **pre-vuelo, no «soy MinSalud»**: caza forma y conformidad; el oráculo
definitivo es el MUV. También convierte la plantilla oficial de Excel al JSON que
espera la cadena.

### Pruébalo

Funciona como herramienta libre de autoservicio —subes o pegas, obtienes la lista
de faltas, no se guarda nada—. **[rips.xiliux.com](https://rips.xiliux.com)**

### Este repositorio

Casa pública del proyecto. El código no se publica aquí. Más en
**[xiliux.com](https://xiliux.com)**.

---

© 2026 Juan Carlos Isaza. Proprietary — all rights reserved.
