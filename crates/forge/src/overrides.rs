//! Designer overrides (proof of concept): keep hand edits across
//! regeneration by writing them as a small TOML patch instead of editing the
//! generated file. `dungeon-forge generate --overrides FILE` applies the
//! patch after generation and loads the result through the game's loader.
//!
//! Patch rules:
//! * Tables merge key by key; any other value replaces the generated one.
//! * Arrays of tables (`[[encounters]]`, `[[dungeon.areas]]`, `[[mobs]]` …)
//!   match elements by `id`, else by `name`; the patch element is merged into
//!   the generated element with that key. `remove = true` deletes it;
//!   `new = true` appends the element instead of matching.
//! * A keyed element that no longer exists is an error naming it: a layout
//!   change has made the override stale, and silently dropping a designer's
//!   edit would be worse than stopping.
//!
//! Elements without an `id` or `name` (mobs, hazards) are addressed by
//! position: `index = N` (0-based) in the patch element.

use toml::Value;
use toml::map::Map;

/// Apply `patch` to the generated dungeon text; keep its header comments.
pub fn apply(generated: &str, patch: &str, patch_name: &str) -> Result<String, String> {
    let mut base: Value = toml::from_str(generated).map_err(|e| format!("generated file: {e}"))?;
    let patch: Value = toml::from_str(patch).map_err(|e| format!("{patch_name}: {e}"))?;
    merge(&mut base, &patch, "")?;
    prune_branch_rewards(&mut base);
    let header: String = generated
        .lines()
        .take_while(|l| l.starts_with('#') || l.trim().is_empty())
        .filter(|l| l.starts_with('#'))
        .map(|l| format!("{l}\n"))
        .collect();
    let body = toml::to_string(&base).map_err(|e| e.to_string())?;
    Ok(format!(
        "{header}# Designer overrides applied from {patch_name}.\n\n{body}"
    ))
}

/// Branch records (0.9) list the encounters and caches a branch holds; they
/// are derived presentation data, so a removed encounter just drops out.
fn prune_branch_rewards(base: &mut Value) {
    let ids = |v: &Value, key: &str| -> Vec<String> {
        v.get(key)
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    let mut known = ids(base, "encounters");
    if let Some(d) = base.get("dungeon") {
        known.extend(ids(d, "caches"));
    }
    let Some(branches) = base
        .get_mut("dungeon")
        .and_then(|d| d.get_mut("branches"))
        .and_then(|b| b.as_array_mut())
    else {
        return;
    };
    for b in branches {
        if let Some(Value::Array(r)) = b.get_mut("rewards") {
            r.retain(|v| v.as_str().is_some_and(|s| known.iter().any(|k| k == s)));
        }
    }
}

fn merge(base: &mut Value, patch: &Value, path: &str) -> Result<(), String> {
    match (base, patch) {
        (Value::Table(b), Value::Table(p)) => {
            for (k, pv) in p {
                let here = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match b.get_mut(k) {
                    Some(bv) => merge(bv, pv, &here)?,
                    None => {
                        b.insert(k.clone(), pv.clone());
                    }
                }
            }
            Ok(())
        }
        (Value::Array(b), Value::Array(p)) if is_table_array(b) && is_table_array(p) => {
            merge_tables(b, p, path)
        }
        (b, p) => {
            *b = p.clone();
            Ok(())
        }
    }
}

fn is_table_array(a: &[Value]) -> bool {
    !a.is_empty() && a.iter().all(|v| v.is_table())
}

/// The key a patch element addresses: ("id", v), ("name", v) or ("index", n).
fn address(p: &Map<String, Value>) -> Option<(&'static str, String)> {
    for k in ["id", "name"] {
        if let Some(Value::String(s)) = p.get(k) {
            return Some((if k == "id" { "id" } else { "name" }, s.clone()));
        }
    }
    p.get("index")
        .and_then(|v| v.as_integer())
        .map(|n| ("index", n.to_string()))
}

fn merge_tables(base: &mut Vec<Value>, patch: &[Value], path: &str) -> Result<(), String> {
    let mut removed = Vec::new();
    for pv in patch {
        let p = pv.as_table().expect("checked");
        let flag = |k: &str| p.get(k).and_then(|v| v.as_bool()).unwrap_or(false);
        let mut clean = p.clone();
        for k in ["new", "remove", "index"] {
            clean.remove(k);
        }
        if flag("new") {
            base.push(Value::Table(clean));
            continue;
        }
        let (key, want) = address(p).ok_or_else(|| {
            format!("{path}: each override element needs `id`, `name`, `index` or `new = true`")
        })?;
        let at = match key {
            "index" => want.parse::<usize>().ok().filter(|&i| i < base.len()),
            _ => base.iter().position(|v| {
                v.get(key).and_then(|s| s.as_str()) == Some(want.as_str())
                    // An element with an id is matched by id, not by name.
                    && (key == "id" || v.get("id").is_none() || p.get("id").is_none())
            }),
        }
        .ok_or_else(|| {
            format!(
                "{path}: no element with {key} '{want}' in the generated dungeon; \
                 the layout has changed since this override was written (update or remove it)"
            )
        })?;
        if flag("remove") {
            removed.push(at);
            continue;
        }
        merge(
            &mut base[at],
            &Value::Table(clean),
            &format!("{path}[{key}={want}]"),
        )?;
    }
    removed.sort_unstable();
    removed.dedup();
    for i in removed.into_iter().rev() {
        base.remove(i);
    }
    Ok(())
}
