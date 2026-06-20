//! Edición programática del `config.toml` para el panel de control.
//!
//! Usa `toml_edit` para modificar el archivo **preservando comentarios y
//! formato**. Cada cambio se valida (que siga siendo una config válida) ANTES de
//! escribir, y el resto de Oxide lo aplica solo gracias a la recarga en caliente.

use anyhow::{bail, Context};
use toml_edit::{value, ArrayOfTables, DocumentMut, Item, Table};

use crate::config::Config;

fn load_doc(path: &str) -> anyhow::Result<DocumentMut> {
    let raw = std::fs::read_to_string(path).with_context(|| format!("no pude leer {path}"))?;
    raw.parse::<DocumentMut>().context("config.toml inválido")
}

fn save_doc(path: &str, doc: &DocumentMut) -> anyhow::Result<()> {
    let text = doc.to_string();
    // Red de seguridad: que el resultado siga siendo una config válida.
    toml::from_str::<Config>(&text).context("el cambio dejaría la config inválida")?;
    std::fs::write(path, text).with_context(|| format!("no pude escribir {path}"))?;
    Ok(())
}

/// Agrega un backend al pool por defecto (`[[upstreams]]`).
pub fn add_backend(path: &str, url: &str, weight: u32) -> anyhow::Result<()> {
    // Validación rápida de la URL.
    let _: hyper::Uri = url.parse().with_context(|| format!("URL inválida: {url}"))?;

    let mut doc = load_doc(path)?;
    if doc.get("upstreams").and_then(|i| i.as_array_of_tables()).is_none() {
        doc["upstreams"] = Item::ArrayOfTables(ArrayOfTables::new());
    }
    let aot = doc["upstreams"].as_array_of_tables_mut().unwrap();

    if aot
        .iter()
        .any(|t| t.get("url").and_then(|v| v.as_str()) == Some(url))
    {
        bail!("el backend {url} ya existe");
    }

    let mut tbl = Table::new();
    tbl["url"] = value(url);
    tbl["weight"] = value(weight.max(1) as i64);
    aot.push(tbl);

    save_doc(path, &doc)
}

/// Quita un backend del pool por defecto.
pub fn remove_backend(path: &str, url: &str) -> anyhow::Result<()> {
    let mut doc = load_doc(path)?;
    let aot = doc
        .get_mut("upstreams")
        .and_then(|i| i.as_array_of_tables_mut())
        .context("no hay upstreams en la config")?;

    let idx = aot
        .iter()
        .position(|t| t.get("url").and_then(|v| v.as_str()) == Some(url));
    match idx {
        Some(i) => {
            aot.remove(i);
        }
        None => bail!("no encontré el backend {url}"),
    }

    if aot.is_empty() {
        bail!("no podés quedarte sin backends");
    }

    save_doc(path, &doc)
}

/// Cambia el algoritmo de balanceo.
pub fn set_algorithm(path: &str, algorithm: &str) -> anyhow::Result<()> {
    if !matches!(algorithm, "round_robin" | "least_connections" | "weighted") {
        bail!("algoritmo inválido: {algorithm}");
    }
    let mut doc = load_doc(path)?;
    if doc.get("balancer").and_then(|i| i.as_table()).is_none() {
        doc["balancer"] = Item::Table(Table::new());
    }
    doc["balancer"]["algorithm"] = value(algorithm);
    save_doc(path, &doc)
}
