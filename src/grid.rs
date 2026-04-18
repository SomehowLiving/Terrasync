use sha2::{Digest, Sha256};

pub fn region_id(x: usize, y: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{x}:{y}"));
    hex::encode(hasher.finalize())
}

pub fn parse_xy(s: &str) -> Option<(usize, usize)> {
    let (x, y) = s.split_once(',')?;
    Some((x.parse().ok()?, y.parse().ok()?))
}
