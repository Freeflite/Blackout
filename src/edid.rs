//! Just enough EDID parsing to tell monitors apart: manufacturer, product code, serial and model name.

#[derive(Clone, Debug, PartialEq)]
pub struct Edid {
    /// Three-letter PNP vendor code, e.g. "GSM" for LG.
    pub manufacturer: String,
    pub product: u16,
    pub serial: Option<String>,
    pub name: Option<String>,
}

impl Edid {
    /// The model's hardware ID as Windows writes it, e.g. "GSM776F".
    pub fn model_id(&self) -> String {
        format!("{}{:04X}", self.manufacturer, self.product)
    }
}

const HEADER: [u8; 8] = [0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00];
/// A numeric serial some panels ship with instead of a real one.
const PLACEHOLDER_SERIAL: u32 = 0x0101_0101;

pub fn parse(bytes: &[u8]) -> Option<Edid> {
    if bytes.len() < 128 || bytes[..8] != HEADER {
        return None;
    }
    let packed = u16::from_be_bytes([bytes[8], bytes[9]]);
    let letter = |shift: u16| (b'A' - 1 + ((packed >> shift) & 0x1F) as u8) as char;
    let manufacturer = [letter(10), letter(5), letter(0)].iter().collect();
    let product = u16::from_le_bytes([bytes[10], bytes[11]]);
    let serial_number = u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]);

    let (mut serial, mut name) = (None, None);
    for start in [54, 72, 90, 108] {
        let descriptor = &bytes[start..start + 18];
        if descriptor[..3] != [0, 0, 0] {
            continue; // a detailed timing, not a text descriptor
        }
        match descriptor[3] {
            0xFF => serial = descriptor_text(&descriptor[5..]),
            0xFC => name = descriptor_text(&descriptor[5..]),
            _ => {}
        }
    }
    // Prefer the text serial, which is what's printed on the monitor's label.
    let serial = serial.or_else(|| {
        (serial_number != 0 && serial_number != PLACEHOLDER_SERIAL).then(|| format!("{serial_number:08X}"))
    });
    Some(Edid { manufacturer, product, serial, name })
}

/// A text descriptor's payload: up to 13 ASCII bytes, ended by a line feed and padded with spaces.
fn descriptor_text(raw: &[u8]) -> Option<String> {
    let text: String = raw
        .iter()
        .take_while(|&&b| b != b'\n')
        .filter(|b| b.is_ascii_graphic() || **b == b' ')
        .map(|&b| b as char)
        .collect();
    let text = text.trim();
    (text.chars().any(|c| c != '0')).then(|| text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(serial_text: Option<&str>, serial_number: u32) -> Vec<u8> {
        let mut edid = vec![0u8; 128];
        edid[..8].copy_from_slice(&HEADER);
        edid[8..10].copy_from_slice(&0x1E6Du16.to_be_bytes()); // "GSM"
        edid[10..12].copy_from_slice(&0x776Fu16.to_le_bytes());
        edid[12..16].copy_from_slice(&serial_number.to_le_bytes());
        edid[54] = 0x01; // first slot holds a detailed timing
        put_text(&mut edid, 72, 0xFC, "LG ULTRAGEAR");
        if let Some(serial) = serial_text {
            put_text(&mut edid, 90, 0xFF, serial);
        }
        edid
    }

    fn put_text(edid: &mut [u8], start: usize, tag: u8, text: &str) {
        edid[start + 3] = tag;
        let payload = &mut edid[start + 5..start + 18];
        payload.fill(b' ');
        payload[..text.len()].copy_from_slice(text.as_bytes());
        if text.len() < 13 {
            payload[text.len()] = b'\n';
        }
    }

    #[test]
    fn reads_identity_from_text_descriptors() {
        let edid = parse(&sample(Some("305NTZX56789"), 0x0001_2345)).unwrap();
        assert_eq!(edid.manufacturer, "GSM");
        assert_eq!(edid.model_id(), "GSM776F");
        assert_eq!(edid.name.as_deref(), Some("LG ULTRAGEAR"));
        assert_eq!(edid.serial.as_deref(), Some("305NTZX56789"));
    }

    #[test]
    fn falls_back_to_numeric_serial() {
        let edid = parse(&sample(None, 0x1234_ABCD)).unwrap();
        assert_eq!(edid.serial.as_deref(), Some("1234ABCD"));
    }

    #[test]
    fn treats_blank_and_placeholder_serials_as_missing() {
        assert_eq!(parse(&sample(None, 0)).unwrap().serial, None);
        assert_eq!(parse(&sample(None, PLACEHOLDER_SERIAL)).unwrap().serial, None);
        assert_eq!(parse(&sample(Some("000000000000"), 0)).unwrap().serial, None);
    }

    #[test]
    fn rejects_non_edid_data() {
        assert!(parse(&[0u8; 128]).is_none());
        assert!(parse(&HEADER).is_none());
    }
}
