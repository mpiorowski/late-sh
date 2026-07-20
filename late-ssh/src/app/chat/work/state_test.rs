use super::{ComposerField, normalize_status};

#[test]
fn field_cycles_forward_and_back() {
    assert_eq!(ComposerField::Headline.next(), ComposerField::Status);
    assert_eq!(ComposerField::Summary.next(), ComposerField::Headline);
    assert_eq!(ComposerField::Headline.prev(), ComposerField::Summary);
}

#[test]
fn status_normalization_accepts_aliases() {
    assert_eq!(normalize_status("available"), Some("open"));
    assert_eq!(normalize_status("maybe"), Some("casual"));
    assert_eq!(normalize_status("not looking"), Some("not-looking"));
    assert_eq!(normalize_status("busy"), None);
}
