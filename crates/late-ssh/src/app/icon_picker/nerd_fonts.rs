use serde_json::Value;

pub struct NerdFontGlyph {
    pub name: String,
    pub icon: String,
}

/// Parse the vendored glyphnames.json into a sorted list of glyphs.
pub fn load() -> Vec<NerdFontGlyph> {
    let raw = include_str!("glyphnames.json");
    let map: Value = serde_json::from_str(raw).expect("invalid glyphnames.json");
    let obj = map.as_object().expect("glyphnames.json is not an object");

    let mut glyphs: Vec<NerdFontGlyph> = obj
        .iter()
        .filter(|(key, _)| *key != "METADATA")
        .filter_map(|(key, val)| {
            let code_str = val.get("code")?.as_str()?;
            let code = u32::from_str_radix(code_str, 16).ok()?;
            let ch = char::from_u32(code)?;
            Some(NerdFontGlyph {
                name: key.replace(['_', '-'], " "),
                icon: ch.to_string(),
            })
        })
        .collect();

    glyphs.sort_by(|a, b| a.name.cmp(&b.name));
    glyphs
}
