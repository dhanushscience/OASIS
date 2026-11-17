//! The three buttons from the keyboard: which key presses and releases belong to a button.
//!
//! OS-neutral. The OS side reports every key going down or up (as a [`ChordKey`]) and asks what to
//! do with it; this says whether it is button 1, 2 or 3 going down or up, and whether the key
//! should be kept from the program in front. A button goes down when its key is pressed with exactly
//! its modifiers held, and up when that key is let go, or when one of its modifiers is.

use crate::shortcut::{Chord, ChordKey};

/// A shortcut for one button: one plain key and the modifiers that go with it, or one modifier
/// (Shift, Ctrl, Alt, Win) on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
    key: ChordKey,
}

impl Binding {
    /// `None` if the chord does not have exactly one key besides its modifiers (or is not a single
    /// modifier on its own).
    pub fn from_chord(chord: &Chord) -> Option<Binding> {
        let mut plain = chord.keys.iter().copied().filter(|k| !k.is_modifier());
        let Some(key) = plain.next() else {
            return match chord.keys.as_slice() {
                [m] => Some(Binding { ctrl: false, shift: false, alt: false, win: false, key: *m }),
                _ => None,
            };
        };
        if plain.next().is_some() {
            return None;
        }
        let has = |m: ChordKey| chord.keys.contains(&m);
        Some(Binding { ctrl: has(ChordKey::Ctrl), shift: has(ChordKey::Shift), alt: has(ChordKey::Alt), win: has(ChordKey::Win), key })
    }

    pub fn parse(text: &str) -> Option<Binding> {
        Chord::parse(text).ok().and_then(|c| Binding::from_chord(&c))
    }
}

/// What to do with one key event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyVerdict {
    /// Not ours: the program in front gets it as usual.
    Pass,
    /// Ours, but a repeat or a stray release: nothing to report. Kept from the program when
    /// the shortcuts are exclusive.
    Quiet,
    /// Button (0, 1 or 2) went down.
    Press(u8),
    /// Button (0, 1 or 2) came up.
    Release(u8),
}

#[derive(Debug, Default)]
pub struct KeyMatcher {
    bindings: [Option<Binding>; 3],
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
    /// The key that holds each button down.
    held: [Option<ChordKey>; 3],
    /// Keys whose press was ours but whose button already came up (a modifier was let go first):
    /// their release is ours too.
    orphans: Vec<ChordKey>,
}

impl KeyMatcher {
    pub fn new(bindings: [Option<Binding>; 3]) -> KeyMatcher {
        KeyMatcher { bindings, ..KeyMatcher::default() }
    }

    /// New shortcuts. A button that is down comes up first (the caller gets its release).
    pub fn set_bindings(&mut self, bindings: [Option<Binding>; 3]) -> Vec<KeyVerdict> {
        let released = self.release_all();
        self.bindings = bindings;
        released
    }

    /// Every held button comes up (the listener was turned off).
    pub fn release_all(&mut self) -> Vec<KeyVerdict> {
        let mut out = Vec::new();
        for (i, slot) in self.held.iter_mut().enumerate() {
            if slot.take().is_some() {
                out.push(KeyVerdict::Release(i as u8));
            }
        }
        out
    }

    /// The modifiers that are really down, from the system. Called before a key is judged: a release the
    /// listener never heard (it came while another program, or the window itself, had the keys) must not
    /// leave a shortcut dead for good because the matcher still thinks Ctrl, Shift, Alt or Win is held.
    pub fn sync_modifiers(&mut self, ctrl: bool, shift: bool, alt: bool, win: bool) {
        (self.ctrl, self.shift, self.alt, self.win) = (ctrl, shift, alt, win);
    }

    fn mods_match(&self, b: &Binding) -> bool {
        (b.ctrl, b.shift, b.alt, b.win) == (self.ctrl, self.shift, self.alt, self.win)
    }

    /// One key went down (`down`) or up. Modifiers are tracked and never kept from the program,
    /// though letting one go can end a button.
    pub fn key(&mut self, key: ChordKey, down: bool) -> Vec<KeyVerdict> {
        match key {
            ChordKey::Ctrl | ChordKey::Shift | ChordKey::Alt | ChordKey::Win => {
                let flag = match key {
                    ChordKey::Ctrl => &mut self.ctrl,
                    ChordKey::Shift => &mut self.shift,
                    ChordKey::Alt => &mut self.alt,
                    _ => &mut self.win,
                };
                *flag = down;
                if down {
                    // A modifier that is itself a button goes down when nothing else is held with it.
                    // It still reaches the program in front (typing would break otherwise): the
                    // Pass after the Press keeps it from being kept back.
                    let alone = [self.ctrl, self.shift, self.alt, self.win].iter().filter(|m| **m).count() == 1;
                    for i in 0..3 {
                        if self.bindings[i].is_some_and(|b| b.key == key) && alone && self.held[i].is_none() {
                            self.held[i] = Some(key);
                            return vec![KeyVerdict::Press(i as u8), KeyVerdict::Pass];
                        }
                    }
                    return vec![KeyVerdict::Pass];
                }
                // A modifier let go: its own button comes up, and so does a button that needed it.
                let mut out = Vec::new();
                for i in 0..3 {
                    if let (Some(held), Some(b)) = (self.held[i], self.bindings[i]) {
                        if b.key.is_modifier() {
                            if b.key == key {
                                self.held[i] = None;
                                out.push(KeyVerdict::Release(i as u8));
                            }
                        } else if !self.mods_match(&b) {
                            self.held[i] = None;
                            self.orphans.push(held);
                            out.push(KeyVerdict::Release(i as u8));
                        }
                    }
                }
                out.push(KeyVerdict::Pass);
                out
            }
            _ if down => {
                if self.held.contains(&Some(key)) || self.orphans.contains(&key) {
                    return vec![KeyVerdict::Quiet]; // auto-repeat while held
                }
                for i in 0..3 {
                    if let Some(b) = self.bindings[i] {
                        if b.key == key && self.mods_match(&b) && self.held[i].is_none() {
                            self.held[i] = Some(key);
                            return vec![KeyVerdict::Press(i as u8)];
                        }
                    }
                }
                vec![KeyVerdict::Pass]
            }
            _ => {
                if let Some(i) = self.held.iter().position(|h| *h == Some(key)) {
                    self.held[i] = None;
                    return vec![KeyVerdict::Release(i as u8)];
                }
                if let Some(at) = self.orphans.iter().position(|k| *k == key) {
                    self.orphans.remove(at);
                    return vec![KeyVerdict::Quiet];
                }
                vec![KeyVerdict::Pass]
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use KeyVerdict::*;

    fn matcher() -> KeyMatcher {
        KeyMatcher::new([Binding::parse("Ctrl+Alt+1"), Binding::parse("F13"), Binding::parse("Ctrl+Shift+K")])
    }
    fn tap(m: &mut KeyMatcher, keys: &[ChordKey], plain: ChordKey) -> (Vec<KeyVerdict>, Vec<KeyVerdict>) {
        for k in keys {
            m.key(*k, true);
        }
        let down = m.key(plain, true);
        let up = m.key(plain, false);
        for k in keys.iter().rev() {
            m.key(*k, false);
        }
        (down, up)
    }

    #[test]
    fn a_modifier_the_listener_never_saw_let_go_does_not_kill_a_bare_key() {
        let mut m = KeyMatcher::new([None, Binding::parse("Tab"), None]);
        m.key(ChordKey::Alt, true); // ... and its release is never heard
        assert_eq!(m.key(ChordKey::Tab, true), vec![Pass], "Alt looks held, so Tab alone is not the shortcut");
        m.key(ChordKey::Tab, false);
        m.sync_modifiers(false, false, false, false); // the system says nothing is down
        assert_eq!(m.key(ChordKey::Tab, true), vec![Press(1)]);
    }

    #[test]
    fn a_binding_needs_exactly_one_plain_key() {
        assert!(Binding::parse("Ctrl+Alt+1").is_some() && Binding::parse("F13").is_some());
        assert!(Binding::parse("Ctrl+Alt").is_none());
        assert!(Binding::parse("A+B").is_none());
        assert!(Binding::parse("").is_none());
    }

    #[test]
    fn the_shortcut_presses_and_releases_its_button() {
        let mut m = matcher();
        assert_eq!(tap(&mut m, &[ChordKey::Ctrl, ChordKey::Alt], ChordKey::Char('1')), (vec![Press(0)], vec![Release(0)]));
        assert_eq!(tap(&mut m, &[], ChordKey::Function(13)), (vec![Press(1)], vec![Release(1)]));
        assert_eq!(tap(&mut m, &[ChordKey::Ctrl, ChordKey::Shift], ChordKey::Char('K')), (vec![Press(2)], vec![Release(2)]));
    }

    #[test]
    fn other_keys_and_the_wrong_modifiers_pass_through() {
        let mut m = matcher();
        assert_eq!(tap(&mut m, &[], ChordKey::Char('1')).0, vec![Pass]);
        assert_eq!(tap(&mut m, &[ChordKey::Ctrl], ChordKey::Char('1')).0, vec![Pass], "Alt is missing");
        assert_eq!(tap(&mut m, &[ChordKey::Ctrl, ChordKey::Alt, ChordKey::Shift], ChordKey::Char('1')).0, vec![Pass], "an extra modifier");
        assert_eq!(m.key(ChordKey::Char('Z'), false), vec![Pass]);
    }

    #[test]
    fn holding_the_key_is_one_press_not_many() {
        let mut m = matcher();
        m.key(ChordKey::Function(13), true);
        assert_eq!(m.key(ChordKey::Function(13), true), vec![Quiet]);
        assert_eq!(m.key(ChordKey::Function(13), true), vec![Quiet]);
        assert_eq!(m.key(ChordKey::Function(13), false), vec![Release(1)]);
    }

    #[test]
    fn letting_go_of_a_modifier_first_ends_the_button_and_swallows_the_late_release() {
        let mut m = matcher();
        m.key(ChordKey::Ctrl, true);
        m.key(ChordKey::Alt, true);
        assert_eq!(m.key(ChordKey::Char('1'), true), vec![Press(0)]);
        assert_eq!(m.key(ChordKey::Alt, false), vec![Release(0), Pass]);
        assert_eq!(m.key(ChordKey::Char('1'), true), vec![Quiet], "still down, not a new press");
        assert_eq!(m.key(ChordKey::Char('1'), false), vec![Quiet], "its release is ours too");
        assert_eq!(m.key(ChordKey::Char('1'), false), vec![Pass], "and only once");
    }

    #[test]
    fn caps_lock_and_tab_can_be_buttons() {
        let mut m = KeyMatcher::new([Binding::parse("CapsLock"), Binding::parse("Tab"), None]);
        assert_eq!(tap(&mut m, &[], ChordKey::CapsLock), (vec![Press(0)], vec![Release(0)]));
        assert_eq!(tap(&mut m, &[], ChordKey::Tab), (vec![Press(1)], vec![Release(1)]));
    }

    #[test]
    fn a_modifier_on_its_own_can_be_a_button_and_still_reaches_the_program() {
        assert!(Binding::parse("Shift").is_some() && Binding::parse("Ctrl+Shift").is_none());
        let mut m = KeyMatcher::new([Binding::parse("Shift"), Binding::parse("Ctrl+K"), None]);
        assert_eq!(m.key(ChordKey::Shift, true), vec![Press(0), Pass], "pressed, and Shift is still passed on");
        assert_eq!(m.key(ChordKey::Shift, true), vec![Pass], "holding it is one press");
        assert_eq!(m.key(ChordKey::Shift, false), vec![Release(0), Pass]);
        // with another modifier already held it is part of a chord, not the button
        m.key(ChordKey::Ctrl, true);
        assert_eq!(m.key(ChordKey::Shift, true), vec![Pass]);
        assert_eq!(m.key(ChordKey::Shift, false), vec![Pass]);
        m.key(ChordKey::Ctrl, false);
        // other bindings are unaffected
        assert_eq!(tap(&mut m, &[ChordKey::Ctrl], ChordKey::Char('K')), (vec![Press(1)], vec![Release(1)]));
    }

    #[test]
    fn new_shortcuts_release_what_is_down_and_apply() {
        let mut m = matcher();
        m.key(ChordKey::Function(13), true);
        assert_eq!(m.set_bindings([Binding::parse("F14"), None, None]), vec![Release(1)]);
        assert_eq!(tap(&mut m, &[], ChordKey::Function(13)).0, vec![Pass]);
        assert_eq!(tap(&mut m, &[], ChordKey::Function(14)).0, vec![Press(0)]);
    }
}
