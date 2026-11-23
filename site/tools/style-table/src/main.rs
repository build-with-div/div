//! Probe `div_style::parse_literal` for every property in PROPERTY_METADATA
//! except the internal rows, for the website's styling reference (see
//! generate.py next to Cargo.toml).
//! Usage: div-style-table meta            -> TSV of metadata
//!        div-style-table probe <file>    -> reads `css-name\tcandidate` lines,
//!                                         prints `css\tcandidate\tok\tapplies\tvalue`
use div_style::{
    parse_literal, value_matches_property, Color, Edges, LengthValue, PropertyMetadata, StyleColor,
    StyleProperty, StyleValue, INTERNAL_CSS_NAMES, PROPERTY_METADATA,
};

/// The schema's rows without the internal ones, which are not CSS properties.
fn css_properties() -> impl Iterator<Item = &'static PropertyMetadata> {
    PROPERTY_METADATA
        .iter()
        .filter(|m| !INTERNAL_CSS_NAMES.contains(&m.css_name))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("meta") => {
            for m in css_properties() {
                println!(
                    "{}\t{}\t{:?}\t{}\t{:?}\t{}\t{}\t{:?}\t{:?}",
                    m.property as u16,
                    m.css_name,
                    m.property,
                    m.rust_type,
                    m.value_kind,
                    m.initial,
                    m.inherited,
                    m.invalidation,
                    m.interpolation
                );
            }
        }
        Some("probe") => {
            let text = std::fs::read_to_string(&args[2]).expect("read candidates");
            for line in text.lines() {
                let Some((name, candidate)) = line.split_once('\t') else {
                    continue;
                };
                let Some(property) = StyleProperty::from_css_name(name) else {
                    println!("{name}\t{candidate}\tUNKNOWN-PROPERTY\t-\t-");
                    continue;
                };
                match parse_literal(property, candidate) {
                    Ok(value) => {
                        let applies = value_matches_property(property, &value);
                        let mut debug = format!("{value:?}");
                        debug.truncate(120);
                        println!(
                            "{name}\t{candidate}\tok\t{applies}\t{}",
                            debug.replace('\t', " ")
                        );
                    }
                    Err(_) => println!("{name}\t{candidate}\terr\t-\t-"),
                }
            }
        }
        Some("typed") => {
            // Canned typed values for the Rust types that have `From<T> for StyleValue`
            // and that `view!` names for `style:prop={expr}`. The Engine admits a value
            // iff `value_matches_property` holds (div-engine/src/authored_source.rs).
            let canned: Vec<(&str, StyleValue)> = vec![
                ("f32", 1.0_f32.into()),
                ("i32", 1_i32.into()),
                ("LengthValue", LengthValue::px(1.0).into()),
                (
                    "Edges<LengthValue>",
                    Edges::all(LengthValue::px(1.0)).into(),
                ),
                ("Color", Color::BLACK.into()),
                ("StyleColor", StyleColor::Resolved(Color::BLACK).into()),
                ("StyleColor::CurrentColor", StyleColor::CurrentColor.into()),
            ];
            for m in css_properties() {
                for (name, value) in &canned {
                    println!(
                        "{}	{}	{}",
                        m.css_name,
                        name,
                        value_matches_property(m.property, value)
                    );
                }
            }
        }
        _ => eprintln!("usage: div-style-table meta | probe <file> | typed"),
    }
}
