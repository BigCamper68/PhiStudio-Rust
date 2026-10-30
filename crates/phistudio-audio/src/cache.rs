//! Disposable, content-addressed, versioned cache. Corruption causes recomputation.
//! Chart packages, recovery and user resources never contain this data.
use crate::{
    ALGORITHM_VERSION, Analysis, Candidate, Control, Error, MAX_SECONDS, StereoSource, Waveform,
    waveform::Bin,
};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const MAGIC: &[u8; 8] = b"PHIAUD01";
pub fn fingerprint(source: &impl StereoSource, control: &Control) -> Result<String, Error> {
    crate::validate(source)?;
    let mut hash = Sha256::new();
    hash.update(source.sample_rate().to_le_bytes());
    hash.update((source.frame_count() as u64).to_le_bytes());
    let mut bytes = Vec::with_capacity(8192 * 8);
    for first in (0..source.frame_count()).step_by(8192) {
        control.progress((first as u64 * 100 / source.frame_count() as u64) as u32)?;
        bytes.clear();
        for i in first..(first + 8192).min(source.frame_count()) {
            for v in source.frame(i) {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
        }
        hash.update(&bytes);
    }
    Ok(format!("{:x}", hash.finalize()))
}
fn directory(root: &Path) -> std::io::Result<PathBuf> {
    let path = root.join("audio-analysis-cache");
    fs::create_dir_all(&path)?;
    let meta = fs::symlink_metadata(&path)?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    Ok(path)
}
fn file(root: &Path, key: &str) -> std::io::Result<PathBuf> {
    if key.len() != 64 || !key.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    Ok(directory(root)?.join(format!("v{ALGORITHM_VERSION}-{key}.bin")))
}
struct Input<R> {
    stream: R,
    hash: Sha256,
}
impl<R: Read> Input<R> {
    fn bytes<const N: usize>(&mut self) -> Result<[u8; N], Error> {
        let mut b = [0; N];
        self.stream.read_exact(&mut b).map_err(|_| Error::Cache)?;
        self.hash.update(b);
        Ok(b)
    }
    fn u32(&mut self) -> Result<u32, Error> {
        Ok(u32::from_le_bytes(self.bytes()?))
    }
    fn f32(&mut self) -> Result<f32, Error> {
        Ok(f32::from_le_bytes(self.bytes()?))
    }
}
pub fn load(root: &Path, key: &str, control: &Control) -> Option<Analysis> {
    read(root, key, control).ok()
}
fn read(root: &Path, key: &str, control: &Control) -> Result<Analysis, Error> {
    let path = file(root, key).map_err(|_| Error::Cache)?;
    let meta = fs::symlink_metadata(&path).map_err(|_| Error::Cache)?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() > MAX_BYTES {
        return Err(Error::Cache);
    }
    let file = File::open(&path).map_err(|_| Error::Cache)?;
    let mut r = Input {
        stream: BufReader::new(file),
        hash: Sha256::new(),
    };
    if &r.bytes::<8>()? != MAGIC
        || r.u32()? != ALGORITHM_VERSION
        || r.bytes::<64>()? != key.as_bytes()
    {
        return Err(Error::Cache);
    }
    let sr = r.u32()?;
    let frames = u64::from_le_bytes(r.bytes()?) as usize;
    let block = r.u32()? as usize;
    let bins = r.u32()? as usize;
    let candidates = r.u32()? as usize;
    if !(8000..=192000).contains(&sr)
        || frames == 0
        || frames > sr as usize * MAX_SECONDS
        || block != (sr as usize).div_ceil(500).next_power_of_two()
        || bins != frames.div_ceil(block)
        || candidates > MAX_SECONDS * crate::FRAME_RATE * 4
    {
        return Err(Error::Cache);
    }
    let expected = 8 + 4 + 64 + 4 + 8 + 4 + 4 + 4 + bins as u64 * 16 + candidates as u64 * 28 + 32;
    if expected != meta.len() {
        return Err(Error::Cache);
    }
    let mut base = Vec::with_capacity(bins);
    for i in 0..bins {
        if i % 4096 == 0 {
            control.check()?;
        }
        let b = Bin {
            min: r.f32()?,
            max: r.f32()?,
            energy: r.f32()?,
            count: r.u32()?,
        };
        if !b.min.is_finite()
            || !b.max.is_finite()
            || b.min > b.max
            || b.min < -16.
            || b.max > 16.
            || !b.energy.is_finite()
            || b.energy < 0.
            || b.count as usize != (frames - i * block).min(block)
            || b.energy > b.count as f32 * 256.001
        {
            return Err(Error::Cache);
        }
        base.push(b);
    }
    let mut peaks = Vec::with_capacity(candidates);
    let duration = frames as f64 / f64::from(sr);
    for i in 0..candidates {
        if i % 4096 == 0 {
            control.check()?;
        }
        let seconds = f64::from_le_bytes(r.bytes()?);
        let strength = [r.f32()?, r.f32()?, r.f32()?, r.f32()?];
        let band = r.u32()? as usize;
        if !(1..=3).contains(&band)
            || !seconds.is_finite()
            || !(0. ..duration).contains(&seconds)
            || strength
                .iter()
                .any(|s| !s.is_finite() || !(0. ..=1.).contains(s))
            || peaks
                .last()
                .is_some_and(|c: &Candidate| c.seconds > seconds)
        {
            return Err(Error::Cache);
        }
        peaks.push(Candidate {
            seconds,
            strength,
            band,
        });
    }
    let mut checksum = [0; 32];
    r.stream
        .read_exact(&mut checksum)
        .map_err(|_| Error::Cache)?;
    if r.hash.finalize().as_slice() != checksum {
        return Err(Error::Cache);
    }
    let _ = r
        .stream
        .get_ref()
        .set_modified(std::time::SystemTime::now());
    Ok(Analysis {
        waveform: Arc::new(Waveform::from_bins(sr, frames, block, base)),
        candidates: peaks,
    })
}
pub fn store(root: &Path, key: &str, analysis: &Analysis, control: &Control) -> Result<(), Error> {
    control.check()?;
    let path = file(root, key).map_err(|_| Error::Cache)?;
    let dir = path.parent().unwrap();
    let mut temp = tempfile::NamedTempFile::new_in(dir).map_err(|_| Error::Cache)?;
    {
        let mut w = BufWriter::new(temp.as_file_mut());
        let mut hash = Sha256::new();
        let mut put = |b: &[u8]| -> Result<(), Error> {
            hash.update(b);
            w.write_all(b).map_err(|_| Error::Cache)
        };
        let wave = &analysis.waveform;
        put(MAGIC)?;
        put(&ALGORITHM_VERSION.to_le_bytes())?;
        put(key.as_bytes())?;
        put(&wave.sample_rate.to_le_bytes())?;
        put(&(wave.frames as u64).to_le_bytes())?;
        put(&(wave.block as u32).to_le_bytes())?;
        put(&(wave.levels[0].len() as u32).to_le_bytes())?;
        put(&(analysis.candidates.len() as u32).to_le_bytes())?;
        for (i, b) in wave.levels[0].iter().enumerate() {
            if i % 4096 == 0 {
                control.check()?;
            }
            put(&b.min.to_le_bytes())?;
            put(&b.max.to_le_bytes())?;
            put(&b.energy.to_le_bytes())?;
            put(&b.count.to_le_bytes())?;
        }
        for (i, c) in analysis.candidates.iter().enumerate() {
            if i % 4096 == 0 {
                control.check()?;
            }
            put(&c.seconds.to_le_bytes())?;
            for v in c.strength {
                put(&v.to_le_bytes())?;
            }
            put(&(c.band as u32).to_le_bytes())?;
        }
        w.write_all(&hash.finalize()).map_err(|_| Error::Cache)?;
        w.flush().map_err(|_| Error::Cache)?;
    }
    control.check()?;
    temp.as_file().sync_all().map_err(|_| Error::Cache)?;
    temp.persist(&path).map_err(|_| Error::Cache)?;
    prune(dir);
    Ok(())
}
fn prune(dir: &Path) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|e| {
            let meta = fs::symlink_metadata(e.path()).ok()?;
            let name = e.file_name();
            let name = name.to_str()?;
            if !meta.is_file()
                || meta.file_type().is_symlink()
                || !name.starts_with("v1-")
                || !name.ends_with(".bin")
            {
                return None;
            }
            Some((meta.modified().ok()?, meta.len(), e.path()))
        })
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut bytes = 0;
    for (i, (_, size, path)) in entries.into_iter().enumerate() {
        bytes += size;
        if i >= 3 || bytes > 128 * 1024 * 1024 {
            let _ = fs::remove_file(path);
        }
    }
}
