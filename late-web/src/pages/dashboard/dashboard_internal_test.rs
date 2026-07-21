use super::generate_status_values;

#[test]
fn status_values_are_deterministic() {
    let first = generate_status_values(123456);
    let second = generate_status_values(123456);
    assert_eq!(first, second);
}

#[test]
fn status_values_stay_in_expected_ranges() {
    for now in [0_u64, 1, 39, 40, 41, 999_999] {
        let (cpu, mem) = generate_status_values(now);
        assert!((20..=60).contains(&cpu));
        assert!((40..=70).contains(&mem));
    }
}

#[test]
fn status_values_differ_across_inputs() {
    let a = generate_status_values(100);
    let b = generate_status_values(105);
    // Different inputs should produce different CPU values (mod 40 cycle)
    assert_ne!(a, b);
}

#[test]
fn status_values_at_modulo_boundaries() {
    // CPU = now % 40 + 20, so at now=40 we wrap
    let (cpu_at_0, _) = generate_status_values(0);
    let (cpu_at_40, _) = generate_status_values(40);
    assert_eq!(cpu_at_0, cpu_at_40);
}
