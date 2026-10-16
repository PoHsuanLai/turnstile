//! The keys palmrest's keyboard may register: the name table of palmrest's `keys.rs` plus its four
//! left-hand modifiers, with how each may be pressed. A test in this crate fails when palmrest's
//! table has a code this one lacks.

/// How a key may be pressed on palmrest's keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyClass {
    /// Control, shift, alt or super: always allowed, and control, alt or super make a chord.
    Modifier,
    /// A letter, digit, punctuation, Enter, Space, Tab, Backspace, Delete or Escape: only while a
    /// chord modifier is held, so the keyboard cannot type text.
    Chorded,
    /// Navigation, function keys, Insert, Back and Forward: no restriction.
    Free,
}

/// One key of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyEntry {
    /// The name palmrest's settings use.
    pub name: &'static str,
    /// The evdev code.
    pub code: u16,
    /// How it may be pressed.
    pub class: KeyClass,
}

const fn key(name: &'static str, code: u16, class: KeyClass) -> KeyEntry {
    KeyEntry { name, code, class }
}

const M: KeyClass = KeyClass::Modifier;
const C: KeyClass = KeyClass::Chorded;
const F: KeyClass = KeyClass::Free;

/// Every key palmrest's keyboard may register.
#[rustfmt::skip]
pub const KEY_TABLE: &[KeyEntry] = &[
    key("ctrl", 29, M), key("shift", 42, M), key("alt", 56, M), key("super", 125, M),
    key("escape", 1, C), key("1", 2, C), key("2", 3, C), key("3", 4, C), key("4", 5, C),
    key("5", 6, C), key("6", 7, C), key("7", 8, C), key("8", 9, C), key("9", 10, C),
    key("0", 11, C), key("minus", 12, C), key("equal", 13, C), key("backspace", 14, C),
    key("tab", 15, C), key("q", 16, C), key("w", 17, C), key("e", 18, C), key("r", 19, C),
    key("t", 20, C), key("y", 21, C), key("u", 22, C), key("i", 23, C), key("o", 24, C),
    key("p", 25, C), key("bracketleft", 26, C), key("bracketright", 27, C), key("enter", 28, C),
    key("a", 30, C), key("s", 31, C), key("d", 32, C), key("f", 33, C), key("g", 34, C),
    key("h", 35, C), key("j", 36, C), key("k", 37, C), key("l", 38, C), key("z", 44, C),
    key("x", 45, C), key("c", 46, C), key("v", 47, C), key("b", 48, C), key("n", 49, C),
    key("m", 50, C), key("comma", 51, C), key("period", 52, C), key("slash", 53, C),
    key("space", 57, C), key("f1", 59, F), key("f2", 60, F), key("f3", 61, F), key("f4", 62, F),
    key("f5", 63, F), key("f6", 64, F), key("f7", 65, F), key("f8", 66, F), key("f9", 67, F),
    key("f10", 68, F), key("f11", 87, F), key("f12", 88, F), key("home", 102, F),
    key("up", 103, F), key("pageup", 104, F), key("left", 105, F), key("right", 106, F),
    key("end", 107, F), key("down", 108, F), key("pagedown", 109, F), key("insert", 110, F),
    key("delete", 111, C), key("back", 158, F), key("forward", 159, F),
];

/// The table row for `code`.
pub fn entry(code: u16) -> Option<&'static KeyEntry> {
    KEY_TABLE.iter().find(|e| e.code == code)
}

/// How `code` may be pressed, if it is in the table.
pub fn class_of(code: u16) -> Option<KeyClass> {
    entry(code).map(|e| e.class)
}
