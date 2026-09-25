use crate::locate::{PointPx, RegionAnchorKind, RegionHint, ScreenInfo};

// ── Region Crop ──

/// Maps a [0,1] relative point from a cropped region back to full-screen pixel coordinates.
pub fn uncrop_point(rel_x: f32, rel_y: f32, region: &RegionHint, screen: &ScreenInfo) -> PointPx {
    let px = (region.x + rel_x * region.width) * screen.logical_width as f32;
    let py = (region.y + rel_y * region.height) * screen.logical_height as f32;
    PointPx {
        x: px as i32,
        y: py as i32,
    }
}

/// Generate a RegionHint from a known anchor kind.
pub fn anchor_region(kind: RegionAnchorKind, _screen: &ScreenInfo) -> RegionHint {
    let (rx, ry, rw, rh) = match kind {
        RegionAnchorKind::TaskbarBottom => (0.0, 0.88, 1.0, 0.12),
        RegionAnchorKind::TaskbarLeft => (0.0, 0.0, 0.06, 1.0),
        RegionAnchorKind::TaskbarRight => (0.94, 0.0, 0.06, 1.0),
        RegionAnchorKind::TaskbarTop => (0.0, 0.0, 1.0, 0.05),
        RegionAnchorKind::SystemTray => (0.78, 0.90, 0.22, 0.10),
        RegionAnchorKind::ActiveWindowTitleBar => (0.0, 0.0, 1.0, 0.06),
        RegionAnchorKind::ActiveWindowClientArea => (0.0, 0.06, 1.0, 0.82),
        RegionAnchorKind::DesktopFull => (0.0, 0.0, 1.0, 1.0),
    };
    RegionHint {
        x: rx,
        y: ry,
        width: rw,
        height: rh,
        anchor_kind: Some(kind),
    }
}

// ── DBSCAN Clustering (simplified) ──

/// Simple density-based clustering: groups points within `eps` pixels.
/// Returns the largest cluster's centroid, or None if no cluster has >= min_samples.
pub fn cluster_median(points: &[PointPx], eps: f64, min_samples: usize) -> Option<PointPx> {
    if points.len() < min_samples {
        return None;
    }
    let n = points.len();
    let mut visited = vec![false; n];
    let mut best_cluster: Vec<usize> = Vec::new();

    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;
        let mut cluster = vec![i];
        let mut frontier = vec![i];
        while let Some(current) = frontier.pop() {
            for j in 0..n {
                if visited[j] {
                    continue;
                }
                let dx = (points[current].x - points[j].x) as f64;
                let dy = (points[current].y - points[j].y) as f64;
                if (dx * dx + dy * dy).sqrt() <= eps {
                    visited[j] = true;
                    cluster.push(j);
                    frontier.push(j);
                }
            }
        }
        if cluster.len() > best_cluster.len() {
            best_cluster = cluster;
        }
    }
    if best_cluster.len() < min_samples {
        return None;
    }
    let sum_x: i32 = best_cluster.iter().map(|&i| points[i].x).sum();
    let sum_y: i32 = best_cluster.iter().map(|&i| points[i].y).sum();
    let count = best_cluster.len() as i32;
    Some(PointPx {
        x: sum_x / count,
        y: sum_y / count,
    })
}

// ── Prompt Variants ──

pub fn prompt_variants(target: &str) -> Vec<String> {
    vec![
        format!("{target}"),
        format!("Find the {target} on screen"),
        format!("{target} (look carefully)"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn region_taskbar_bottom_is_correct() {
        let screen = ScreenInfo {
            logical_width: 1707,
            logical_height: 960,
            dpi_scale: 1.5,
        };
        let r = anchor_region(RegionAnchorKind::TaskbarBottom, &screen);
        assert_eq!(r.y, 0.88);
        assert_eq!(r.height, 0.12);
    }

    #[test]
    fn uncrop_center_maps_correctly() {
        let screen = ScreenInfo {
            logical_width: 1000,
            logical_height: 1000,
            dpi_scale: 1.0,
        };
        let region = RegionHint {
            x: 0.0,
            y: 0.9,
            width: 1.0,
            height: 0.1,
            anchor_kind: Some(RegionAnchorKind::TaskbarBottom),
        };
        // Center of taskbar region at (0.5, 0.5) relative → full screen (500, 950)
        let p = uncrop_point(0.5, 0.5, &region, &screen);
        assert_eq!(p.x, 500);
        assert_eq!(p.y, 950);
    }

    #[test]
    fn cluster_rejects_outliers() {
        let points = vec![
            PointPx { x: 600, y: 170 },
            PointPx { x: 614, y: 173 },
            PointPx { x: 1552, y: 882 }, // outlier
        ];
        let result = cluster_median(&points, 50.0, 2);
        assert!(result.is_some());
        let p = result.unwrap();
        // Should be near (607, 171) - centroid of first two points
        assert!((p.x - 607).abs() <= 10);
        assert!((p.y - 171).abs() <= 10);
    }

    #[test]
    fn cluster_returns_none_for_all_outliers() {
        let points = vec![
            PointPx { x: 0, y: 0 },
            PointPx { x: 1000, y: 0 },
            PointPx { x: 0, y: 1000 },
        ];
        let result = cluster_median(&points, 10.0, 2);
        assert!(result.is_none());
    }

    #[test]
    fn cluster_single_group_returns_centroid() {
        let points = vec![
            PointPx { x: 100, y: 100 },
            PointPx { x: 102, y: 100 },
            PointPx { x: 101, y: 102 },
        ];
        let result = cluster_median(&points, 20.0, 2);
        assert!(result.is_some());
        let p = result.unwrap();
        assert_eq!(p.x, 101);
        assert_eq!(p.y, 100);
    }
}
