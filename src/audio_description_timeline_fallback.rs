//! Isolated timestamp fallbacks from Sonarpad Windows 0.9.12/0.9.13.
//!
//! Call these only after the existing timeline and <= 2% drift paths fail.
//! The caller must additionally gate the segment-schedule fallback on an original
//! measured excess strictly above 2% and at most 10%. Successful ordinary media
//! never enters these helpers. The first helper delegates to the unchanged host
//! builder, preserving that host's normal and historical drift behavior.

use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(super) struct RepeatedStartFallback<T> {
    pub(super) timeline: T,
    pub(super) adjusted_count: usize,
    pub(super) median_start_sec: f64,
}

#[derive(Debug)]
pub(super) struct SegmentScheduleFallback<T> {
    pub(super) timeline: Vec<T>,
    pub(super) replaced_count: usize,
    pub(super) plausible_non_last: usize,
    pub(super) repaired_total_sec: f64,
}

pub(super) fn repeated_small_start_offset<T, B>(
    raw_chunks: &[(PathBuf, f64, f64)],
    duration_sec: f64,
    segment_seconds: f64,
    mut build_timeline: B,
) -> Option<RepeatedStartFallback<T>>
where
    B: FnMut(&[(PathBuf, f64)], f64, bool) -> Option<T>,
{
    if raw_chunks.len() < 2
        || !duration_sec.is_finite()
        || duration_sec <= 0.0
        || !segment_seconds.is_finite()
    {
        return None;
    }

    let expected_sec = segment_seconds.max(1.0);
    let max_small_start_sec = (expected_sec * 0.20).min(10.0);
    let mut candidates = raw_chunks
        .iter()
        .enumerate()
        .filter_map(|(index, (_, raw_duration, start_time))| {
            if !raw_duration.is_finite()
                || *raw_duration <= 0.0
                || !start_time.is_finite()
                || *start_time < 0.250
                || *start_time > max_small_start_sec
                || *raw_duration <= *start_time + 0.001
            {
                return None;
            }
            let local_duration = *raw_duration - *start_time;
            if !local_duration.is_finite() || local_duration <= 0.001 {
                return None;
            }
            let improves_expected = index + 1 == raw_chunks.len()
                || (local_duration - expected_sec).abs() + 0.050
                    < (*raw_duration - expected_sec).abs();
            improves_expected.then_some(*start_time)
        })
        .collect::<Vec<_>>();

    let minimum_candidates = (raw_chunks.len() * 7).div_ceil(10);
    if candidates.len() < minimum_candidates.max(2) {
        return None;
    }

    candidates.sort_by(|a, b| a.total_cmp(b));
    let median_start = candidates[candidates.len() / 2];
    let tolerance_sec = (median_start * 0.20).clamp(0.250, 1.0);
    let mut adjusted_count = 0usize;
    let mut adjusted_chunks = Vec::with_capacity(raw_chunks.len());

    for (index, (path, raw_duration, start_time)) in raw_chunks.iter().enumerate() {
        let mut duration = *raw_duration;
        if start_time.is_finite()
            && (*start_time - median_start).abs() <= tolerance_sec
            && *raw_duration > *start_time + 0.001
        {
            let local_duration = *raw_duration - *start_time;
            let improves_expected = index + 1 == raw_chunks.len()
                || (local_duration - expected_sec).abs() + 0.050
                    < (*raw_duration - expected_sec).abs();
            if improves_expected && local_duration <= expected_sec * 2.0 {
                duration = local_duration;
                adjusted_count = adjusted_count.saturating_add(1);
            }
        }
        adjusted_chunks.push((path.clone(), duration));
    }

    if adjusted_count < minimum_candidates.max(2) {
        return None;
    }

    let raw_total = raw_chunks
        .iter()
        .map(|(_, duration, _)| *duration)
        .sum::<f64>();
    let adjusted_total = adjusted_chunks
        .iter()
        .map(|(_, duration)| *duration)
        .sum::<f64>();
    if !raw_total.is_finite()
        || !adjusted_total.is_finite()
        || adjusted_total <= 0.0
        || adjusted_total >= raw_total
    {
        return None;
    }
    let correction_ratio = (raw_total - adjusted_total) / raw_total;
    if !correction_ratio.is_finite() || correction_ratio <= 0.0 || correction_ratio > 0.10 {
        return None;
    }

    if let Some(timeline) = build_timeline(&adjusted_chunks, duration_sec, false) {
        return Some(RepeatedStartFallback {
            timeline,
            adjusted_count,
            median_start_sec: median_start,
        });
    }
    build_timeline(&adjusted_chunks, duration_sec, true).map(|timeline| RepeatedStartFallback {
        timeline,
        adjusted_count,
        median_start_sec: median_start,
    })
}

pub(super) fn segment_schedule_fallback<T, C>(
    raw_chunks: &[(PathBuf, f64, f64)],
    duration_sec: f64,
    segment_seconds: f64,
    mut make_chunk: C,
) -> Option<SegmentScheduleFallback<T>>
where
    C: FnMut(&Path, f64, f64) -> T,
{
    if raw_chunks.len() < 2
        || !duration_sec.is_finite()
        || duration_sec <= 0.0
        || !segment_seconds.is_finite()
    {
        return None;
    }

    let expected_sec = segment_seconds.max(1.0);
    let expected_count = (duration_sec / expected_sec).ceil() as usize;
    if expected_count != raw_chunks.len() {
        return None;
    }

    let last_nominal_sec = duration_sec - expected_sec * (raw_chunks.len() - 1) as f64;
    if !last_nominal_sec.is_finite()
        || last_nominal_sec <= 0.001
        || last_nominal_sec > expected_sec * 1.25
    {
        return None;
    }

    // FFmpeg's segment muxer can cut a little before/after the requested boundary
    // because of keyframes. Preserve those real variations. Broadcast H.264 normally
    // has frequent keyframes, so only durations more than about 8% away from the known
    // segment schedule are treated as suspect metadata. This fallback is used only
    // after the historical timeline paths failed.
    let normal_tolerance_sec = (expected_sec * 0.08).clamp(2.0, 8.0);
    let non_last_count = raw_chunks.len() - 1;
    let plausible_non_last = raw_chunks[..non_last_count]
        .iter()
        .filter(|chunk| {
            let duration = chunk.1;
            duration.is_finite()
                && duration > 0.001
                && (duration - expected_sec).abs() <= normal_tolerance_sec
        })
        .count();
    let minimum_plausible = (non_last_count * 6).div_ceil(10);
    if plausible_non_last < minimum_plausible.max(1) {
        return None;
    }

    // reset_timestamps=1 should make prepared analysis chunks local/zero-based. A
    // small muxer rounding offset is fine, but if most chunks retain a real source
    // clock this is not the pattern this fallback is designed to repair.
    let near_zero_start_count = raw_chunks
        .iter()
        .filter(|chunk| {
            let start_time = chunk.2;
            start_time.is_finite() && start_time.abs() <= 0.500
        })
        .count();
    let minimum_zero_starts = (raw_chunks.len() * 8).div_ceil(10);
    if near_zero_start_count < minimum_zero_starts.max(2) {
        return None;
    }

    let mut replaced_count = 0usize;
    let mut repaired = Vec::with_capacity(raw_chunks.len());
    for (index, (path, raw_duration, _)) in raw_chunks.iter().enumerate() {
        let nominal = if index + 1 == raw_chunks.len() {
            last_nominal_sec
        } else {
            expected_sec
        };
        let tolerance = if index + 1 == raw_chunks.len() {
            (last_nominal_sec * 0.75).clamp(2.0, 8.0)
        } else {
            normal_tolerance_sec
        };
        let duration = if raw_duration.is_finite()
            && *raw_duration > 0.001
            && (*raw_duration - nominal).abs() <= tolerance
        {
            *raw_duration
        } else {
            replaced_count = replaced_count.saturating_add(1);
            nominal
        };
        repaired.push((path.clone(), duration));
    }

    if replaced_count * 10 > raw_chunks.len() * 4 {
        return None;
    }
    if replaced_count == 0 && plausible_non_last * 10 < non_last_count * 8 {
        return None;
    }

    let repaired_total = repaired.iter().map(|(_, duration)| *duration).sum::<f64>();
    if !repaired_total.is_finite() || repaired_total <= 0.0 {
        return None;
    }
    let residual_ratio = (repaired_total - duration_sec).abs() / duration_sec;
    if !residual_ratio.is_finite() || residual_ratio > 0.06 {
        return None;
    }

    let scale = duration_sec / repaired_total;
    if !scale.is_finite() || !(0.94..=1.06).contains(&scale) {
        return None;
    }

    let mut timeline = Vec::with_capacity(repaired.len());
    let mut cursor = 0.0_f64;
    for (index, (path, duration)) in repaired.iter().enumerate() {
        let start_sec = cursor;
        let end_sec = if index + 1 == repaired.len() {
            duration_sec
        } else {
            (start_sec + duration * scale).min(duration_sec)
        };
        if !end_sec.is_finite() || end_sec <= start_sec {
            return None;
        }
        timeline.push(make_chunk(path, start_sec, end_sec));
        cursor = end_sec;
    }

    Some(SegmentScheduleFallback {
        timeline,
        replaced_count,
        plausible_non_last,
        repaired_total_sec: repaired_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(values: &[(f64, f64)]) -> Vec<(PathBuf, f64, f64)> {
        values
            .iter()
            .enumerate()
            .map(|(index, (duration, start))| {
                (
                    PathBuf::from(format!("chunk{}.mkv", index + 1)),
                    *duration,
                    *start,
                )
            })
            .collect()
    }

    fn tuple_chunk(path: &Path, start: f64, end: f64) -> (PathBuf, f64, f64) {
        (path.to_path_buf(), start, end)
    }

    #[test]
    fn repeated_offset_repairs_live_recording_and_keeps_normal_builder_first() {
        let values = raw(&[(86.88, 3.88), (86.88, 3.88), (86.88, 3.88), (35.88, 3.88)]);
        let mut calls = Vec::new();
        let result =
            repeated_small_start_offset(&values, 284.0, 83.0, |adjusted, duration, drift| {
                calls.push(drift);
                assert_eq!(duration, 284.0);
                Some(adjusted.to_vec())
            })
            .expect("repeated small timestamp origin is repairable");
        assert_eq!(calls, [false]);
        assert_eq!(result.adjusted_count, 4);
        assert!((result.median_start_sec - 3.88).abs() < 0.001);
        let durations: Vec<_> = result.timeline.iter().map(|chunk| chunk.1).collect();
        for (actual, expected) in durations.iter().zip([83.0, 83.0, 83.0, 32.0]) {
            assert!((actual - expected).abs() < 0.001);
        }
        assert_eq!(result.timeline[0].0, values[0].0);
    }

    #[test]
    fn repeated_offset_delegates_to_existing_drift_only_after_builder_failure() {
        let values = raw(&[(86.88, 3.88), (86.88, 3.88), (86.88, 3.88), (35.88, 3.88)]);
        let mut calls = Vec::new();
        let result = repeated_small_start_offset(&values, 284.0, 83.0, |_, _, drift| {
            calls.push(drift);
            drift.then_some("existing host drift result")
        })
        .unwrap();
        assert_eq!(calls, [false, true]);
        assert_eq!(result.timeline, "existing host drift result");
    }

    #[test]
    fn repeated_offset_preserves_host_rejection() {
        let values = raw(&[(86.88, 3.88), (86.88, 3.88), (86.88, 3.88), (35.88, 3.88)]);
        let mut calls = Vec::new();
        let result = repeated_small_start_offset::<(), _>(&values, 100.0, 83.0, |_, _, drift| {
            calls.push(drift);
            None
        });
        assert!(result.is_none());
        assert_eq!(calls, [false, true]);
    }

    #[test]
    fn repeated_offset_rejects_inconsistent_or_large_origins() {
        for values in [
            raw(&[(86.88, 3.88), (84.20, 1.20), (90.0, 7.0), (35.88, 3.88)]),
            raw(&[(103.0, 20.0), (103.0, 20.0), (103.0, 20.0)]),
        ] {
            assert!(
                repeated_small_start_offset::<(), _>(&values, 284.0, 83.0, |_, _, _| panic!(
                    "unsafe timestamp pattern reached the builder"
                ))
                .is_none()
            );
        }
    }

    #[test]
    fn ordinary_zero_based_timings_do_not_activate_repeated_offset() {
        let values = raw(&[(83.0, 0.0), (83.0, 0.0), (83.0, 0.0), (32.0, 0.0)]);
        assert!(
            repeated_small_start_offset::<(), _>(&values, 281.0, 83.0, |_, _, _| panic!(
                "ordinary timing must not be adjusted"
            ))
            .is_none()
        );
    }

    #[test]
    fn repeated_offset_rejects_more_than_ten_percent_correction() {
        let values = raw(&[(23.0, 3.0), (23.0, 3.0), (23.0, 3.0)]);
        assert!(
            repeated_small_start_offset::<(), _>(&values, 60.0, 20.0, |_, _, _| panic!(
                "excessive correction reached the builder"
            ))
            .is_none()
        );
    }

    #[test]
    fn schedule_repairs_sparse_bad_metadata_preserving_real_duration_ratios() {
        let duration = 5_402.795;
        let values = (0..66)
            .map(|index| {
                let measured = if index == 65 {
                    7.795
                } else if matches!(index, 19 | 39 | 59) {
                    167.0
                } else if index % 4 == 0 {
                    84.52
                } else {
                    82.56
                };
                (
                    PathBuf::from(format!("chunk{index:04}.mkv")),
                    measured,
                    if index == 0 { 0.040 } else { 0.0 },
                )
            })
            .collect::<Vec<_>>();
        let result = segment_schedule_fallback(&values, duration, 83.0, tuple_chunk).unwrap();
        assert_eq!(result.timeline.len(), 66);
        assert_eq!(result.replaced_count, 3);
        assert!(result.plausible_non_last >= 62);
        assert!((result.repaired_total_sec - duration).abs() / duration < 0.02);
        assert_eq!(result.timeline.last().unwrap().2, duration);
        let first_duration = result.timeline[0].2 - result.timeline[0].1;
        let second_duration = result.timeline[1].2 - result.timeline[1].1;
        assert!((first_duration / second_duration - 84.52 / 82.56).abs() < 1e-9);
        for pair in result.timeline.windows(2) {
            assert_eq!(pair[0].2, pair[1].1);
        }
    }

    #[test]
    fn schedule_handles_bounded_global_metadata_bias() {
        let duration = 5_402.795;
        let values = (0..66)
            .map(|index| {
                let nominal = if index == 65 { 7.795 } else { 83.0 };
                (
                    PathBuf::from(format!("chunk{index:04}.mkv")),
                    nominal * 1.04663,
                    if index == 0 { 0.040 } else { 0.0 },
                )
            })
            .collect::<Vec<_>>();
        let result = segment_schedule_fallback(&values, duration, 83.0, tuple_chunk).unwrap();
        assert_eq!(result.replaced_count, 0);
        assert_eq!(result.plausible_non_last, 65);
        assert!((result.repaired_total_sec - 5_654.72).abs() < 1.0);
        assert_eq!(result.timeline.last().unwrap().2, duration);
    }

    #[test]
    fn schedule_rejects_wrong_chunk_count() {
        let values = raw(&[(83.0, 0.0), (83.0, 0.0), (20.0, 0.0)]);
        assert!(segment_schedule_fallback(&values, 250.0, 83.0, tuple_chunk).is_none());
    }

    #[test]
    fn schedule_rejects_widespread_metadata_damage() {
        let values = (0..10)
            .map(|index| {
                (
                    PathBuf::from(format!("chunk{index}.mkv")),
                    if index < 6 { 130.0 } else { 83.0 },
                    0.0,
                )
            })
            .collect::<Vec<_>>();
        assert!(segment_schedule_fallback(&values, 830.0, 83.0, tuple_chunk).is_none());
    }

    #[test]
    fn schedule_rejects_nonlocal_source_clock() {
        let values = raw(&[(86.0, 1.0), (86.0, 84.0), (86.0, 167.0)]);
        assert!(segment_schedule_fallback(&values, 249.0, 83.0, tuple_chunk).is_none());
    }

    #[test]
    fn schedule_rejects_more_than_six_percent_residual_adjustment() {
        let values = raw(&[(88.81, 0.0), (88.81, 0.0), (88.81, 0.0)]);
        assert!(segment_schedule_fallback(&values, 249.0, 83.0, tuple_chunk).is_none());
    }

    #[test]
    fn nonfinite_duration_or_segment_size_never_produces_a_timeline() {
        let values = raw(&[(86.88, 3.88), (86.88, 3.88), (86.88, 3.88)]);
        for (duration, segment) in [
            (f64::NAN, 83.0),
            (f64::INFINITY, 83.0),
            (249.0, f64::NAN),
            (249.0, f64::INFINITY),
            (0.0, 83.0),
        ] {
            assert!(
                repeated_small_start_offset::<(), _>(&values, duration, segment, |_, _, _| panic!(
                    "nonfinite inputs reached the host builder"
                ))
                .is_none()
            );
            assert!(segment_schedule_fallback(&values, duration, segment, tuple_chunk).is_none());
        }
    }
}
