//! Dump computed values as `{page: {viewport: {key: {property: value}}}}`, the
//! shape `bench/chrome-styles.mjs` writes, to compare engines value by value.
//!
//! usage: cargo run --release --example dump -- <site-dir> <base> <viewports> > out.json

use std::collections::BTreeMap;
use std::path::Path;

use cssdiff::page::{Page, Size};
use cssdiff::property;
use style::properties::PropertyDeclarationId;

type Elements = BTreeMap<String, BTreeMap<&'static str, String>>;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = Path::new(&args[0]);
    let viewports: Vec<Size> = args[2].split(',').map(|v| v.parse().unwrap()).collect();
    let plan = cssdiff::site::plan(root, root)?;
    let properties = property::all();
    let mut out: BTreeMap<String, BTreeMap<String, Elements>> = BTreeMap::new();
    for (rel, _) in &plan.pairs {
        let mut page = Page::load(root, &args[1], rel)?;
        for &size in &viewports {
            page.resolve_at(size);
            let mut elements = BTreeMap::new();
            for entry in page.entries() {
                let Some(styles) = page.computed(entry.node) else {
                    continue;
                };
                let values = properties
                    .iter()
                    .map(|p| {
                        (
                            p.name,
                            styles.computed_value_to_string(PropertyDeclarationId::Longhand(p.id)),
                        )
                    })
                    .collect();
                elements.insert(entry.key, values);
            }
            out.entry(rel.display().to_string())
                .or_default()
                .insert(size.to_string(), elements);
        }
    }
    println!("{}", serde_json::to_string(&out)?);
    Ok(())
}
