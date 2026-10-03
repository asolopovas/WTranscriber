use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::{audio_cache_key, load_samples};
use crate::error::Result;

const MAX_ENTRIES: usize = 32;
const MAX_CACHED_PEAKS: usize = 4096;

struct Entry {
    path: PathBuf,
    source_key: String,
    bins: usize,
    peaks: Vec<f32>,
}

#[derive(Default)]
struct WaveformCache {
    entries: VecDeque<Entry>,
}

static CACHE: Mutex<WaveformCache> = Mutex::new(WaveformCache {
    entries: VecDeque::new(),
});

impl WaveformCache {
    fn get_or_load(
        cache: &Mutex<Self>,
        path: &Path,
        bins: usize,
        load: impl FnOnce(&Path) -> Result<Vec<f32>>,
    ) -> Result<Vec<f32>> {
        let path = std::path::absolute(path)?;
        let source_key = audio_cache_key(&path)?;
        {
            let mut cache = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache
                .entries
                .retain(|entry| entry.path != path || entry.source_key == source_key);
            if let Some(index) = cache.entries.iter().position(|entry| {
                entry.path == path && entry.source_key == source_key && entry.bins == bins
            }) {
                let entry = cache
                    .entries
                    .remove(index)
                    .expect("waveform cache index exists");
                let peaks = entry.peaks.clone();
                cache.entries.push_back(entry);
                drop(cache);
                return Ok(peaks);
            }
        }
        let samples = load(&path)?;
        let peaks = if samples.is_empty() || bins == 0 {
            Vec::new()
        } else {
            let step = samples.len().div_ceil(bins.min(samples.len()));
            samples
                .chunks(step)
                .map(|chunk| {
                    chunk
                        .iter()
                        .fold(0.0_f32, |acc, v| acc.max(v.abs()))
                        .min(1.0)
                })
                .collect::<Vec<_>>()
        };
        if peaks.len() <= MAX_CACHED_PEAKS && audio_cache_key(&path)? == source_key {
            let mut cache = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache
                .entries
                .retain(|entry| entry.path != path || entry.bins != bins);
            if cache.entries.len() == MAX_ENTRIES {
                cache.entries.pop_front();
            }
            cache.entries.push_back(Entry {
                path,
                source_key,
                bins,
                peaks: peaks.clone(),
            });
        }
        Ok(peaks)
    }
}

pub fn waveform_peaks(path: &Path, bins: usize) -> Result<Vec<f32>> {
    WaveformCache::get_or_load(&CACHE, path, bins, load_samples)
}

pub(super) fn clear_cache() {
    CACHE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .entries
        .clear();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs::FileTimes, time::SystemTime};

    #[test]
    fn decoding_does_not_lock_out_cached_waveforms() {
        let dir = tempfile::tempdir().unwrap();
        let ready = dir.path().join("ready.wav");
        let loading = dir.path().join("loading.wav");
        std::fs::write(&ready, b"audio").unwrap();
        std::fs::write(&loading, b"audio").unwrap();
        let cache = Mutex::new(WaveformCache::default());
        WaveformCache::get_or_load(&cache, &ready, 2, |_| Ok(vec![0.2])).unwrap();
        WaveformCache::get_or_load(&cache, &loading, 2, |_| {
            assert!(cache.try_lock().is_ok());
            assert_eq!(
                WaveformCache::get_or_load(&cache, &ready, 2, |_| panic!("cached audio reloaded"))?,
                vec![0.2]
            );
            Ok(vec![0.5])
        })
        .unwrap();
    }

    #[test]
    fn reuses_peaks_but_separates_bins_and_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        let other = dir.path().join("other.wav");
        std::fs::write(&path, b"audio").unwrap();
        std::fs::write(&other, b"audio").unwrap();
        let cache = Mutex::new(WaveformCache::default());
        let peaks = WaveformCache::get_or_load(&cache, &path, 2, |_| Ok(vec![0.1, -0.8, 0.2, 1.5]))
            .unwrap();
        assert_eq!(peaks, vec![0.8, 1.0]);
        assert_eq!(
            WaveformCache::get_or_load(&cache, &path, 2, |_| panic!("cached audio was reloaded"))
                .unwrap(),
            peaks
        );
        assert_eq!(
            WaveformCache::get_or_load(&cache, &path, 1, |_| Ok(vec![0.4])).unwrap(),
            vec![0.4]
        );
        assert_eq!(
            WaveformCache::get_or_load(&cache, &other, 2, |_| Ok(vec![0.6])).unwrap(),
            vec![0.6]
        );
    }

    #[test]
    fn invalidates_after_size_or_modification_time_changes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        std::fs::write(&path, b"audio").unwrap();
        let cache = Mutex::new(WaveformCache::default());
        WaveformCache::get_or_load(&cache, &path, 2, |_| Ok(vec![0.1])).unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        std::fs::write(&path, b"longer audio").unwrap();
        let file = std::fs::File::options().write(true).open(&path).unwrap();
        file.set_times(FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(
            WaveformCache::get_or_load(&cache, &path, 2, |_| Ok(vec![0.2])).unwrap(),
            vec![0.2]
        );
        file.set_times(FileTimes::new().set_modified(SystemTime::UNIX_EPOCH))
            .unwrap();
        assert_eq!(
            WaveformCache::get_or_load(&cache, &path, 2, |_| Ok(vec![0.3])).unwrap(),
            vec![0.3]
        );
        assert_eq!(cache.lock().unwrap().entries.len(), 1);
        std::fs::remove_file(&path).unwrap();
        assert!(
            WaveformCache::get_or_load(&cache, &path, 2, |_| panic!("missing source")).is_err()
        );
    }

    #[test]
    fn retries_failures_and_does_not_cache_a_source_changed_during_loading() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        std::fs::write(&path, b"audio").unwrap();
        let cache = Mutex::new(WaveformCache::default());
        assert!(
            WaveformCache::get_or_load(&cache, &path, 2, |_| Err(std::io::Error::other(
                "decode failed"
            )
            .into()))
            .is_err()
        );
        assert!(cache.lock().unwrap().entries.is_empty());
        WaveformCache::get_or_load(&cache, &path, 2, |path| {
            std::fs::write(path, b"changed audio")?;
            Ok(vec![0.2])
        })
        .unwrap();
        assert!(cache.lock().unwrap().entries.is_empty());
        assert_eq!(
            WaveformCache::get_or_load(&cache, &path, 2, |_| Ok(vec![0.3])).unwrap(),
            vec![0.3]
        );
    }

    #[test]
    fn evicts_old_entries_and_skips_oversized_results() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        std::fs::write(&path, b"audio").unwrap();
        let cache = Mutex::new(WaveformCache::default());
        for bins in 1..=MAX_ENTRIES + 1 {
            WaveformCache::get_or_load(&cache, &path, bins, |_| Ok(vec![0.1])).unwrap();
        }
        assert_eq!(cache.lock().unwrap().entries.len(), MAX_ENTRIES);
        assert_eq!(cache.lock().unwrap().entries.front().unwrap().bins, 2);
        WaveformCache::get_or_load(&cache, &path, MAX_CACHED_PEAKS + 1, |_| {
            Ok(vec![0.1; MAX_CACHED_PEAKS + 1])
        })
        .unwrap();
        assert_eq!(
            cache.lock().unwrap().entries.back().unwrap().bins,
            MAX_ENTRIES + 1
        );
    }
}
