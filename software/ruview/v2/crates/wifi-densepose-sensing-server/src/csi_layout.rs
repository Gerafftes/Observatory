//! Explicit S3/IDF-5.4 physical CSI evidence. Legacy lengths are not attestations.
//! Additive trailer v1; base v2/v3 bytes and reserved fields remain unchanged.
use crate::raw_csi_recording::RawCsiFrame;
use serde::{Deserialize, Serialize};
pub const MAGIC: u32 = 0xc511_a063;
pub const WIRE_LEN: usize = 32;
pub const PROFILE: u32 = 0x0504_0009;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Evidence {
    pub sig_mode: u8,
    pub bandwidth: u8,
    pub stbc: u8,
    pub secondary: u8,
    pub processing: u8,
    pub shift: u8,
    pub iq_order: u8,
    pub first_word_invalid: bool,
    pub iq_bytes: u16,
    pub rx_state: u8,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutKey {
    pub contract: u8,
    pub profile: u32,
    pub sig_mode: u8,
    pub bandwidth: u8,
    pub stbc: u8,
    pub secondary: u8,
    pub processing: u8,
    pub shift: u8,
    pub iq_order: u8,
    pub iq_bytes: u16,
}
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut c = !0u32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = (c >> 1) ^ (0xedb8_8320 & 0u32.wrapping_sub(c & 1));
        }
    }
    !c
}
impl Evidence {
    pub fn decode(b: &[u8]) -> Result<Self, &'static str> {
        if b.len() != WIRE_LEN
            || b[..4] != MAGIC.to_le_bytes()
            || b[4] != 1
            || b[5] != 9
            || b[6..8] != (WIRE_LEN as u16).to_le_bytes()
            || b[19] != 0
            || b[20..24] != PROFILE.to_le_bytes()
            || b[24..28] != [0; 4]
            || b[15] > 1
            || b[12] & !0x3f != 0
            || b[13] > 15
            || b[14] != 1
            || crc32(&b[..28]) != u32::from_le_bytes(b[28..32].try_into().unwrap())
        {
            return Err("invalid physical CSI trailer version/profile/reserved/CRC");
        }
        Ok(Self {
            sig_mode: b[8],
            bandwidth: b[9],
            stbc: b[10],
            secondary: b[11],
            processing: b[12],
            shift: b[13],
            iq_order: b[14],
            first_word_invalid: b[15] != 0,
            iq_bytes: u16::from_le_bytes([b[16], b[17]]),
            rx_state: b[18],
        })
    }
    pub fn encode(self) -> [u8; WIRE_LEN] {
        let mut b = [0; WIRE_LEN];
        b[..4].copy_from_slice(&MAGIC.to_le_bytes());
        b[4] = 1;
        b[5] = 9;
        b[6..8].copy_from_slice(&(WIRE_LEN as u16).to_le_bytes());
        b[8] = self.sig_mode;
        b[9] = self.bandwidth;
        b[10] = self.stbc;
        b[11] = self.secondary;
        b[12] = self.processing;
        b[13] = self.shift;
        b[14] = self.iq_order;
        b[15] = u8::from(self.first_word_invalid);
        b[16..18].copy_from_slice(&self.iq_bytes.to_le_bytes());
        b[18] = self.rx_state;
        b[20..24].copy_from_slice(&PROFILE.to_le_bytes());
        let c = crc32(&b[..28]);
        b[28..].copy_from_slice(&c.to_le_bytes());
        b
    }
    pub fn validate_frame(&self, f: &RawCsiFrame) -> Result<(), &'static str> {
        Self::decode(&self.encode())?;
        if usize::from(self.iq_bytes) != f.iq_pairs.len() * 2
            || self.first_word_invalid != (f.flags & 0x20 != 0)
            || (self.bandwidth != 0) != (f.flags & 1 != 0)
            || (self.stbc != 0) != (f.flags & 4 != 0)
        {
            return Err("physical metadata contradicts wire header or IQ length");
        }
        Ok(())
    }
    pub fn key(self) -> LayoutKey {
        LayoutKey {
            contract: 1,
            profile: PROFILE,
            sig_mode: self.sig_mode,
            bandwidth: self.bandwidth,
            stbc: self.stbc,
            secondary: self.secondary,
            processing: self.processing,
            shift: self.shift,
            iq_order: self.iq_order,
            iq_bytes: self.iq_bytes,
        }
    }
    /// IDF's ordered LLTF, HT-LTF, STBC-HT-LTF blocks on HT20 with no secondary.
    /// Other channel/PHY combinations stay transport-readable but not feature-eligible.
    pub fn mask(self) -> Result<Vec<bool>, &'static str> {
        if self.rx_state != 0
            || self.bandwidth != 0
            || self.secondary != 0
            || self.stbc > 1
            || self.sig_mode > 1
            || (self.sig_mode == 0 && self.stbc != 0)
            || self.iq_order != 1
        {
            return Err("unqualified CSI physical layout or receive state");
        }
        let mut blocks = Vec::new();
        if self.processing & 1 != 0 {
            blocks.push(26i32);
        }
        if self.sig_mode == 1 && self.processing & 2 != 0 {
            blocks.push(28);
        }
        if self.sig_mode == 1 && self.stbc != 0 && self.processing & 4 != 0 {
            blocks.push(28);
        }
        if blocks.is_empty() || usize::from(self.iq_bytes) != blocks.len() * 128 {
            return Err("LTF selection and exact length disagree");
        }
        let mut mask = Vec::new();
        for limit in blocks {
            for i in 0..64 {
                let bin: i32 = if i < 32 { i } else { i - 64 };
                mask.push(bin != 0 && bin.abs() <= limit);
            }
        }
        if self.first_word_invalid {
            mask[0] = false;
            mask[1] = false;
        }
        if mask.iter().filter(|&&x| x).count() < 8 {
            return Err("too few physically valid CSI bins");
        }
        Ok(mask)
    }
}
pub fn qualified(f: &RawCsiFrame) -> Result<(LayoutKey, Vec<bool>), String> {
    let e = f
        .physical_layout
        .ok_or("physical CSI layout not attested; diagnostic-only legacy frame")?;
    e.validate_frame(f)?;
    if f.antenna_count != 1 {
        return Err("unqualified antenna layout".into());
    }
    Ok((e.key(), e.mask()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_ltf_masks_exclude_dc_guard_and_invalid_prefix() {
        let e=Evidence{sig_mode:1,bandwidth:0,stbc:0,secondary:0,processing:15,shift:0,iq_order:1,
            first_word_invalid:false,iq_bytes:256,rx_state:0};
        let mask=e.mask().unwrap(); assert_eq!(mask.iter().filter(|v|**v).count(),108);
        assert!(!mask[0] && !mask[32] && !mask[64]);
        let mut invalid=e; invalid.first_word_invalid=true;
        assert!(!invalid.mask().unwrap()[1]);
        invalid.sig_mode=0; invalid.iq_bytes=128;
        assert_eq!(invalid.mask().unwrap().iter().filter(|v|**v).count(),51);
        assert_eq!(Evidence::decode(&e.encode()).unwrap(),e);
        let mut corrupt=e.encode(); corrupt[12]^=1; assert!(Evidence::decode(&corrupt).is_err());
    }
}
