//! Reference-track A/B player: plays a loaded track in place of the master output, optionally
//! level-matched to the processed programme, with a short cross-fade when switching.

use std::sync::Arc;

use crate::util::db_to_lin;

/// A decoded reference (already at the host sample rate).
#[derive(Debug)]
pub struct ReferenceTrack {
    pub l: Vec<f32>,
    pub r: Vec<f32>,
    /// Integrated loudness of the reference in LUFS.
    pub lufs: f32,
    pub name: String,
}

impl ReferenceTrack {
    pub fn len(&self) -> usize {
        self.l.len()
    }
    pub fn is_empty(&self) -> bool {
        self.l.is_empty()
    }
}

pub struct ReferencePlayer {
    track: Option<Arc<ReferenceTrack>>,
    pos: usize,
    /// Reference transport (independent of the host transport).
    pub playing: bool,
    /// Listen to the reference instead of the master.
    pub monitor: bool,
    pub looping: bool,
    /// Match the reference loudness to the processed programme.
    pub level_match: bool,
    pub trim_db: f32,
    xfade: f32,
    gain_lin: f32,
    mix_lufs: f32,
    have_mix_lufs: bool,
    applied_db: f32,
    sr: f32,
}

impl ReferencePlayer {
    pub fn new(sr: f32) -> Self {
        Self {
            track: None,
            pos: 0,
            playing: false,
            monitor: false,
            looping: true,
            level_match: true,
            trim_db: 0.0,
            xfade: 0.0,
            gain_lin: 1.0,
            mix_lufs: -23.0,
            have_mix_lufs: false,
            applied_db: 0.0,
            sr,
        }
    }

    pub fn set_sample_rate(&mut self, sr: f32) {
        self.sr = sr;
    }

    pub fn set_track(&mut self, t: Option<Arc<ReferenceTrack>>) {
        self.track = t;
        self.pos = 0;
    }

    pub fn has_track(&self) -> bool {
        self.track.is_some()
    }

    pub fn seek(&mut self, frac: f32) {
        if let Some(t) = &self.track {
            let n = t.l.len();
            self.pos = ((frac.clamp(0.0, 1.0) * n as f32) as usize).min(n.saturating_sub(1));
        }
    }

    /// Playback position 0..1.
    pub fn position(&self) -> f32 {
        match &self.track {
            Some(t) if !t.l.is_empty() => self.pos as f32 / t.l.len() as f32,
            _ => 0.0,
        }
    }

    pub fn length_seconds(&self) -> f32 {
        self.track.as_ref().map_or(0.0, |t| t.l.len() as f32 / self.sr)
    }

    pub fn track_lufs(&self) -> f32 {
        self.track.as_ref().map_or(-70.0, |t| t.lufs)
    }

    /// Gain currently applied to the reference in dB (level match + trim).
    pub fn applied_gain_db(&self) -> f32 {
        self.applied_db
    }

    /// Mix the reference into the master block. `processed_lufs` is the integrated loudness of the
    /// processed programme (used for level matching; ignored while the reference is monitored).
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], processed_lufs: f32) {
        // remember how loud the processed programme is while it is what we hear
        if self.xfade < 0.001 && processed_lufs > -60.0 {
            if self.have_mix_lufs {
                self.mix_lufs += (processed_lufs - self.mix_lufs) * 0.05;
            } else {
                self.mix_lufs = processed_lufs;
                self.have_mix_lufs = true;
            }
        }
        let target_x = if self.monitor && self.track.is_some() { 1.0 } else { 0.0 };
        if target_x == 0.0 && self.xfade == 0.0 && !self.playing {
            return;
        }
        let ref_lufs = self.track_lufs();
        let matched = if self.level_match && self.have_mix_lufs { (self.mix_lufs - ref_lufs).clamp(-24.0, 24.0) } else { 0.0 };
        self.applied_db = matched + self.trim_db;
        let g_new = db_to_lin(self.applied_db);
        let g_old = self.gain_lin;
        let n = l.len().max(1);
        let step = 1.0 / (0.005 * self.sr);
        for i in 0..l.len() {
            if self.xfade < target_x {
                self.xfade = (self.xfade + step).min(target_x);
            } else if self.xfade > target_x {
                self.xfade = (self.xfade - step).max(target_x);
            }
            let g = g_old + (g_new - g_old) * (i as f32 / n as f32);
            let (mut rl, mut rr) = (0.0f32, 0.0f32);
            if self.playing {
                if let Some(t) = self.track.as_ref() {
                    if self.pos < t.l.len() {
                        rl = t.l[self.pos];
                        rr = t.r[self.pos];
                        self.pos += 1;
                    }
                    if self.pos >= t.l.len() {
                        if self.looping {
                            self.pos = 0;
                        } else {
                            self.playing = false;
                        }
                    }
                }
            }
            l[i] = l[i] * (1.0 - self.xfade) + rl * g * self.xfade;
            r[i] = r[i] * (1.0 - self.xfade) + rr * g * self.xfade;
        }
        self.gain_lin = g_new;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(n: usize, amp: f32) -> Arc<ReferenceTrack> {
        let l: Vec<f32> = (0..n).map(|i| amp * (i as f32 * 0.05).sin()).collect();
        Arc::new(ReferenceTrack { r: l.clone(), l, lufs: -20.0, name: "t".into() })
    }

    #[test]
    fn passes_audio_through_when_not_monitoring() {
        let mut p = ReferencePlayer::new(48000.0);
        p.set_track(Some(track(48000, 0.5)));
        p.playing = true;
        let mut l = vec![0.25f32; 480];
        let mut r = l.clone();
        p.process(&mut l, &mut r, -14.0);
        assert!(l.iter().all(|x| (*x - 0.25).abs() < 1e-6));
    }

    #[test]
    fn monitoring_replaces_the_master_with_the_reference() {
        let mut p = ReferencePlayer::new(48000.0);
        p.set_track(Some(track(48000, 0.5)));
        p.playing = true;
        p.monitor = true;
        p.level_match = false;
        let mut l = vec![0.0f32; 4800];
        let mut r = l.clone();
        p.process(&mut l, &mut r, -14.0);
        let peak = l[480..].iter().fold(0.0f32, |a, b| a.max(b.abs()));
        assert!(peak > 0.4, "{peak}");
    }

    #[test]
    fn level_match_scales_the_reference_to_the_programme() {
        let mut p = ReferencePlayer::new(48000.0);
        p.set_track(Some(track(48000, 0.5))); // reference at -20 LUFS
        p.playing = true;
        let mut l = vec![0.0f32; 480];
        let mut r = l.clone();
        p.process(&mut l, &mut r, -14.0); // learn: programme at -14 LUFS
        p.monitor = true;
        for _ in 0..40 {
            let mut l = vec![0.0f32; 480];
            let mut r = l.clone();
            p.process(&mut l, &mut r, -14.0);
        }
        assert!((p.applied_gain_db() - 6.0).abs() < 0.5, "{}", p.applied_gain_db());
    }
}
