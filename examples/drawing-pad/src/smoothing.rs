//! # Smoothing and shape recognition
//!
//! Multi-stage pipeline that transforms raw mouse input into polished, committed
//! geometry. Raw strokes are noisy (jitter, inconsistent spacing), so we apply
//! a sequence of filters to produce clean, visually pleasing output.
//!
//! The pipeline is applied only to freehand strokes with ≥3 points. Geometric
//! tools (Line, Rect, Circle) use simple start/end anchors and skip smoothing.

use alloc::vec;
use alloc::vec::Vec;

use crate::geometry::Point;

/// The result of shape recognition after smoothing. Determines whether the user's
/// freehand stroke should be rendered as an ideal geometric shape or kept as a
/// smoothed polyline.
pub(crate) enum RecognizedShape {
    /// The stroke is kept as a freehand polyline (no geometric shape detected).
    Freehand,
    /// The stroke closely matches a circle. `center` is the centroid of all points,
    /// `radius` is the mean distance from center to each point.
    Circle { center: Point, radius: f32 },
}

/// The main smoothing pipeline. Takes raw mouse input points and returns a polished
/// version suitable for rendering, plus the recognized shape (freehand or circle).
///
/// Pipeline stages:
///   1. **Dedup** — Remove consecutive duplicate points (distance² < 0.0001).
///   2. **Gaussian** — Weighted average blur with σ=1.0 to smooth jitter.
///   3. **Resample** — Re-space to uniform 3px intervals for consistent processing.
///   4. **Circle detection** — Multi-test heuristic (closing distance, radius variance).
///   5. **Ideal arc** — If circle detected, snap each point to the perfect circle.
///   6. **Douglas-Peucker** — Reduce point count with 2px tolerance.
///   7. **Chaikin** — Corner-cutting subdivision for final smoothness.
pub(crate) fn smooth_pipeline(raw_points: &[Point]) -> (Vec<Point>, RecognizedShape) {
    let deduplicated = remove_consecutive_duplicates(raw_points);
    let smoothed = gaussian_filter(&deduplicated);
    let resampled = resample_uniform(&smoothed, 3.0);
    let recognized = recognize_circle(&resampled);
    let with_ideal_arc = if let RecognizedShape::Circle { center, radius } = recognized {
        // If the stroke is a circle, replace each point's position with the ideal
        // circle point at the same angle — this snaps wobbly hand-drawn circles
        // to perfect circular geometry.
        replace_circle_with_ideal_arc(&resampled, center, radius)
    } else {
        resampled.clone()
    };
    let simplified = simplify_douglas_peucker(&with_ideal_arc, 2.0);
    (apply_chaikin_smoothing(&simplified), recognized)
}

/// Remove consecutive duplicate points that are essentially identical (distance² < 0.0001,
/// i.e. < 0.01px apart). This eliminates redundant samples when the mouse is stationary
/// or moving extremely slowly, which would otherwise waste processing in downstream stages.
///
/// Uses a fold to build the output incrementally, comparing each new point against the
/// last accepted point.
fn remove_consecutive_duplicates(points: &[Point]) -> Vec<Point> {
    points.iter().fold(Vec::new(), |mut acc, &point| {
        if acc.last().is_none_or(|&(last_x, last_y)| {
            // Keep the point only if it's more than 0.01px from the last accepted point.
            // The squared-distance check avoids an expensive sqrt for this tiny threshold.
            (point.0 - last_x).powi(2) + (point.1 - last_y).powi(2) > 0.0001
        }) {
            acc.push(point);
        }
        acc
    })
}

/// Apply a Gaussian (weighted average) filter to smooth jitter in the point sequence.
///
/// Each point is replaced by a Gaussian-weighted average of ALL points in the sequence.
/// The weight for a neighboring point at index `j` when processing index `i` is:
///
///   w(j) = exp(-((i - j)²) / (2σ²))
///
/// With σ=1.0, points at the same index get weight 1.0, adjacent indices get ~0.61,
/// two away get ~0.14, and three+ away are negligible (<0.02). This creates a smooth
/// blur effect that preserves the overall stroke shape while eliminating high-frequency
/// jitter.
///
/// For sequences shorter than 3 points, returns the input unchanged (not enough context
/// to meaningfully filter).
fn gaussian_filter(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    let sigma = 1.0;
    let sigma_sq = sigma * sigma;

    (0..points.len())
        .map(|current_idx| {
            let (weighted_x, weighted_y, weight_sum) = points.iter().enumerate().fold(
                (0.0, 0.0, 0.0),
                |(accum_x, accum_y, accum_weight), (neighbor_idx, &(neighbor_x, neighbor_y))| {
                    // Squared distance between the current index and the neighbor index.
                    // This is index-space distance (not spatial), so it measures how far
                    // apart the samples are in the sequence, not on the canvas.
                    let dist_sq = ((current_idx as i32 - neighbor_idx as i32).pow(2)) as f32;
                    // Gaussian kernel: high weight for nearby indices, rapid falloff for distant ones.
                    let weight = (-dist_sq / (2.0 * sigma_sq)).exp();
                    (
                        accum_x + neighbor_x * weight,
                        accum_y + neighbor_y * weight,
                        accum_weight + weight,
                    )
                },
            );
            // Normalize by total weight to get the weighted average position.
            (weighted_x / weight_sum, weighted_y / weight_sum)
        })
        .collect()
}

/// Compute the total arc length of a polyline by summing the Euclidean distances
/// between each consecutive pair of points. Uses `windows(2)` to iterate over
/// adjacent pairs.
///
/// This is used by `resample_uniform` to determine how many output points are needed
/// and by `path_length` to space them evenly.
fn path_length(points: &[Point]) -> f32 {
    points
        .windows(2)
        .map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt())
        .sum()
}

/// Re-space the point sequence so that consecutive points are approximately
/// `target_spacing` pixels apart (uniform spacing along the path).
///
/// This is important because raw mouse input has inconsistent spacing: fast strokes
/// produce sparse points while slow strokes produce dense clusters. Uniform resampling
/// ensures downstream algorithms (Douglas-Peucker, circle detection) work consistently
/// regardless of drawing speed.
///
/// Algorithm:
///   1. Compute total path length.
///   2. Determine output count = ceil(total_length / target_spacing).
///   3. Walk the source polyline, inserting interpolated points at each target distance.
///   4. Linearly interpolate between the two enclosing source points at each insertion.
///   5. Always include the original last point (prevents truncation).
fn resample_uniform(points: &[Point], target_spacing: f32) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let total_length = path_length(points);
    if total_length <= target_spacing {
        return points.to_vec();
    }
    let output_count = (total_length / target_spacing).ceil() as usize;
    let mut output = Vec::with_capacity(output_count + 1);
    output.push(points[0]);
    let mut accumulated_distance = 0.0;
    let mut source_index = 0;
    for output_index in 1..=output_count {
        let target_distance = output_index as f32 * target_spacing;
        // Walk source segments until we've covered enough distance to place the next
        // output point.
        while source_index < points.len() - 1 {
            let seg_start = points[source_index];
            let seg_end = points[source_index + 1];
            let seg_len =
                ((seg_end.0 - seg_start.0).powi(2) + (seg_end.1 - seg_start.1).powi(2)).sqrt();
            if accumulated_distance + seg_len >= target_distance {
                // Interpolate: t is the fractional position along this segment (0.0–1.0)
                // where the output point should land.
                let t = (target_distance - accumulated_distance) / seg_len;
                output.push((
                    seg_start.0 + (seg_end.0 - seg_start.0) * t,
                    seg_start.1 + (seg_end.1 - seg_start.1) * t,
                ));
                break;
            }
            accumulated_distance += seg_len;
            source_index += 1;
        }
    }
    // Always include the original last point to prevent the polyline from being
    // truncated by floating-point rounding in the loop above.
    if let Some(&last_point) = points.last() {
        if output.last().is_none_or(|&(lx, ly)| {
            (last_point.0 - lx).powi(2) + (last_point.1 - ly).powi(2) > 0.01
        }) {
            output.push(last_point);
        }
    }
    output
}

/// Douglas-Peucker line simplification algorithm. Reduces the number of points in a
/// polyline while preserving its overall shape within a given `tolerance` threshold.
///
/// Algorithm:
///   1. Start with the line between the first and last points.
///   2. Find the point farthest from this line (perpendicular distance).
///   3. If the farthest distance exceeds `tolerance`, keep that point and recurse on
///      both halves (start→farthest, farthest→end).
///   4. If no point exceeds tolerance, discard all intermediate points (the line segment
///      is a good enough approximation).
///
/// The result is a polyline that uses the minimum number of points to stay within
/// `tolerance` of the original shape. A tolerance of 2.0px means the simplified path
/// deviates at most 2 pixels from the original.
fn simplify_douglas_peucker(points: &[Point], tolerance: f32) -> Vec<Point> {
    if points.len() <= 2 {
        return points.to_vec();
    }
    // Boolean flags: which points to keep in the output.
    let mut keep_flags = vec![false; points.len()];
    keep_flags[0] = true;
    keep_flags[points.len() - 1] = true;
    dp_recurse(points, 0, points.len() - 1, tolerance, &mut keep_flags);
    points
        .iter()
        .zip(keep_flags)
        .filter(|(_, k)| *k)
        .map(|(p, _)| *p)
        .collect()
}

/// Recursive helper for Douglas-Peucker simplification. Finds the farthest point
/// from the line segment between `points[start_index]` and `points[end_index]`,
/// and recurses if it exceeds the tolerance.
///
/// Distance calculation uses point-to-line-segment projection:
///   1. Project the point onto the line defined by start→end.
///   2. Clamp the projection parameter `t` to [0, 1] to stay within the segment.
///   3. Compute the distance from the point to its projection on the segment.
///
/// If the segment is degenerate (zero length), falls back to simple point-to-start distance.
fn dp_recurse(
    points: &[Point],
    start_index: usize,
    end_index: usize,
    tolerance: f32,
    keep_flags: &mut [bool],
) {
    if end_index <= start_index + 1 {
        return;
    }
    let (start_x, start_y) = points[start_index];
    let (end_x, end_y) = points[end_index];
    // Direction vector of the segment, and its squared length.
    let (segment_dx, segment_dy) = (end_x - start_x, end_y - start_y);
    let seg_sq = segment_dx * segment_dx + segment_dy * segment_dy;

    // Find the point with maximum perpendicular distance to the segment.
    let farthest = (start_index + 1..end_index)
        .map(|point_idx| {
            let (point_x, point_y) = points[point_idx];
            let distance = if seg_sq <= f32::EPSILON {
                // Degenerate segment (start == end): distance is just point-to-point.
                ((point_x - start_x).powi(2) + (point_y - start_y).powi(2)).sqrt()
            } else {
                // Project the point onto the segment line. `t` is the fractional position
                // along the segment (0.0 = at start, 1.0 = at end). Clamped to [0, 1]
                // so the projection stays within the segment bounds.
                let t = (((point_x - start_x) * segment_dx + (point_y - start_y) * segment_dy)
                    / seg_sq)
                    .clamp(0.0, 1.0);
                // Perpendicular distance from the point to its projection on the segment.
                ((point_x - start_x - t * segment_dx).powi(2)
                    + (point_y - start_y - t * segment_dy).powi(2))
                .sqrt()
            };
            (distance, point_idx)
        })
        .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(core::cmp::Ordering::Equal));

    if let Some((dist, farthest_index)) = farthest {
        if dist > tolerance {
            // The farthest point is outside tolerance — keep it and recurse on both halves.
            keep_flags[farthest_index] = true;
            dp_recurse(points, start_index, farthest_index, tolerance, keep_flags);
            dp_recurse(points, farthest_index, end_index, tolerance, keep_flags);
        }
        // If dist <= tolerance, all intermediate points can be discarded — the straight
        // line from start to end is a good enough approximation.
    }
}

/// Replace a recognized circle's points with ideal circular geometry. For each input
/// point, compute its angle from the circle's center, then place the output point on
/// the ideal circle at that angle.
///
/// This transforms a wobbly hand-drawn circle into a perfect geometric circle while
/// preserving the angular ordering and extent of the original stroke (i.e., if the user
/// drew a 270° arc, the output is a 270° arc on the ideal circle, not a full circle).
///
/// Requires at least 8 points to operate; returns the input unchanged for shorter strokes.
fn replace_circle_with_ideal_arc(points: &[Point], center: Point, radius: f32) -> Vec<Point> {
    if points.len() < 8 {
        return points.to_vec();
    }

    let mut arc_points = Vec::with_capacity(points.len());
    for point in points {
        // Compute the angle of this point relative to the circle center using atan2.
        // atan2 returns the angle in radians from the +X axis, ranging from -π to +π.
        let angle = (point.1 - center.1).atan2(point.0 - center.0);
        // Place the point on the ideal circle at this angle.
        arc_points.push((
            center.0 + radius * angle.cos(),
            center.1 + radius * angle.sin(),
        ));
    }
    arc_points
}

/// Attempt to recognize whether the given polyline approximates a circle.
///
/// Uses four heuristic tests, applied as early-exit gates:
///
///   1. **Closing distance** — The distance between the first and last points must be
///      less than 25% of the bounding box diagonal. A circle should roughly close on
///      itself; an open stroke (like a spiral or arc) fails this check.
///
///   2. **Minimum radius** — The mean radius must be > 5px. Very small "circles" are
///      just jitter and should be treated as freehand.
///
///   3. **Coefficient of variation** — The standard deviation of radii divided by the
///      mean radius must be < 8%. A perfect circle has all radii equal (CV = 0);
///      real hand-drawn circles have slight variation.
///
///   4. **Maximum deviation** — No single point's radius can differ from the mean by
///      more than 12%. This catches strokes that are mostly circular but have one
///      outlier section (like a circle with a tail).
///
/// If all tests pass, returns `RecognizedShape::Circle` with the computed center and
/// mean radius. Otherwise returns `RecognizedShape::Freehand`.
fn recognize_circle(points: &[Point]) -> RecognizedShape {
    if points.len() < 8 {
        return RecognizedShape::Freehand;
    }
    // Test 1: Closing distance — how close is the end point to the start point?
    let (first_point, last_point) = (points[0], *points.last().unwrap());
    let closing_distance =
        ((last_point.0 - first_point.0).powi(2) + (last_point.1 - first_point.1).powi(2)).sqrt();

    // Compute the bounding box of all points to get a scale reference.
    let (min_x, max_x) = points
        .iter()
        .fold((f32::MAX, f32::MIN), |(curr_min, curr_max), point| {
            (curr_min.min(point.0), curr_max.max(point.0))
        });
    let (min_y, max_y) = points
        .iter()
        .fold((f32::MAX, f32::MIN), |(curr_min, curr_max), point| {
            (curr_min.min(point.1), curr_max.max(point.1))
        });
    let bbox_diag = ((max_x - min_x).powi(2) + (max_y - min_y).powi(2)).sqrt();
    if closing_distance > bbox_diag * 0.25 {
        // The stroke doesn't close on itself — not a circle.
        return RecognizedShape::Freehand;
    }

    // Compute the centroid (arithmetic mean of all points) as the candidate center.
    let point_count = points.len() as f32;
    let center_x = points.iter().map(|point| point.0).sum::<f32>() / point_count;
    let center_y = points.iter().map(|point| point.1).sum::<f32>() / point_count;

    // Compute the radius of each point from the centroid.
    let radii: Vec<f32> = points
        .iter()
        .map(|point| ((point.0 - center_x).powi(2) + (point.1 - center_y).powi(2)).sqrt())
        .collect();
    let mean_radius = radii.iter().sum::<f32>() / point_count;

    // Test 2: Minimum radius — too small to be a meaningful circle.
    if mean_radius < 5.0 {
        return RecognizedShape::Freehand;
    }

    // Test 3: Coefficient of variation (stddev / mean) — measures how consistent
    // the radii are. A perfect circle has CV = 0; real circles typically have CV < 0.08.
    let stddev = (radii
        .iter()
        .map(|radius_val| (radius_val - mean_radius).powi(2))
        .sum::<f32>()
        / point_count)
        .sqrt();
    if stddev / mean_radius > 0.08 {
        return RecognizedShape::Freehand;
    }

    // Test 4: Maximum single-point deviation — catches outlier points that the
    // stddev test might miss (e.g., one bad point in a long stroke).
    let max_deviation = radii
        .iter()
        .map(|radius_val| (radius_val - mean_radius).abs())
        .fold(0.0f32, f32::max);
    if max_deviation > mean_radius * 0.12 {
        return RecognizedShape::Freehand;
    }

    RecognizedShape::Circle {
        center: (center_x, center_y),
        radius: mean_radius,
    }
}

/// Apply Chaikin's corner-cutting algorithm for one iteration of curve subdivision.
///
/// For each pair of consecutive points (A, B), two new points are generated:
///   - P₁ = 0.75·A + 0.25·B  (quarter-point, closer to A)
///   - P₂ = 0.25·A + 0.75·B  (three-quarter point, closer to B)
///
/// The original points A and B are removed (except the first and last endpoints,
/// which are preserved). This effectively "cuts" every sharp corner, producing a
/// smoother curve that approximates the original polyline.
///
/// One iteration approximately doubles the point count. The visual effect is a
/// gentle rounding of corners — ideal for the final pass after Douglas-Peucker
/// simplification, which can leave sharp angles.
///
/// Example: A polyline with points [P0, P1, P2, P3] becomes:
///   [P0, ¾P0+¼P1, ¼P0+¾P1, ¾P1+¼P2, ¼P1+¾P2, ¾P2+¼P3, ¼P2+¾P3, P3]
fn apply_chaikin_smoothing(points: &[Point]) -> Vec<Point> {
    if points.len() < 3 {
        return points.to_vec();
    }
    // Start with the first endpoint (preserved).
    let mut result = vec![points[0]];
    // For each edge, emit the two Chaikin subdivision points.
    result.extend(points.windows(2).flat_map(|window| {
        let (point_a, point_b) = (window[0], window[1]);
        [
            (
                point_a.0 * 0.75 + point_b.0 * 0.25,
                point_a.1 * 0.75 + point_b.1 * 0.25,
            ),
            (
                point_a.0 * 0.25 + point_b.0 * 0.75,
                point_a.1 * 0.25 + point_b.1 * 0.75,
            ),
        ]
    }));
    // End with the last endpoint (preserved).
    result.push(*points.last().unwrap());
    result
}
