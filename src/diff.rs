use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use anyhow::Result;
use serde::Serialize;

use crate::page::{Page, Size, State};
use crate::property::{Mode, Property};
use blitz_dom::NodeId;
use style::properties::{ComputedValues, PropertyDeclarationId};
use style::servo_arc::Arc as ServoArc;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    pub element: String,
    pub path: String,
    /// Document order of the element, for sorting the report.
    #[serde(skip)]
    pub order: usize,
    pub property: String,
    pub before: String,
    pub after: String,
    pub viewport: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Before,
    After,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Unmatched {
    pub element: String,
    pub path: String,
    pub only_in: Side,
    pub viewport: String,
}

#[derive(Debug, Default, Serialize)]
pub struct PageDiff {
    pub changes: Vec<Change>,
    pub unmatched: Vec<Unmatched>,
    pub warnings: Vec<String>,
    pub elements: usize,
}

impl PageDiff {
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty() && self.unmatched.is_empty()
    }
}

pub struct Sources<'a> {
    pub before_root: &'a Path,
    pub after_root: &'a Path,
    /// URL path prefix the site is served under, e.g. `/docs`.
    pub base: &'a str,
}

pub fn diff_page(
    sources: &Sources,
    before_rel: &Path,
    after_rel: &Path,
    viewports: &[Size],
    properties: &[Property],
    mode: Mode,
    states: &[State],
) -> Result<PageDiff> {
    let mut before = Page::load(sources.before_root, sources.base, before_rel)?;
    let mut after = Page::load(sources.after_root, sources.base, after_rel)?;
    let mut out = PageDiff::default();

    let passes: Vec<(Size, Option<State>)> = viewports
        .iter()
        .flat_map(|&s| {
            std::iter::once((s, None)).chain(states.iter().map(move |&st| (s, Some(st))))
        })
        .collect();
    // Changes already seen without a forced state are not repeated under one.
    let mut base: std::collections::HashSet<(String, String, String, String)> = Default::default();
    for (size, state) in passes {
        let forced = state.is_some();
        if !forced {
            base.clear();
        }
        before.resolve_at(size);
        after.resolve_at(size);
        if let Some(state) = state {
            before.force_state(state, true);
            after.force_state(state, true);
        }
        let mut before_cache = ValueCache::default();
        let mut after_cache = ValueCache::default();
        let viewport = match state {
            Some(state) => format!("{size}{}", state.name()),
            None => size.to_string(),
        };
        let before_entries = before.entries();
        let after_entries = after.entries();
        out.elements = out.elements.max(before_entries.len());

        let mut after_by_key: HashMap<&str, &crate::page::Entry> =
            after_entries.iter().map(|e| (e.key.as_str(), e)).collect();

        for (order, b) in before_entries.iter().enumerate() {
            let Some(a) = after_by_key.remove(b.key.as_str()) else {
                if forced {
                    continue;
                }
                out.unmatched.push(Unmatched {
                    element: b.label.clone(),
                    path: b.key.clone(),
                    only_in: Side::Before,
                    viewport: viewport.clone(),
                });
                continue;
            };
            let mut push = |property: Property, before: String, after: String| {
                let id = (
                    a.key.clone(),
                    property.name.to_string(),
                    before.clone(),
                    after.clone(),
                );
                if forced {
                    if base.contains(&id) {
                        return;
                    }
                } else {
                    base.insert(id);
                }
                out.changes.push(Change {
                    element: a.label.clone(),
                    path: a.key.clone(),
                    order,
                    property: property.name.to_string(),
                    before,
                    after,
                    viewport: viewport.clone(),
                })
            };
            match mode {
                Mode::Computed => {
                    let bv = before_cache.values(&before, b.node, properties);
                    let av = after_cache.values(&after, a.node, properties);
                    if Rc::ptr_eq(&bv, &av) || bv == av {
                        continue;
                    }
                    for (i, &property) in properties.iter().enumerate() {
                        if bv[i] != av[i] {
                            push(property, bv[i].clone(), av[i].clone());
                        }
                    }
                }
                Mode::Used => {
                    for &property in properties {
                        let bv = before.used(b.node, property);
                        let av = after.used(a.node, property);
                        if bv != av {
                            push(property, bv, av);
                        }
                    }
                }
            }
        }
        for a in &after_entries {
            if !forced && after_by_key.contains_key(a.key.as_str()) {
                out.unmatched.push(Unmatched {
                    element: a.label.clone(),
                    path: a.key.clone(),
                    only_in: Side::After,
                    viewport: viewport.clone(),
                });
            }
        }
        if let Some(state) = state {
            before.force_state(state, false);
            after.force_state(state, false);
        }
    }

    out.warnings
        .extend(before.warnings.drain(..).map(|w| format!("before: {w}")));
    out.warnings
        .extend(after.warnings.drain(..).map(|w| format!("after: {w}")));
    out.warnings.sort();
    out.warnings.dedup();
    Ok(out)
}

/// Serialized computed values, memoized per `ComputedValues`: Stylo shares one
/// `ComputedValues` between elements that style identically, so most elements
/// are a cache hit. Each entry keeps its `Arc` alive, so a pointer cannot be
/// freed and reused for another style while the cache exists.
#[derive(Default)]
struct ValueCache {
    by_style: HashMap<*const ComputedValues, (ServoArc<ComputedValues>, Rc<Vec<String>>)>,
    unstyled: Option<Rc<Vec<String>>>,
}

impl ValueCache {
    fn values(&mut self, page: &Page, node: NodeId, properties: &[Property]) -> Rc<Vec<String>> {
        let Some(styles) = page.computed(node) else {
            return self
                .unstyled
                .get_or_insert_with(|| Rc::new(vec![String::new(); properties.len()]))
                .clone();
        };
        let key: *const ComputedValues = &*styles;
        if let Some((_, values)) = self.by_style.get(&key) {
            return values.clone();
        }
        let values: Rc<Vec<String>> = Rc::new(
            properties
                .iter()
                .map(|p| styles.computed_value_to_string(PropertyDeclarationId::Longhand(p.id)))
                .collect(),
        );
        self.by_style.insert(key, (styles, values.clone()));
        values
    }
}
