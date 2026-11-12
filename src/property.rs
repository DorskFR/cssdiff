use style::properties::{LonghandId, PropertyDeclarationId, PropertyId};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Mode {
    /// Cascade output: computed values, independent of layout and fonts
    Computed,
    /// What `getComputedStyle()` returns: layout-dependent properties
    /// (width, height, margins, insets…) become used pixel values
    Used,
}

#[derive(Debug, Clone, Copy)]
pub struct Property {
    pub name: &'static str,
    pub id: LonghandId,
}

/// Every physical CSS longhand exposed by `getComputedStyle()`. Logical
/// longhands (`inline-size`, `margin-block-start`…) are left out: they always
/// resolve to the physical longhand they map to, so they only duplicate it.
pub fn all() -> Vec<Property> {
    // Blitz enables Stylo prefs (grid, columns, writing-mode…) when it creates
    // a document; properties behind them are unknown until then.
    drop(blitz_dom::BaseDocument::new(
        blitz_dom::DocumentConfig::default(),
    ));
    blitz_dom::resolved_style_property_names()
        .iter()
        .filter_map(|&name| {
            let id = PropertyId::parse_enabled_for_all_content(name)
                .ok()?
                .longhand_id()?;
            (!PropertyDeclarationId::Longhand(id).is_logical()).then_some(Property { name, id })
        })
        .collect()
}
