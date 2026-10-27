//! `--explain --cause <code>`: what each closed cause means and what to do
//! about it (§10.1: text on stdout, exit 0, **no** verdict row — the `command`
//! enum of schema 1.0 only admits the five evaluators).

use crate::taxonomy::Cause;

/// The full explanation text for a cause, in Spanish, newline-terminated.
pub fn explain(cause: Cause) -> String {
    let (meaning, action) = match cause {
        Cause::BundleManifestInvalid => (
            "CONTRACT.json del bundle no es un manifiesto válido: falta, no está \
             serializado de forma canónica, tiene campos desconocidos o faltantes, \
             files[] se incluye a sí mismo, hay orden (M2/M3/M4) o tipos (sha256, \
             class, SemVer) violados, o anchors[] no coincide con facts/anchors.json.",
            "volver a generar el bundle en MAIN con `side-contract --generate`; \
             un bundle con manifiesto inválido no es transportable ni sincronizable.",
        ),
        Cause::BundleTampered => (
            "los bytes de un archivo del bundle no corresponden a su entrada en \
             CONTRACT.json: digest distinto, archivo listado ausente, archivo \
             extra, o violación de R1/R2/R3 (\\r, 0 o 2 \\n finales, BOM).",
            "descartar el bundle y volver a copiarlo desde MAIN; no se sincroniza \
             nada con un bundle alterado.",
        ),
        Cause::LockMissing => (
            "el side no tiene contract.lock: nunca fue sincronizado (o el side \
             directamente no existe). No es un error del side: es su estado real.",
            "correr `side-contract --sync --bundle <dir> --side <dir>` para \
             componer el side con el contrato vigente.",
        ),
        Cause::LockInvalid => (
            "contract.lock no es un lock válido: TOML ilegible, schema_version \
             distinto de \"1\", bundle_schema distinto de \"1\", campos faltantes, \
             files[] sin ordenar o sha256 malformado.",
            "un lock solo lo escribe --sync: restaurarlo desde git o volver a \
             sincronizar con `--sync --force` (explícito, nunca silencioso).",
        ),
        Cause::LockAhead => (
            "el contract.lock del side es MÁS nuevo que el bundle ofrecido: el \
             side viene del futuro y --sync nunca degrada sin permiso.",
            "ofrecer un bundle igual o más nuevo, o aceptar la degradación con \
             `--allow-downgrade` cuando sea deliberada.",
        ),
        Cause::LockStale => (
            "el contract.lock del side es más viejo que el bundle: MAIN avanzó y \
             el side no re-sincronizó. La clase (patch/minor/major) dice cuánto \
             trabajo implica según las reglas de bump del contrato.",
            "patch/minor: `--sync`. major: además revalidar probes + preview y \
             volver a correr el gate antes de promocionar.",
        ),
        Cause::FileMissing => (
            "un path que contract.lock registra no tiene bytes bajo \
             .side-contract/: la copia del side está incompleta.",
            "reponer la copia con `--sync` (o restaurar desde git si la copia \
             estaba commiteada).",
        ),
        Cause::FileDrift => (
            "los bytes de un archivo bajo .side-contract/ no coinciden con el \
             digest que contract.lock registra: alguien editó la copia local.",
            "`--sync --force` para restaurar desde el bundle, o revertir la \
             edición a mano; la edición local nunca es válida.",
        ),
        Cause::FileUnexpected => (
            "hay un archivo bajo .side-contract/ que contract.lock no registra: \
             la copia recibió contenido ajeno al contrato.",
            "eliminar el archivo o volver a sincronizar con `--sync --force`.",
        ),
        Cause::BundleRetailViolation => (
            "un path del árbol empaquetado casa un glob de [exclusions] de \
             DATA-CLASSIFICATION: datos retail (BSA/VID/pal/ARENA2) que nunca \
             pueden viajar ni al bundle ni al side.",
            "retirar el dato retail del contract-root (o del bundle) y volver a \
             generar/verificar; la clasificación de datos no es negociable.",
        ),
        Cause::ContractSourceDrift => (
            "la fuente de MAIN ya no coincide con el contrato: un anchor no casa \
             en su línea, un bloque cerrado (SURFACE/ERROR_CODES/methods::ALL/\
             Cause::ALL/id()) es ilegible, o un item derivado no está citado en \
             el capítulo de su [coverage].",
            "en MAIN: actualizar contract.toml/capítulos al cambio real y hacer \
             el bump de versión que corresponda (§9.2); luego regenerar.",
        ),
        Cause::InternalError => (
            "el tool no pudo evaluar: error de I/O, permisos, UTF-8 inválido, o \
             dos generaciones con --verify-determinism no coincidieron byte a \
             byte. No es un veredicto sobre el side.",
            "revisar el detalle (son errores del entorno o del tool), corregir \
             la causa y volver a correr; el exit 4 nunca se interpreta como \
             drift del side.",
        ),
    };
    format!(
        "cause: {}\n\n{}\n\nQué hacer: {}\n",
        cause.id(),
        meaning,
        action
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_cause_explains_meaning_and_action() {
        for cause in Cause::ALL {
            let text = explain(cause);
            assert!(text.starts_with(&format!("cause: {}\n", cause.id())));
            assert!(text.contains("\n\nQué hacer: "));
            assert!(text.ends_with('\n'));
            assert!(text.len() > 100, "too short for {}", cause.id());
        }
    }
}
