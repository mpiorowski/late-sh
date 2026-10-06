use crate::app::door::lateania::damage::*;

#[test]
fn weakness_amplifies_and_resist_reduces() {
    let undead = DamageProfile::new(
        DamageType::Shadow,
        Some(DamageType::Shadow),
        Some(DamageType::Holy),
    );
    let (holy, def_h) = undead.apply(100, DamageType::Holy);
    let (shadow, def_s) = undead.apply(100, DamageType::Shadow);
    let (phys, def_p) = undead.apply(100, DamageType::Physical);
    assert_eq!((holy, def_h), (150, Defense::Weak));
    assert_eq!((shadow, def_s), (50, Defense::Resist));
    assert_eq!((phys, def_p), (100, Defense::Normal));
}

#[test]
fn damage_never_drops_below_one() {
    let p = DamageProfile::new(DamageType::Fire, Some(DamageType::Fire), None);
    let (dmg, _) = p.apply(1, DamageType::Fire);
    assert!(dmg >= 1);
}
