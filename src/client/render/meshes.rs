use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::PrimitiveTopology;

use frozen_city::game::types::tile_index;

use crate::client::*;

// ------------------------------------------------------------ merged meshes

/// Flat-shaded triangle soup with per-vertex colors.
#[derive(Default)]
pub(crate) struct MeshBuf {
    pos: Vec<[f32; 3]>,
    nor: Vec<[f32; 3]>,
    col: Vec<[f32; 4]>,
}

impl MeshBuf {
    pub(crate) fn tri(&mut self, a: Vec3, b: Vec3, c: Vec3, color: [f32; 4]) {
        let n = (b - a).cross(c - a).normalize_or_zero().to_array();
        for p in [a, b, c] {
            self.pos.push(p.to_array());
            self.nor.push(n);
            self.col.push(color);
        }
    }

    pub(crate) fn quad(&mut self, a: Vec3, b: Vec3, c: Vec3, d: Vec3, color: [f32; 4]) {
        self.tri(a, b, c, color);
        self.tri(a, c, d, color);
    }

    /// Quad with explicit per-vertex normals (smooth-shaded terrain).
    fn quad_smooth(&mut self, v: [(Vec3, [f32; 3]); 4], color: [f32; 4]) {
        for i in [0, 1, 2, 0, 2, 3] {
            self.pos.push(v[i].0.to_array());
            self.nor.push(v[i].1);
            self.col.push(color);
        }
    }

    /// Axis-aligned box between `min` and `max` (top, sides — no bottom).
    ///
    /// Wound counter-clockwise as seen from outside, per [`Self::tri`]'s
    /// convention. All four side walls used to be reversed, which back-face
    /// culling turned into holes rather than into shading artifacts: a rock
    /// or a tree trunk drew its lid and nothing else, reading as a flat slab
    /// lying on the snow. Only the top was ever right.
    fn boxx(&mut self, min: Vec3, max: Vec3, color: [f32; 4]) {
        let (a, b) = (min, max);
        let p = |x: f32, y: f32, z: f32| Vec3::new(x, y, z);
        // Top.
        self.quad(p(a.x, b.y, a.z), p(a.x, b.y, b.z), p(b.x, b.y, b.z), p(b.x, b.y, a.z), color);
        // Sides, facing -z, +z, -x, +x in turn.
        self.quad(p(a.x, b.y, a.z), p(b.x, b.y, a.z), p(b.x, a.y, a.z), p(a.x, a.y, a.z), color);
        self.quad(p(b.x, b.y, b.z), p(a.x, b.y, b.z), p(a.x, a.y, b.z), p(b.x, a.y, b.z), color);
        self.quad(p(a.x, b.y, b.z), p(a.x, b.y, a.z), p(a.x, a.y, a.z), p(a.x, a.y, b.z), color);
        self.quad(p(b.x, b.y, a.z), p(b.x, b.y, b.z), p(b.x, a.y, b.z), p(b.x, a.y, a.z), color);
    }

    /// Open cone (no base) with `seg` sides.
    fn cone(&mut self, center: Vec3, radius: f32, height: f32, seg: u32, color: [f32; 4]) {
        let apex = center + Vec3::Y * height;
        for i in 0..seg {
            let t0 = i as f32 / seg as f32 * std::f32::consts::TAU;
            let t1 = (i + 1) as f32 / seg as f32 * std::f32::consts::TAU;
            let p0 = center + Vec3::new(t0.cos() * radius, 0.0, t0.sin() * radius);
            let p1 = center + Vec3::new(t1.cos() * radius, 0.0, t1.sin() * radius);
            self.tri(p0, apex, p1, color);
        }
    }

    pub(crate) fn into_mesh(self) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, self.pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, self.nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, self.col);
        mesh
    }
}

fn linear(c: Color) -> [f32; 4] {
    c.to_linear().to_f32_array()
}

/// Deterministic per-grid-point hash for snow-drift heights and tree jitter.
fn hash2(x: u32, y: u32) -> u32 {
    let mut h = x.wrapping_mul(0x9E37_79B9) ^ y.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 13;
    h = h.wrapping_mul(0xC2B2_AE35);
    h ^ (h >> 16)
}

fn corner_height(gx: u32, gz: u32) -> f32 {
    // Keep the drift subtle and flatten toward the furnace so buildings sit flush.
    let d = GameState::dist_to_furnace(
        (gx.min(MAP_W as u32 - 1)) as u8,
        (gz.min(MAP_H as u32 - 1)) as u8,
    );
    let k = ((d - 3.0) / 10.0).clamp(0.0, 1.0);
    (hash2(gx, gz) % 100) as f32 * 0.0011 * k
}

/// Smooth terrain normal from the height field (central differences).
fn corner_normal(gx: u32, gz: u32) -> [f32; 3] {
    let h = |x: i64, z: i64| {
        corner_height(
            x.clamp(0, MAP_W as i64) as u32,
            z.clamp(0, MAP_H as i64) as u32,
        )
    };
    let dx = h(gx as i64 + 1, gz as i64) - h(gx as i64 - 1, gz as i64);
    let dz = h(gx as i64, gz as i64 + 1) - h(gx as i64, gz as i64 - 1);
    Vec3::new(-dx, 2.0, -dz).normalize().to_array()
}

pub(crate) fn ground_mesh(tiles: &[Tile]) -> Mesh {
    let mut buf = MeshBuf::default();
    let half_w = MAP_W as f32 / 2.0;
    let half_h = MAP_H as f32 / 2.0;
    for ty in 0..MAP_H as u32 {
        for tx in 0..MAP_W as u32 {
            let tile = &tiles[tile_index(tx as u8, ty as u8)];
            let color = linear(terrain_color(tile, tx as u8, ty as u8));
            let p = |gx: u32, gz: u32| {
                (
                    Vec3::new(
                        gx as f32 - half_w,
                        corner_height(gx, gz),
                        gz as f32 - half_h,
                    ),
                    corner_normal(gx, gz),
                )
            };
            buf.quad_smooth(
                [p(tx, ty), p(tx, ty + 1), p(tx + 1, ty + 1), p(tx + 1, ty)],
                color,
            );
        }
    }
    buf.into_mesh()
}

pub(crate) fn trees_mesh(tiles: &[Tile], dense: bool) -> Mesh {
    let mut buf = MeshBuf::default();
    let trunk = linear(Color::srgb(0.32, 0.22, 0.14));
    // Phones: one tree per tile, fewer cone facets, no snowy cap cone.
    let seg = if dense { 7 } else { 5 };
    for ty in 0..MAP_H as u8 {
        for tx in 0..MAP_W as u8 {
            let tile = &tiles[tile_index(tx, ty)];
            if tile.terrain != Terrain::Forest || tile.deposit == 0 {
                continue;
            }
            let base = tile_center_world(tx, ty);
            let n = if dense {
                1 + (tile.deposit / 40).min(2) as u32
            } else {
                1
            };
            for i in 0..n {
                let h = hash2(tx as u32 * 31 + i * 7, ty as u32 * 17 + i * 13);
                let ox = ((h % 60) as f32 - 30.0) * 0.011;
                let oz = (((h >> 8) % 60) as f32 - 30.0) * 0.011;
                let scale = 0.65 + ((h >> 16) % 40) as f32 * 0.012;
                let c = base + Vec3::new(ox, corner_height(tx as u32, ty as u32), oz);
                let green = 0.30 + ((h >> 5) % 20) as f32 * 0.006;
                let canopy = linear(Color::srgb(0.10, green, 0.16));
                let snowy = linear(Color::srgb(0.55, 0.62 + green * 0.3, 0.62));
                buf.boxx(
                    c - Vec3::new(0.035, 0.0, 0.035),
                    c + Vec3::new(0.035, 0.16 * scale, 0.035),
                    trunk,
                );
                buf.cone(c + Vec3::Y * 0.12 * scale, 0.26 * scale, 0.42 * scale, seg, canopy);
                if dense {
                    buf.cone(c + Vec3::Y * 0.38 * scale, 0.18 * scale, 0.34 * scale, seg, snowy);
                }
            }
        }
    }
    buf.into_mesh()
}

pub(crate) fn rocks_mesh(tiles: &[Tile], dense: bool) -> Mesh {
    let mut buf = MeshBuf::default();
    let rocks_per_tile = if dense { 2u32 } else { 1 };
    for ty in 0..MAP_H as u8 {
        for tx in 0..MAP_W as u8 {
            let tile = &tiles[tile_index(tx, ty)];
            if tile.terrain != Terrain::Coal || tile.deposit == 0 {
                continue;
            }
            let base = tile_center_world(tx, ty);
            let richness = (tile.deposit as f32 / 500.0).clamp(0.2, 1.0);
            for i in 0..rocks_per_tile {
                let h = hash2(tx as u32 * 13 + i * 29, ty as u32 * 7 + i * 41);
                let ox = ((h % 50) as f32 - 25.0) * 0.012;
                let oz = (((h >> 6) % 50) as f32 - 25.0) * 0.012;
                let s = 0.10 + ((h >> 12) % 30) as f32 * 0.004 + richness * 0.10;
                let dark = 0.13 + ((h >> 18) % 12) as f32 * 0.006;
                let color = linear(Color::srgb(dark, dark + 0.012, dark + 0.03));
                let c = base + Vec3::new(ox, 0.0, oz);
                buf.boxx(
                    c - Vec3::new(s, 0.0, s * 0.8),
                    c + Vec3::new(s, s * 1.5, s * 0.8),
                    color,
                );
            }
        }
    }
    buf.into_mesh()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every triangle of a closed-ish primitive must face away from the solid
    /// it encloses. This is not a shading nicety: a reversed winding is
    /// back-face culled, so the triangle vanishes completely. Two separate
    /// bugs of exactly this shape shipped — the tent/roof prism, and `boxx`'s
    /// four side walls (rocks and tree trunks drawing as flat lids) — hence a
    /// test over every builder here rather than one over the shape that broke.
    fn assert_outward(buf: MeshBuf, inside: Vec3, what: &str) {
        assert_eq!(buf.pos.len() % 3, 0, "{what}: not a triangle list");
        assert!(!buf.pos.is_empty(), "{what}: built nothing");
        for (i, tri) in buf.pos.chunks_exact(3).enumerate() {
            let a = Vec3::from_array(tri[0]);
            let b = Vec3::from_array(tri[1]);
            let c = Vec3::from_array(tri[2]);
            let normal = (b - a).cross(c - a);
            assert!(normal.length() > 1e-9, "{what}: triangle {i} is degenerate");
            let outward = ((a + b + c) / 3.0 - inside).normalize();
            assert!(
                normal.normalize().dot(outward) > 0.0,
                "{what}: triangle {i} is wound inside-out",
            );
        }
    }

    #[test]
    fn box_faces_point_outward() {
        let (min, max) = (Vec3::new(-0.3, 0.0, -0.2), Vec3::new(0.4, 0.9, 0.5));
        let mut buf = MeshBuf::default();
        buf.boxx(min, max, [1.0; 4]);
        assert_outward(buf, (min + max) / 2.0, "boxx");
    }

    #[test]
    fn cone_faces_point_outward() {
        let mut buf = MeshBuf::default();
        buf.cone(Vec3::new(0.2, 0.1, -0.4), 0.3, 0.8, 7, [1.0; 4]);
        // Inside the cone: above the base, well under the apex.
        assert_outward(buf, Vec3::new(0.2, 0.3, -0.4), "cone");
    }

    #[test]
    fn ground_faces_point_up() {
        let mut buf = MeshBuf::default();
        let flat = |x: f32, z: f32| (Vec3::new(x, 0.0, z), [0.0, 1.0, 0.0]);
        buf.quad_smooth([flat(0.0, 0.0), flat(0.0, 1.0), flat(1.0, 1.0), flat(1.0, 0.0)], [1.0; 4]);
        assert_outward(buf, Vec3::new(0.5, -1.0, 0.5), "quad_smooth");
    }
}
