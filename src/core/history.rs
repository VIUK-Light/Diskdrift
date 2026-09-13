//! Storage history: per-day totals and deltas derived from stored snapshots.

use crate::core::snapshot::{CatVal, SnapshotMeta};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct HistoryDay {
    pub date: String,
    pub snapshot_id: i64,
    pub snapshot_count: usize,
    pub allocated: u64,
    pub logical: u64,
    /// Change in allocated bytes compared with the previous day.
    pub change: Option<i64>,
}

/// Group snapshots by local date, keep the last snapshot of each day and
/// compute day-over-day changes. `values` supplies per-snapshot category
/// values for `history <category>`; when `None`, snapshot totals are used.
pub fn build_days(
    snapshots: &[SnapshotMeta],
    values: Option<&HashMap<i64, CatVal>>,
) -> Vec<HistoryDay> {
    let mut by_date: Vec<(String, Vec<&SnapshotMeta>)> = Vec::new();
    for meta in snapshots {
        let date = meta.created_at_local.get(0..10).unwrap_or("").to_string();
        match by_date.last_mut() {
            Some((d, list)) if *d == date => list.push(meta),
            _ => by_date.push((date, vec![meta])),
        }
    }

    let mut days: Vec<HistoryDay> = by_date
        .into_iter()
        .map(|(date, list)| {
            let last = list.last().expect("non-empty day group");
            let (logical, allocated) = match values {
                Some(map) => map
                    .get(&last.id)
                    .map(|v| (v.logical, v.allocated))
                    .unwrap_or((0, 0)),
                None => (last.total_logical_bytes, last.total_allocated_bytes),
            };
            HistoryDay {
                date,
                snapshot_id: last.id,
                snapshot_count: list.len(),
                allocated,
                logical,
                change: None,
            }
        })
        .collect();

    let mut previous: Option<u64> = None;
    for day in &mut days {
        day.change = previous.map(|p| day.allocated as i64 - p as i64);
        previous = Some(day.allocated);
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    fn meta(id: i64, local: &str, allocated: u64) -> SnapshotMeta {
        SnapshotMeta {
            id,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            created_at_local: local.to_string(),
            total_logical_bytes: allocated,
            total_allocated_bytes: allocated,
            file_count: 0,
            directory_count: 0,
            symlink_count: 0,
            skipped_count: 0,
            duration_ms: 0,
            app_version: "test".to_string(),
        }
    }

    #[test]
    fn groups_by_day_and_computes_deltas() {
        let snapshots = vec![
            meta(1, "2026-09-10T09:00:00+09:00", 100),
            meta(2, "2026-09-10T18:00:00+09:00", 120), // same day: last wins
            meta(3, "2026-09-11T09:00:00+09:00", 150),
            meta(4, "2026-09-12T09:00:00+09:00", 140),
        ];
        let days = build_days(&snapshots, None);
        assert_eq!(days.len(), 3);
        assert_eq!(days[0].date, "2026-09-10");
        assert_eq!(days[0].snapshot_id, 2);
        assert_eq!(days[0].snapshot_count, 2);
        assert_eq!(days[0].change, None);
        assert_eq!(days[1].change, Some(30));
        assert_eq!(days[2].change, Some(-10));
    }

    #[test]
    fn category_values_override_totals() {
        let snapshots = vec![
            meta(1, "2026-09-10T09:00:00+09:00", 100),
            meta(2, "2026-09-11T09:00:00+09:00", 150),
        ];
        let mut values = HashMap::new();
        values.insert(
            1,
            CatVal {
                allocated: 40,
                ..Default::default()
            },
        );
        values.insert(
            2,
            CatVal {
                allocated: 55,
                ..Default::default()
            },
        );
        let days = build_days(&snapshots, Some(&values));
        assert_eq!(days[0].allocated, 40);
        assert_eq!(days[1].allocated, 55);
        assert_eq!(days[1].change, Some(15));
    }
}
