//! `readiness-report.md` — plan §4.2: presence of every mandatory section
//! (gate step 3) and the OD-8 (b) machine-readable probe block.
//!
//! Step 3 checks *presence*, exactly as §4.3.3 states: the report exists, all
//! eight §4.2 sections are H2 headings, the risk section is not empty, and the
//! machine block is a parseable JSON object with a `probes` array. Content
//! cross-checks beyond that are listed in PROMOTION-GATE-v1 as out of F4-A.

use std::path::Path;

use serde_json::Value;

/// The eight mandatory §4.2 sections, in report order.
pub const REQUIRED_SECTIONS: [&str; 8] = [
    "Identidad",
    "Compatibilidad",
    "Probes",
    "Determinismo",
    "Rendimiento",
    "Fronteras",
    "Dependencias",
    "Riesgos conocidos",
];

/// The risk section must carry a justification, not a blank body (§4.2).
pub const RISK_SECTION: &str = "Riesgos conocidos";

/// Read the report named by `manifest.readiness.report` (path relative to the side root).
pub fn load(side_root: &Path, report: &str) -> Result<String, String> {
    let path = side_root.join(report);
    std::fs::read_to_string(&path).map_err(|e| format!("no se pudo leer {}: {e}", path.display()))
}

/// A fenced region (``` ... ```) so headings inside code blocks are not mistaken
/// for sections.
struct Fence {
    start: usize,
    end: usize,
}

fn fences(text: &str) -> Vec<Fence> {
    let mut out = Vec::new();
    let mut open: Option<usize> = None;
    for (idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            match open {
                None => open = Some(idx),
                Some(start) => {
                    out.push(Fence { start, end: idx });
                    open = None;
                }
            }
        }
    }
    if let Some(start) = open {
        out.push(Fence { start, end: text.lines().count() });
    }
    out
}

fn in_fence(fence_list: &[Fence], line: usize) -> bool {
    fence_list.iter().any(|f| line >= f.start && line <= f.end)
}

/// All eight sections present, with a non-empty `Riesgos conocidos` body.
pub fn check_sections(report: &str) -> Result<(), String> {
    let fence_list = fences(report);
    let mut headings: Vec<(usize, String)> = Vec::new();
    for (idx, line) in report.lines().enumerate() {
        if in_fence(&fence_list, idx) {
            continue;
        }
        if let Some(rest) = line.strip_prefix("## ") {
            headings.push((idx, rest.trim().to_string()));
        }
    }

    let mut missing = Vec::new();
    for section in REQUIRED_SECTIONS {
        if !headings.iter().any(|(_, title)| title == section) {
            missing.push(section);
        }
    }
    if !missing.is_empty() {
        return Err(format!("secciones §4.2 ausentes: {}", missing.join(", ")));
    }

    // Risk section body (until the next H2) must not be empty once fences are ignored.
    let risk_line = headings
        .iter()
        .find(|(_, title)| title == RISK_SECTION)
        .map(|(idx, _)| *idx)
        .expect("risk section checked above");
    let body: String = report
        .lines()
        .enumerate()
        .skip(risk_line + 1)
        .take_while(|(idx, line)| {
            !in_fence(&fence_list, *idx) && !line.starts_with("## ")
        })
        .map(|(_, line)| line)
        .collect::<Vec<_>>()
        .join("\n");
    if body.trim().is_empty() {
        return Err(format!(
            "sección {RISK_SECTION:?} vacía (§4.2 exige riesgos conocidos, 'ninguno' se justifica)"
        ));
    }
    Ok(())
}

/// The **first** fenced ```json block of the report is the machine block
/// (plan §4.2 + OD-8 (b)). It must parse as an object carrying `probes`.
pub fn machine_block(report: &str) -> Result<Value, String> {
    let lines = report.lines();
    let mut block: Vec<&str> = Vec::new();
    let mut open = false;
    for line in lines {
        let trimmed = line.trim();
        if !open {
            if trimmed == "```json" {
                open = true;
            }
            continue;
        }
        if trimmed == "```" {
            break;
        }
        block.push(line);
    }
    if !open {
        return Err("bloque machine-readable ```json ausente (OD-8 (b))".into());
    }
    let text = block.join("\n");
    let value: Value = serde_json::from_str(&text)
        .map_err(|e| format!("bloque machine-readable ilegible: {e}"))?;
    let probes = value
        .get("probes")
        .and_then(|p| p.as_array())
        .ok_or("bloque machine-readable sin arreglo `probes`")?;
    if probes.is_empty() {
        return Err("bloque machine-readable con `probes` vacío".into());
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = "\
# readiness-report

```json
{\"schema\":\"readiness-probes\",\"probes\":[{\"name\":\"smoke\",\"command\":[\"printf\",\"{seed}\"],\"seed\":\"s\",\"expected_sha256\":\"aa\",\"runs\":2}]}
```

## Identidad
id: x

## Compatibilidad
core: ^0.1

## Probes
smoke

## Determinismo
2 corridas

## Rendimiento
n/a

## Fronteras
${side}/cache

## Dependencias
0 patch

## Riesgos conocidos
ninguno, solo imprime.
";

    #[test]
    fn full_report_passes() {
        check_sections(FULL).expect("secciones completas");
        machine_block(FULL).expect("bloque machine parsea");
    }

    #[test]
    fn missing_section_is_named() {
        let broken = FULL.replace("## Rendimiento\nn/a\n", "");
        let err = check_sections(&broken).unwrap_err();
        assert!(err.contains("Rendimiento"), "{err}");
    }

    #[test]
    fn empty_risk_section_is_rejected() {
        let broken = FULL.replace("ninguno, solo imprime.", "   ");
        let err = check_sections(&broken).unwrap_err();
        assert!(err.contains("Riesgos conocidos"), "{err}");
    }

    #[test]
    fn heading_inside_fence_is_not_a_section() {
        let broken = FULL.replace("## Rendimiento\nn/a\n", "");
        // A fenced fake heading must not satisfy the missing section.
        let with_fake = format!("{broken}\n```\n## Rendimiento\n```\n");
        assert!(check_sections(&with_fake).is_err());
        // ...while the machine block still parses (fences are transparent to it).
        assert!(machine_block(&with_fake).is_ok());
    }

    #[test]
    fn absent_block_is_reported() {
        let no_block = FULL.split("```json").next().unwrap();
        let err = machine_block(no_block).unwrap_err();
        assert!(err.contains("machine-readable"), "{err}");
    }

    #[test]
    fn malformed_block_is_reported() {
        let bad = "# x\n\n```json\n{not json}\n```\n\n## Riesgos conocidos\nx\n";
        let err = machine_block(bad).unwrap_err();
        assert!(err.contains("ilegible"), "{err}");
    }
}
