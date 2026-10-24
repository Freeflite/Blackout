//! Hotkey text such as "Ctrl+Alt+B", turned into what RegisterHotKey wants.

use std::fmt;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hotkey {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
    key: String,
    pub vk: u32,
}

/// Named keys besides letters, digits and F1-F24, with their virtual-key codes.
const NAMED_KEYS: &[(&str, u32)] = &[
    ("Backspace", 0x08),
    ("Tab", 0x09),
    ("Enter", 0x0D),
    ("Escape", 0x1B),
    ("Space", 0x20),
    ("PageUp", 0x21),
    ("PageDown", 0x22),
    ("End", 0x23),
    ("Home", 0x24),
    ("Left", 0x25),
    ("Up", 0x26),
    ("Right", 0x27),
    ("Down", 0x28),
    ("Insert", 0x2D),
    ("Delete", 0x2E),
    ("Pause", 0x13),
    ("ScrollLock", 0x91),
];

impl Hotkey {
    pub fn parse(text: &str) -> Result<Self, String> {
        let mut hotkey = Hotkey { ctrl: false, alt: false, shift: false, win: false, key: String::new(), vk: 0 };
        for part in text.split('+').map(str::trim) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => hotkey.ctrl = true,
                "alt" => hotkey.alt = true,
                "shift" => hotkey.shift = true,
                "win" | "windows" => hotkey.win = true,
                _ if hotkey.vk != 0 => return Err("only one key can go with the modifiers".into()),
                _ => {
                    let (key, vk) = key_code(part).ok_or_else(|| format!("\"{part}\" isn't a key Blackout knows"))?;
                    hotkey.key = key;
                    hotkey.vk = vk;
                }
            }
        }
        if hotkey.vk == 0 {
            return Err("it needs a key to go with the modifiers".into());
        }
        if !(hotkey.ctrl || hotkey.alt || hotkey.win) {
            return Err("it needs Ctrl, Alt or Win, so it can't get in the way of typing".into());
        }
        Ok(hotkey)
    }

    /// Modifiers for RegisterHotKey; MOD_NOREPEAT so holding the keys down fires once.
    pub fn modifiers(&self) -> HOT_KEY_MODIFIERS {
        let mut modifiers = MOD_NOREPEAT;
        for (on, flag) in [(self.ctrl, MOD_CONTROL), (self.alt, MOD_ALT), (self.shift, MOD_SHIFT), (self.win, MOD_WIN)] {
            if on {
                modifiers |= flag;
            }
        }
        modifiers
    }
}

/// A single key with no modifiers, such as the key tapped to wake a full blackout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Key {
    pub name: String,
    pub vk: u32,
}

impl Key {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        key_code(text).map(|(name, vk)| Key { name, vk }).ok_or_else(|| format!("\"{text}\" isn't a key Blackout knows"))
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (on, name) in [(self.ctrl, "Ctrl+"), (self.alt, "Alt+"), (self.shift, "Shift+"), (self.win, "Win+")] {
            if on {
                f.write_str(name)?;
            }
        }
        f.write_str(&self.key)
    }
}

/// Canonical name and virtual-key code for a key name, any case.
fn key_code(name: &str) -> Option<(String, u32)> {
    let upper = name.to_ascii_uppercase();
    if let [c] = upper.as_bytes() {
        // Letters and digits are their own virtual-key codes.
        return c.is_ascii_alphanumeric().then(|| (upper.clone(), *c as u32));
    }
    if let Some(n) = upper.strip_prefix('F').and_then(|n| n.parse::<u32>().ok()) {
        return (1..=24).contains(&n).then(|| (format!("F{n}"), 0x6F + n));
    }
    NAMED_KEYS.iter().find(|(key, _)| key.eq_ignore_ascii_case(name)).map(|&(key, vk)| (key.to_string(), vk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_default() {
        let hotkey = Hotkey::parse("Ctrl+Alt+B").unwrap();
        assert_eq!(hotkey.vk, 0x42);
        assert_eq!(hotkey.modifiers(), MOD_NOREPEAT | MOD_CONTROL | MOD_ALT);
        assert_eq!(hotkey.to_string(), "Ctrl+Alt+B");
    }

    #[test]
    fn normalizes_case_spacing_and_order() {
        assert_eq!(Hotkey::parse("alt + ctrl + b").unwrap().to_string(), "Ctrl+Alt+B");
        assert_eq!(Hotkey::parse("ctrl+pageup").unwrap().to_string(), "Ctrl+PageUp");
    }

    #[test]
    fn knows_function_digit_and_named_keys() {
        assert_eq!(Hotkey::parse("Ctrl+F12").unwrap().vk, 0x7B);
        assert_eq!(Hotkey::parse("Win+7").unwrap().vk, 0x37);
        assert_eq!(Hotkey::parse("Ctrl+Shift+Pause").unwrap().vk, 0x13);
    }

    #[test]
    fn single_keys_for_waking() {
        assert_eq!(Key::parse("enter").unwrap(), Key { name: "Enter".into(), vk: 0x0D });
        assert_eq!(Key::parse(" space ").unwrap().vk, 0x20);
        assert_eq!(Key::parse("x").unwrap(), Key { name: "X".into(), vk: 0x58 });
        assert!(Key::parse("Ctrl+B").is_err());
        assert!(Key::parse("").is_err());
    }

    #[test]
    fn rejects_unusable_hotkeys() {
        assert!(Hotkey::parse("Shift+B").is_err(), "Shift alone would eat capital letters");
        assert!(Hotkey::parse("B").is_err());
        assert!(Hotkey::parse("Ctrl+Alt").is_err());
        assert!(Hotkey::parse("Ctrl+A+B").is_err());
        assert!(Hotkey::parse("Ctrl+F25").is_err());
        assert!(Hotkey::parse("Ctrl+Banana").is_err());
    }
}
