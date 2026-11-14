//! The hit sound is made in code (a short bright click), so there are no asset files.

const RATE: u32 = 44_100;

pub fn hit_wav() -> Vec<u8> {
    let len = (RATE as f32 * 0.07) as usize;
    let mut noise = 0x1234_5678_u32;
    let samples: Vec<i16> = (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            noise ^= noise << 13;
            noise ^= noise >> 17;
            noise ^= noise << 5;
            let n = (noise as f32 / u32::MAX as f32) * 2.0 - 1.0;
            let tone = (t * 1900.0 * std::f32::consts::TAU).sin() * (-t * 70.0).exp();
            let snap = n * (-t * 400.0).exp() * 0.5;
            ((tone * 0.6 + snap) * 0.9 * i16::MAX as f32) as i16
        })
        .collect();
    wav(&samples)
}

fn wav(samples: &[i16]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&s.to_le_bytes());
    }
    b
}
