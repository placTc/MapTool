//! Linking shared border chains into per-province rings and writing SVG path data.

use std::fmt::Write;

use crate::label::{Labels, NONE};
use crate::smooth::{Curve, P};
use crate::trace::V;
use crate::{Options, smooth, trace};

struct Chain {
    left: u32,
    right: u32,
    start: V,
    end: V,
    /// Direction of the first / last crack edge, walking forward.
    first_dir: V,
    last_dir: V,
    curve: Curve,
}

#[derive(Clone, Copy)]
struct Arc {
    chain: u32,
    rev: bool,
}

struct ArcInfo {
    start: V,
    end: V,
    /// Direction leaving the start / arriving at the end, in walking order.
    out_dir: V,
    in_dir: V,
}

fn info(c: &Chain, rev: bool) -> ArcInfo {
    let neg = |d: V| (-d.0, -d.1);
    if rev {
        ArcInfo { start: c.end, end: c.start, out_dir: neg(c.last_dir), in_dir: neg(c.first_dir) }
    } else {
        ArcInfo { start: c.start, end: c.end, out_dir: c.first_dir, in_dir: c.last_dir }
    }
}

/// Signed turn from `d_in` to `d_out`: larger means a sharper left turn
/// (screen coordinates, y down).
fn turn(d_in: V, d_out: V) -> f64 {
    let left = (d_out.0 * d_in.1 - d_out.1 * d_in.0) as f64;
    let fwd = (d_out.0 * d_in.0 + d_out.1 * d_in.1) as f64;
    left.atan2(fwd)
}

fn push_num(s: &mut String, v: f64, prec: usize) {
    let mark = s.len();
    let _ = write!(s, "{v:.prec$}");
    if s[mark..].contains('.') {
        let trimmed = s.trim_end_matches('0').trim_end_matches('.').len();
        s.truncate(trimmed);
    }
    if &s[mark..] == "-0" {
        s.truncate(mark);
        s.push('0');
    }
}

fn push_pt(s: &mut String, pt: P, prec: usize) {
    push_num(s, pt.0, prec);
    s.push(' ');
    push_num(s, pt.1, prec);
}

fn write_arc(s: &mut String, curve: &Curve, rev: bool, prec: usize) {
    for (c1, c2, to, line) in curve.walk(rev) {
        if line {
            s.push('L');
        } else {
            s.push('C');
            push_pt(s, c1, prec);
            s.push(' ');
            push_pt(s, c2, prec);
            s.push(' ');
        }
        push_pt(s, to, prec);
    }
}

/// Returns the SVG path data of every province, indexed by province id.
pub fn build_paths(labels: &Labels, opts: &Options) -> Vec<String> {
    let mut chains: Vec<Chain> = Vec::new();
    trace::trace_chains(labels, |raw| {
        let n = raw.pts.len();
        let curve = smooth::build_curve(&raw.pts, raw.closed, opts);
        chains.push(Chain {
            left: raw.left,
            right: raw.right,
            start: raw.pts[0],
            end: raw.pts[n - 1],
            first_dir: (raw.pts[1].0 - raw.pts[0].0, raw.pts[1].1 - raw.pts[0].1),
            last_dir: (raw.pts[n - 1].0 - raw.pts[n - 2].0, raw.pts[n - 1].1 - raw.pts[n - 2].1),
            curve,
        });
    });

    // Every chain is drawn by both neighbours, walked in opposite directions.
    let mut arcs: Vec<Vec<Arc>> = vec![Vec::new(); labels.colors.len()];
    for (i, c) in chains.iter().enumerate() {
        if c.left != NONE {
            arcs[c.left as usize].push(Arc { chain: i as u32, rev: false });
        }
        if c.right != NONE {
            arcs[c.right as usize].push(Arc { chain: i as u32, rev: true });
        }
    }

    arcs.iter().map(|list| ring_path(list, &chains, opts.precision)).collect()
}

fn ring_path(list: &[Arc], chains: &[Chain], prec: usize) -> String {
    let infos: Vec<ArcInfo> = list.iter().map(|a| info(&chains[a.chain as usize], a.rev)).collect();
    let mut by_start: Vec<(V, usize)> = infos.iter().enumerate().map(|(i, a)| (a.start, i)).collect();
    by_start.sort_unstable();
    let mut used = vec![false; list.len()];
    let mut out = String::new();

    for first in 0..list.len() {
        if used[first] {
            continue;
        }
        let a = list[first];
        push_move(&mut out, chains[a.chain as usize].curve.begin(a.rev), prec);
        let mut cur = first;
        loop {
            used[cur] = true;
            let arc = list[cur];
            write_arc(&mut out, &chains[arc.chain as usize].curve, arc.rev, prec);

            // Continue with the arc leaving this vertex that turns left the
            // most; that keeps pieces touching only at a point separate.
            let end = infos[cur].end;
            let in_dir = infos[cur].in_dir;
            let lo = by_start.partition_point(|&(v, _)| v < end);
            let next = by_start[lo..]
                .iter()
                .take_while(|&&(v, _)| v == end)
                .map(|&(_, j)| j)
                .filter(|&j| !used[j] || j == first)
                .max_by(|&x, &y| {
                    turn(in_dir, infos[x].out_dir).total_cmp(&turn(in_dir, infos[y].out_dir))
                });
            match next {
                Some(j) if j != first => cur = j,
                _ => break,
            }
        }
        out.push('Z');
    }
    out
}

fn push_move(s: &mut String, pt: P, prec: usize) {
    s.push('M');
    push_pt(s, pt, prec);
}
