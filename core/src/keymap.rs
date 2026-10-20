//! Platform-neutral key codes.
//!
//! On the wire every key is a USB HID usage from the Keyboard/Keypad page
//! (0x07), the same numbering keyboards themselves report. Each platform shell
//! maps its native codes to and from this space, so a Mac can drive a Windows or
//! Linux machine with the right physical key. Keys without a HID usage get an
//! Edgeway extension code at 0x0300 and above.

/// Neutral modifier bits carried with injected events.
pub mod mods {
    pub const SHIFT: u32 = 1 << 0;
    pub const CONTROL: u32 = 1 << 1;
    pub const ALT: u32 = 1 << 2;
    /// Command on macOS, Windows key elsewhere.
    pub const META: u32 = 1 << 3;
    pub const CAPS_LOCK: u32 = 1 << 4;
    pub const FN: u32 = 1 << 5;
}

/// HID usages for the keys Edgeway treats specially.
pub mod hid {
    pub const H: u16 = 0x0B;
    pub const P: u16 = 0x13;
    pub const CAPS_LOCK: u16 = 0x39;
    pub const LEFT_CONTROL: u16 = 0xE0;
    pub const LEFT_SHIFT: u16 = 0xE1;
    pub const LEFT_ALT: u16 = 0xE2;
    pub const LEFT_META: u16 = 0xE3;
    pub const RIGHT_CONTROL: u16 = 0xE4;
    pub const RIGHT_SHIFT: u16 = 0xE5;
    pub const RIGHT_ALT: u16 = 0xE6;
    pub const RIGHT_META: u16 = 0xE7;
    /// Apple's Fn/Globe key has no standard keyboard-page usage.
    pub const FN: u16 = 0x0300;
}

/// The modifier bit a key contributes while held, or 0.
pub fn modifier_bit(key: u16) -> u32 {
    match key {
        hid::LEFT_SHIFT | hid::RIGHT_SHIFT => mods::SHIFT,
        hid::LEFT_CONTROL | hid::RIGHT_CONTROL => mods::CONTROL,
        hid::LEFT_ALT | hid::RIGHT_ALT => mods::ALT,
        hid::LEFT_META | hid::RIGHT_META => mods::META,
        hid::FN => mods::FN,
        _ => 0,
    }
}

pub fn is_modifier(key: u16) -> bool {
    modifier_bit(key) != 0
}

/// macOS virtual key code (`kVK_*`) ↔ HID usage.
const MAC: &[(u16, u16)] = &[
    (0x00, 0x04),    // A
    (0x0B, 0x05),    // B
    (0x08, 0x06),    // C
    (0x02, 0x07),    // D
    (0x0E, 0x08),    // E
    (0x03, 0x09),    // F
    (0x05, 0x0A),    // G
    (0x04, 0x0B),    // H
    (0x22, 0x0C),    // I
    (0x26, 0x0D),    // J
    (0x28, 0x0E),    // K
    (0x25, 0x0F),    // L
    (0x2E, 0x10),    // M
    (0x2D, 0x11),    // N
    (0x1F, 0x12),    // O
    (0x23, 0x13),    // P
    (0x0C, 0x14),    // Q
    (0x0F, 0x15),    // R
    (0x01, 0x16),    // S
    (0x11, 0x17),    // T
    (0x20, 0x18),    // U
    (0x09, 0x19),    // V
    (0x0D, 0x1A),    // W
    (0x07, 0x1B),    // X
    (0x10, 0x1C),    // Y
    (0x06, 0x1D),    // Z
    (0x12, 0x1E),    // 1
    (0x13, 0x1F),    // 2
    (0x14, 0x20),    // 3
    (0x15, 0x21),    // 4
    (0x17, 0x22),    // 5
    (0x16, 0x23),    // 6
    (0x1A, 0x24),    // 7
    (0x1C, 0x25),    // 8
    (0x19, 0x26),    // 9
    (0x1D, 0x27),    // 0
    (0x24, 0x28),    // Return
    (0x35, 0x29),    // Escape
    (0x33, 0x2A),    // Delete (Backspace)
    (0x30, 0x2B),    // Tab
    (0x31, 0x2C),    // Space
    (0x1B, 0x2D),    // -
    (0x18, 0x2E),    // =
    (0x21, 0x2F),    // [
    (0x1E, 0x30),    // ]
    (0x2A, 0x31),    // \
    (0x29, 0x33),    // ;
    (0x27, 0x34),    // '
    (0x32, 0x35),    // `
    (0x2B, 0x36),    // ,
    (0x2F, 0x37),    // .
    (0x2C, 0x38),    // /
    (0x39, 0x39),    // Caps Lock
    (0x7A, 0x3A),    // F1
    (0x78, 0x3B),    // F2
    (0x63, 0x3C),    // F3
    (0x76, 0x3D),    // F4
    (0x60, 0x3E),    // F5
    (0x61, 0x3F),    // F6
    (0x62, 0x40),    // F7
    (0x64, 0x41),    // F8
    (0x65, 0x42),    // F9
    (0x6D, 0x43),    // F10
    (0x67, 0x44),    // F11
    (0x6F, 0x45),    // F12
    (0x72, 0x49),    // Help / Insert
    (0x73, 0x4A),    // Home
    (0x74, 0x4B),    // Page Up
    (0x75, 0x4C),    // Forward Delete
    (0x77, 0x4D),    // End
    (0x79, 0x4E),    // Page Down
    (0x7C, 0x4F),    // Right Arrow
    (0x7B, 0x50),    // Left Arrow
    (0x7D, 0x51),    // Down Arrow
    (0x7E, 0x52),    // Up Arrow
    (0x47, 0x53),    // Keypad Clear / Num Lock
    (0x4B, 0x54),    // Keypad /
    (0x43, 0x55),    // Keypad *
    (0x4E, 0x56),    // Keypad -
    (0x45, 0x57),    // Keypad +
    (0x4C, 0x58),    // Keypad Enter
    (0x53, 0x59),    // Keypad 1
    (0x54, 0x5A),    // Keypad 2
    (0x55, 0x5B),    // Keypad 3
    (0x56, 0x5C),    // Keypad 4
    (0x57, 0x5D),    // Keypad 5
    (0x58, 0x5E),    // Keypad 6
    (0x59, 0x5F),    // Keypad 7
    (0x5B, 0x60),    // Keypad 8
    (0x5C, 0x61),    // Keypad 9
    (0x52, 0x62),    // Keypad 0
    (0x41, 0x63),    // Keypad .
    (0x0A, 0x64),    // ISO § (Non-US \)
    (0x6E, 0x65),    // Application / context menu
    (0x51, 0x67),    // Keypad =
    (0x69, 0x68),    // F13
    (0x6B, 0x69),    // F14
    (0x71, 0x6A),    // F15
    (0x6A, 0x6B),    // F16
    (0x40, 0x6C),    // F17
    (0x4F, 0x6D),    // F18
    (0x50, 0x6E),    // F19
    (0x5A, 0x6F),    // F20
    (0x4A, 0x7F),    // Mute
    (0x48, 0x80),    // Volume Up
    (0x49, 0x81),    // Volume Down
    (0x5F, 0x85),    // JIS Keypad ,
    (0x5E, 0x87),    // JIS _ (International1)
    (0x5D, 0x89),    // JIS ¥ (International3)
    (0x68, 0x90),    // JIS Kana (Lang1)
    (0x66, 0x91),    // JIS Eisu (Lang2)
    (0x3B, 0xE0),    // Left Control
    (0x38, 0xE1),    // Left Shift
    (0x3A, 0xE2),    // Left Option
    (0x37, 0xE3),    // Left Command
    (0x3E, 0xE4),    // Right Control
    (0x3C, 0xE5),    // Right Shift
    (0x3D, 0xE6),    // Right Option
    (0x36, 0xE7),    // Right Command
    (0x3F, hid::FN), // Fn / Globe
];

/// Maps a macOS virtual key code to a HID usage, or 0 if unknown.
pub fn mac_to_hid(kvk: u16) -> u16 {
    MAC.iter().find(|(k, _)| *k == kvk).map(|(_, h)| *h).unwrap_or(0)
}

/// Maps a HID usage to a macOS virtual key code.
pub fn hid_to_mac(key: u16) -> Option<u16> {
    MAC.iter().find(|(_, h)| *h == key).map(|(k, _)| *k)
}

/// macOS `CGEventFlags` for a modifier key: the generic mask plus the
/// device-specific left/right bit, so apps that check sides behave.
pub fn mac_flags_for_key(key: u16) -> u64 {
    const SHIFT: u64 = 0x0002_0000;
    const CONTROL: u64 = 0x0004_0000;
    const ALTERNATE: u64 = 0x0008_0000;
    const COMMAND: u64 = 0x0010_0000;
    const SECONDARY_FN: u64 = 0x0080_0000;
    match key {
        hid::LEFT_CONTROL => CONTROL | 0x0001,
        hid::LEFT_SHIFT => SHIFT | 0x0002,
        hid::RIGHT_SHIFT => SHIFT | 0x0004,
        hid::LEFT_META => COMMAND | 0x0008,
        hid::RIGHT_META => COMMAND | 0x0010,
        hid::LEFT_ALT => ALTERNATE | 0x0020,
        hid::RIGHT_ALT => ALTERNATE | 0x0040,
        hid::RIGHT_CONTROL => CONTROL | 0x2000,
        hid::FN => SECONDARY_FN,
        _ => 0,
    }
}

/// macOS caps-lock (`kCGEventFlagMaskAlphaShift`).
pub const MAC_FLAG_CAPS_LOCK: u64 = 0x0001_0000;

/// Neutral modifier bits from macOS `CGEventFlags`.
pub fn mods_from_mac_flags(flags: u64) -> u32 {
    let mut m = 0;
    if flags & 0x0002_0000 != 0 {
        m |= mods::SHIFT;
    }
    if flags & 0x0004_0000 != 0 {
        m |= mods::CONTROL;
    }
    if flags & 0x0008_0000 != 0 {
        m |= mods::ALT;
    }
    if flags & 0x0010_0000 != 0 {
        m |= mods::META;
    }
    if flags & MAC_FLAG_CAPS_LOCK != 0 {
        m |= mods::CAPS_LOCK;
    }
    if flags & 0x0080_0000 != 0 {
        m |= mods::FN;
    }
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mac_table_is_a_bijection() {
        for (i, (k, h)) in MAC.iter().enumerate() {
            assert!(*h != 0);
            assert!(MAC[i + 1..].iter().all(|(k2, h2)| k2 != k && h2 != h), "duplicate {k:#x}/{h:#x}");
            assert_eq!(mac_to_hid(*k), *h);
            assert_eq!(hid_to_mac(*h), Some(*k));
        }
    }

    #[test]
    fn letters_and_modifiers_map_correctly() {
        assert_eq!(mac_to_hid(0x00), 0x04); // A
        assert_eq!(mac_to_hid(0x04), hid::H);
        assert_eq!(mac_to_hid(0x37), hid::LEFT_META);
        assert_eq!(modifier_bit(hid::RIGHT_ALT), mods::ALT);
        assert!(!is_modifier(0x04));
        assert_eq!(mac_to_hid(0xFF), 0);
    }

    #[test]
    fn mac_flags_round_trip_to_mods() {
        let flags = mac_flags_for_key(hid::LEFT_META) | mac_flags_for_key(hid::RIGHT_SHIFT) | MAC_FLAG_CAPS_LOCK;
        assert_eq!(mods_from_mac_flags(flags), mods::META | mods::SHIFT | mods::CAPS_LOCK);
    }
}
