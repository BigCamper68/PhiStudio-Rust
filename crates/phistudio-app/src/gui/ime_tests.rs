use super::ImeGate;
#[test]
fn startup_focus_buttons_resume_and_explicit_text_taps() {
    let mut gate = ImeGate::default();
    assert_eq!(gate.update(false, false, false), None);
    assert_eq!(gate.update(true, false, false), None);
    assert_eq!(gate.update(false, true, false), None);
    assert_eq!(gate.update(true, true, false), Some(true));
    assert_eq!(gate.update(true, false, false), None);
    assert_eq!(gate.update(true, false, true), Some(false));
    assert_eq!(gate.update(true, false, false), None);
    assert_eq!(gate.update(true, true, false), Some(true));
    assert_eq!(gate.update(true, true, false), Some(true)); // Reopen after Android Back.
    assert_eq!(gate.update(false, false, false), Some(false));
}
