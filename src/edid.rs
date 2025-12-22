use crate::state::MonitorIdentity;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub fn parse_identity(edid: &[u8]) -> Option<MonitorIdentity> {
    if edid.len() < 128 { return None; }
    
    // Parse Manufacturer ID (0x08 - 0x09) - Big Endian
    let mfg_raw = ((edid[0x08] as u16) << 8) | (edid[0x09] as u16);
    let char1 = ((mfg_raw >> 10) & 0x1F) as u8;
    let char2 = ((mfg_raw >> 5) & 0x1F) as u8;
    let char3 = (mfg_raw & 0x1F) as u8;
    
    // Manufacturer ID characters are 1-based (1='A', 2='B', ...).
    // If 0, it's invalid but we map to '@' or '?' for safety? 
    // Usually valid EDID has non-zero. 'A' is 0x41.
    let mfg_char = |c: u8| -> char {
        if c > 0 && c <= 26 { (c + b'A' - 1) as char } else { '?' }
    };
    
    let manufacturer_id = format!("{}{}{}", mfg_char(char1), mfg_char(char2), mfg_char(char3));
    
    // Product Code (0x0A - 0x0B) - Little Endian
    let product_code = (edid[0x0A] as u16) | ((edid[0x0B] as u16) << 8);
    
    // Serial Number (0x0C - 0x0F) - Little Endian
    // This is the numeric serial number.
    let serial_num = (edid[0x0C] as u32) | ((edid[0x0D] as u32) << 8) |
                     ((edid[0x0E] as u32) << 16) | ((edid[0x0F] as u32) << 24);
    
    let mut serial_str = if serial_num != 0 {
        serial_num.to_string()
    } else {
        String::new()
    };
    
    // Scan descriptors for Serial Number String (Tag 0xFF)
    // Descriptors start at 0x36. 4 of them. 18 bytes each.
    for i in 0..4 {
        let offset = 0x36 + (i * 18);
        if edid.len() < offset + 18 { break; }
        let block = &edid[offset..offset+18];
        
        // Detailed Timing Descriptor if bytes 0-1 are not zero.
        if block[0] != 0 || block[1] != 0 { continue; }
        
        // Display Descriptor
        // byte 3 is tag.
        if block[3] == 0xFF { // Serial Number
            // String is at offset 5, up to 13 bytes. Terminated by 0x0A.
            let text_slice = &block[5..];
            // Find termination (0x0A)
            let len = text_slice.iter().position(|&c| c == 0x0A).unwrap_or(text_slice.len());
            let s = String::from_utf8_lossy(&text_slice[..len]).trim().to_string();
            if !s.is_empty() {
                serial_str = s;
                break; // Found string serial, prefer it over numeric? 
                // Spec usually implies string is "better" if present.
            }
        }
    }
    
    let is_fallback = serial_str.is_empty() || serial_str == "0";
    
    if is_fallback {
        let mut hasher = DefaultHasher::new();
        edid.hash(&mut hasher);
        let hash = hasher.finish();
        serial_str = format!("{:x}", hash);
    }
    
    Some(MonitorIdentity {
        manufacturer_id,
        product_code,
        serial: serial_str,
        is_fallback,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_identity() {
        let mut edid = vec![0u8; 128];
        // Header
        edid[0..8].copy_from_slice(&[0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x00]);
        
        // Mfg ID: "DEL" (0x10AC)
        // D=4 (00100), E=5 (00101), L=12 (01100)
        // 00100 00101 01100 = 0x10AC
        let mfg = 0x10AC_u16;
        edid[0x08] = (mfg >> 8) as u8;
        edid[0x09] = mfg as u8;
        
        // Product Code: 0xA050 (LE -> 50 A0)
        edid[0x0A] = 0x50;
        edid[0x0B] = 0xA0;
        
        // Serial: 12345678 (0x00BC614E) -> LE: 4E 61 BC 00
        edid[0x0C] = 0x4E;
        edid[0x0D] = 0x61;
        edid[0x0E] = 0xBC;
        edid[0x0F] = 0x00;
        
        let ident = parse_identity(&edid).unwrap();
        assert_eq!(ident.manufacturer_id, "DEL");
        assert_eq!(ident.product_code, 0xA050);
        assert_eq!(ident.serial, "12345678");
        assert_eq!(ident.is_fallback, false);
    }

    #[test]
    fn test_parse_serial_string() {
        let mut edid = vec![0u8; 128];
        // Mfg "ABC"
        // A=1, B=2, C=3 => 00001 00010 00011 => 0x0443
        let mfg = 0x0443_u16;
        edid[0x08] = (mfg >> 8) as u8;
        edid[0x09] = mfg as u8;
        
        // Product
        edid[0x0A] = 0x01;
        edid[0x0B] = 0x01;
        
        // Numeric serial = 0
        
        // Descriptor 1 at 0x36: Serial String "SN123"
        let offset = 0x36;
        edid[offset] = 0x00;
        edid[offset+1] = 0x00;
        edid[offset+2] = 0x00;
        edid[offset+3] = 0xFF; // Tag
        edid[offset+4] = 0x00;
        
        let sn = b"SN123\n";
        edid[offset+5..offset+5+sn.len()].copy_from_slice(sn);
        
        let ident = parse_identity(&edid).unwrap();
        assert_eq!(ident.manufacturer_id, "ABC");
        assert_eq!(ident.serial, "SN123");
        assert_eq!(ident.is_fallback, false);
    }

    #[test]
    fn test_fallback_hash_determinism() {
        let mut edid = vec![0u8; 128];
        edid[0x08] = 0x10; // "DEL"
        edid[0x09] = 0xAC;
        
        let ident1 = parse_identity(&edid).unwrap();
        let ident2 = parse_identity(&edid).unwrap();
        
        assert_eq!(ident1.manufacturer_id, "DEL");
        assert_eq!(ident1.is_fallback, true);
        assert_eq!(ident1.serial, ident2.serial); // Determinism
        
        // Change one byte
        edid[127] = 0xFF;
        let ident3 = parse_identity(&edid).unwrap();
        assert_ne!(ident1.serial, ident3.serial); // Sensitivity
    }
}
