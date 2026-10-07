//! Bounding volume hierarchy built with the binned surface area heuristic (SAH).

use crate::geometry::{Hit, Primitive};
use crate::math::{Aabb, Ray, Vec3};

const BINS: usize = 16;
const MAX_LEAF: usize = 4;
const FORCE_SPLIT_ABOVE: usize = 16;
const MAX_DEPTH: usize = 48;
const TRAVERSAL_COST: f64 = 0.125;
const STACK_SIZE: usize = 64;

#[derive(Clone, Copy)]
struct PrimInfo {
    bounds: Aabb,
    centroid: Vec3,
    index: usize,
}

/// Inner node: `offset` is the right child index (left child is the next node).
/// Leaf node (`count > 0`): `offset` is the first primitive index.
#[derive(Clone, Copy, Debug)]
struct Node {
    bounds: Aabb,
    offset: u32,
    count: u32,
    axis: u8,
}

/// A flattened BVH over a list of primitives.
#[derive(Clone, Debug)]
pub struct Bvh {
    nodes: Vec<Node>,
    prims: Vec<Primitive>,
}

impl Bvh {
    /// Builds a hierarchy over `prims`.
    pub fn build(prims: Vec<Primitive>) -> Self {
        if prims.is_empty() {
            return Self {
                nodes: Vec::new(),
                prims,
            };
        }
        let mut infos: Vec<PrimInfo> = prims
            .iter()
            .enumerate()
            .map(|(index, p)| {
                let bounds = p.bounds();
                PrimInfo {
                    bounds,
                    centroid: bounds.centroid(),
                    index,
                }
            })
            .collect();
        let mut nodes = Vec::with_capacity(2 * prims.len());
        build_node(&mut nodes, &mut infos, 0, 0);
        let ordered = infos.iter().map(|i| prims[i.index]).collect();
        Self {
            nodes,
            prims: ordered,
        }
    }

    /// Number of primitives.
    pub fn len(&self) -> usize {
        self.prims.len()
    }

    /// True when the hierarchy holds no primitives.
    pub fn is_empty(&self) -> bool {
        self.prims.is_empty()
    }

    /// Bounding box of everything in the hierarchy.
    pub fn bounds(&self) -> Aabb {
        self.nodes.first().map_or(Aabb::EMPTY, |n| n.bounds)
    }

    /// Nearest intersection with `t_min < t < t_max`.
    pub fn intersect(&self, ray: &Ray, t_min: f64, t_max: f64) -> Option<Hit> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv = Vec3::new(1.0 / ray.dir.x, 1.0 / ray.dir.y, 1.0 / ray.dir.z);
        let negative = [inv.x < 0.0, inv.y < 0.0, inv.z < 0.0];
        let mut stack = [0_usize; STACK_SIZE];
        let mut sp = 0;
        let mut cur = 0;
        let mut closest = t_max;
        let mut best = None;
        loop {
            let node = &self.nodes[cur];
            if node.bounds.hit(ray, inv, closest) {
                if node.count > 0 {
                    let first = node.offset as usize;
                    for prim in &self.prims[first..first + node.count as usize] {
                        if let Some(h) = prim.intersect(ray, t_min, closest) {
                            closest = h.t;
                            best = Some(h);
                        }
                    }
                } else {
                    let right = node.offset as usize;
                    if negative[usize::from(node.axis)] {
                        stack[sp] = cur + 1;
                        cur = right;
                    } else {
                        stack[sp] = right;
                        cur += 1;
                    }
                    sp += 1;
                    continue;
                }
            }
            if sp == 0 {
                break;
            }
            sp -= 1;
            cur = stack[sp];
        }
        best
    }
}

fn finish_leaf(nodes: &mut [Node], id: usize, first: usize, n: usize) {
    nodes[id].offset = first as u32;
    nodes[id].count = n as u32;
}

fn partition(infos: &mut [PrimInfo], mut pred: impl FnMut(&PrimInfo) -> bool) -> usize {
    let mut i = 0;
    for j in 0..infos.len() {
        if pred(&infos[j]) {
            infos.swap(i, j);
            i += 1;
        }
    }
    i
}

fn build_node(nodes: &mut Vec<Node>, infos: &mut [PrimInfo], first: usize, depth: usize) -> usize {
    let n = infos.len();
    let bounds = infos.iter().fold(Aabb::EMPTY, |b, i| b.union(i.bounds));
    let id = nodes.len();
    nodes.push(Node {
        bounds,
        offset: 0,
        count: 0,
        axis: 0,
    });
    if n <= MAX_LEAF || depth >= MAX_DEPTH {
        finish_leaf(nodes, id, first, n);
        return id;
    }

    let centroid_bounds = infos.iter().fold(Aabb::EMPTY, |b, i| b.grow(i.centroid));
    let axis = centroid_bounds.longest_axis();
    let lo = centroid_bounds.min.component(axis);
    let hi = centroid_bounds.max.component(axis);

    let mid = if hi - lo <= 0.0 {
        if n > FORCE_SPLIT_ABOVE {
            n / 2
        } else {
            finish_leaf(nodes, id, first, n);
            return id;
        }
    } else {
        let scale = BINS as f64 / (hi - lo);
        let bin_of =
            |c: Vec3| -> usize { (((c.component(axis) - lo) * scale) as usize).min(BINS - 1) };
        let mut bins = [(Aabb::EMPTY, 0_usize); BINS];
        for info in &*infos {
            let b = bin_of(info.centroid);
            bins[b].0 = bins[b].0.union(info.bounds);
            bins[b].1 += 1;
        }

        let mut right_area = [0.0_f64; BINS];
        let mut right_count = [0_usize; BINS];
        let mut acc = (Aabb::EMPTY, 0_usize);
        for k in (0..BINS - 1).rev() {
            acc.0 = acc.0.union(bins[k + 1].0);
            acc.1 += bins[k + 1].1;
            right_area[k] = acc.0.surface_area();
            right_count[k] = acc.1;
        }

        let parent_area = bounds.surface_area();
        let mut best: Option<(f64, usize)> = None;
        let mut left = (Aabb::EMPTY, 0_usize);
        for k in 0..BINS - 1 {
            left.0 = left.0.union(bins[k].0);
            left.1 += bins[k].1;
            if left.1 == 0 || right_count[k] == 0 {
                continue;
            }
            let cost = TRAVERSAL_COST
                + (left.1 as f64 * left.0.surface_area() + right_count[k] as f64 * right_area[k])
                    / parent_area;
            if best.is_none_or(|(c, _)| cost < c) {
                best = Some((cost, k));
            }
        }

        match best {
            Some((cost, k)) if cost < n as f64 || n > FORCE_SPLIT_ABOVE => {
                partition(infos, |i| bin_of(i.centroid) <= k)
            }
            _ if n > FORCE_SPLIT_ABOVE => n / 2,
            _ => {
                finish_leaf(nodes, id, first, n);
                return id;
            }
        }
    };
    let mid = if mid == 0 || mid == n { n / 2 } else { mid };

    let (left_infos, right_infos) = infos.split_at_mut(mid);
    build_node(nodes, left_infos, first, depth + 1);
    let right = build_node(nodes, right_infos, first + mid, depth + 1);
    nodes[id].offset = right as u32;
    nodes[id].axis = axis as u8;
    id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Sphere, Triangle};
    use crate::math::Rng;

    fn random_scene(rng: &mut Rng, n: usize) -> Vec<Primitive> {
        (0..n)
            .map(|i| {
                let c = Vec3::new(
                    rng.range(-20.0, 20.0),
                    rng.range(-20.0, 20.0),
                    rng.range(-20.0, 20.0),
                );
                if i % 2 == 0 {
                    Primitive::Sphere(Sphere::new(c, rng.range(0.2, 1.5)))
                } else {
                    let mut corner = || {
                        c + Vec3::new(
                            rng.range(-2.0, 2.0),
                            rng.range(-2.0, 2.0),
                            rng.range(-2.0, 2.0),
                        )
                    };
                    Primitive::Triangle(Triangle::new(c, corner(), corner()))
                }
            })
            .collect()
    }

    fn brute_force(prims: &[Primitive], ray: &Ray) -> Option<Hit> {
        let mut closest = f64::INFINITY;
        let mut best = None;
        for p in prims {
            if let Some(h) = p.intersect(ray, 1e-9, closest) {
                closest = h.t;
                best = Some(h);
            }
        }
        best
    }

    #[test]
    fn empty_bvh_never_hits() {
        let bvh = Bvh::build(Vec::new());
        assert!(bvh.is_empty());
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        assert!(bvh.intersect(&ray, 1e-9, f64::INFINITY).is_none());
    }

    #[test]
    fn matches_brute_force() {
        let mut rng = Rng::new(2024);
        let prims = random_scene(&mut rng, 400);
        let bvh = Bvh::build(prims.clone());
        assert_eq!(bvh.len(), prims.len());
        let mut hits = 0;
        for _ in 0..1000 {
            let origin = Vec3::new(
                rng.range(-30.0, 30.0),
                rng.range(-30.0, 30.0),
                rng.range(-30.0, 30.0),
            );
            let ray = Ray::new(origin, rng.unit_vector());
            match (
                bvh.intersect(&ray, 1e-9, f64::INFINITY),
                brute_force(&prims, &ray),
            ) {
                (None, None) => {}
                (Some(a), Some(b)) => {
                    hits += 1;
                    assert!((a.t - b.t).abs() < 1e-9);
                }
                _ => panic!("BVH and brute force disagree"),
            }
        }
        assert!(hits > 50, "scene should produce plenty of hits");
    }

    #[test]
    fn root_bounds_contain_everything() {
        let mut rng = Rng::new(5);
        let prims = random_scene(&mut rng, 100);
        let bvh = Bvh::build(prims.clone());
        let root = bvh.bounds();
        for p in &prims {
            assert_eq!(root.union(p.bounds()), root);
        }
    }

    #[test]
    fn identical_primitives_do_not_overflow_stack() {
        let s = Primitive::Sphere(Sphere::new(Vec3::new(0.0, 0.0, -5.0), 1.0));
        let bvh = Bvh::build(vec![s; 500]);
        let ray = Ray::new(Vec3::ZERO, Vec3::new(0.0, 0.0, -1.0));
        let h = bvh.intersect(&ray, 1e-9, f64::INFINITY).expect("hit");
        assert!((h.t - 4.0).abs() < 1e-9);
    }
}
