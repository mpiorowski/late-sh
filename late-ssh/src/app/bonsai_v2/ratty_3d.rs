use std::{
    collections::{BTreeMap, hash_map::DefaultHasher},
    f32::consts::TAU,
    fmt::Write as _,
    hash::{Hash, Hasher},
};

use base64::Engine as _;
use ratatui::layout::Rect;

use crate::app::bonsai_v2::state::{BonsaiV2State, Branch, BranchStatus};

const PAYLOAD_CHUNK_SIZE: usize = 3072;
const BRANCH_OBJECT_ID: u32 = 0xB05A_1001;
const LEAF_OBJECT_ID: u32 = 0xB05A_1002;
const POT_OBJECT_ID: u32 = 0xB05A_1003;
const SELECTED_OBJECT_ID: u32 = 0xB05A_1004;
const OBJECT_IDS: [u32; 4] = [
    BRANCH_OBJECT_ID,
    LEAF_OBJECT_ID,
    POT_OBJECT_ID,
    SELECTED_OBJECT_ID,
];

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub(crate) struct RattyBonsaiFrame {
    area: Option<Rect>,
}

impl RattyBonsaiFrame {
    pub(crate) fn place(&mut self, area: Rect) {
        if area.width >= 12 && area.height >= 6 {
            self.area = Some(area);
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct RattyBonsaiRenderState {
    branches: ObjectState,
    leaves: ObjectState,
    pot: ObjectState,
    selected: ObjectState,
}

impl RattyBonsaiRenderState {
    pub(crate) fn build_commands(
        &mut self,
        frame: &RattyBonsaiFrame,
        state: &BonsaiV2State,
    ) -> Vec<Vec<u8>> {
        let Some(area) = frame.area else {
            return self.clear_commands();
        };

        let mut meshes = build_meshes(state);
        normalize_meshes(&mut meshes);

        let calibration_bounds = mesh_bounds(&meshes);
        let signature = state_signature(state);
        let rotation = state.ratty_rotation();
        let mut commands = Vec::new();
        push_object(
            &mut commands,
            &mut self.branches,
            ObjectDraw {
                id: BRANCH_OBJECT_ID,
                name: "bonsai-branches.obj",
                color: if state.is_alive {
                    [154, 102, 55]
                } else {
                    [118, 118, 118]
                },
                brightness: if state.is_alive { 1.10 } else { 0.82 },
            },
            signature_with_kind(signature, 1),
            meshes
                .branches
                .to_obj_with_bounds("bonsai_branches", calibration_bounds),
            area,
            rotation,
        );
        push_object(
            &mut commands,
            &mut self.leaves,
            ObjectDraw {
                id: LEAF_OBJECT_ID,
                name: "bonsai-leaves.obj",
                color: if !state.is_alive {
                    [93, 99, 89]
                } else if state.water_stress >= 60 {
                    [204, 157, 78]
                } else {
                    [87, 180, 103]
                },
                brightness: if state.is_alive { 1.22 } else { 0.70 },
            },
            signature_with_kind(signature, 2),
            meshes
                .leaves
                .to_obj_with_bounds("bonsai_leaves", calibration_bounds),
            area,
            rotation,
        );
        push_object(
            &mut commands,
            &mut self.pot,
            ObjectDraw {
                id: POT_OBJECT_ID,
                name: "bonsai-pot.obj",
                color: [104, 106, 114],
                brightness: 0.95,
            },
            signature_with_kind(signature, 3),
            meshes
                .pot
                .to_obj_with_bounds("bonsai_pot", calibration_bounds),
            area,
            rotation,
        );
        push_object(
            &mut commands,
            &mut self.selected,
            ObjectDraw {
                id: SELECTED_OBJECT_ID,
                name: "bonsai-selected.obj",
                color: [252, 190, 96],
                brightness: 1.35,
            },
            signature_with_kind(signature, 4),
            meshes
                .selected
                .to_obj_with_bounds("bonsai_selected", calibration_bounds),
            area,
            rotation,
        );

        commands
    }

    pub(crate) fn reset(&mut self) {
        *self = Self::default();
    }

    fn clear_commands(&mut self) -> Vec<Vec<u8>> {
        let mut commands = Vec::new();
        clear_object(&mut commands, &mut self.branches, BRANCH_OBJECT_ID);
        clear_object(&mut commands, &mut self.leaves, LEAF_OBJECT_ID);
        clear_object(&mut commands, &mut self.pot, POT_OBJECT_ID);
        clear_object(&mut commands, &mut self.selected, SELECTED_OBJECT_ID);
        commands
    }
}

pub(crate) fn cleanup_commands() -> Vec<Vec<u8>> {
    OBJECT_IDS.into_iter().map(delete_command).collect()
}

#[derive(Debug, Default)]
struct ObjectState {
    signature: Option<u64>,
    active: bool,
}

#[derive(Clone, Copy)]
struct ObjectDraw {
    id: u32,
    name: &'static str,
    color: [u8; 3],
    brightness: f32,
}

fn push_object(
    commands: &mut Vec<Vec<u8>>,
    slot: &mut ObjectState,
    draw: ObjectDraw,
    signature: u64,
    obj: Option<Vec<u8>>,
    area: Rect,
    rotation: (f32, f32),
) {
    let Some(obj) = obj else {
        clear_object(commands, slot, draw.id);
        return;
    };

    if slot.signature != Some(signature) {
        commands.extend(register_payload_commands(draw.id, draw.name, &obj));
        slot.signature = Some(signature);
    }
    commands.push(place_command(draw, area, rotation));
    slot.active = true;
}

fn clear_object(commands: &mut Vec<Vec<u8>>, slot: &mut ObjectState, id: u32) {
    if slot.active || slot.signature.is_some() {
        commands.push(delete_command(id));
    }
    *slot = ObjectState::default();
}

fn register_payload_commands(id: u32, name: &str, bytes: &[u8]) -> Vec<Vec<u8>> {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    let mut commands = Vec::new();
    for (index, start) in (0..encoded.len()).step_by(PAYLOAD_CHUNK_SIZE).enumerate() {
        let end = (start + PAYLOAD_CHUNK_SIZE).min(encoded.len());
        let more = u8::from(end < encoded.len());
        let chunk = &encoded[start..end];
        let command = if index == 0 {
            format!(
                "\x1b_ratty;g;r;id={id};fmt=obj;source=payload;more={more};name={name};{chunk}\x1b\\"
            )
        } else {
            format!("\x1b_ratty;g;r;id={id};fmt=obj;source=payload;more={more};{chunk}\x1b\\")
        };
        commands.push(command.into_bytes());
    }

    if commands.is_empty() {
        commands.push(
            format!("\x1b_ratty;g;r;id={id};fmt=obj;source=payload;more=0;name={name};\x1b\\")
                .into_bytes(),
        );
    }
    commands
}

fn place_command(draw: ObjectDraw, area: Rect, rotation: (f32, f32)) -> Vec<u8> {
    let row_offset = ((u32::from(area.height.saturating_sub(1)) * 56) / 100) as u16;
    let center_row = area.y.saturating_add(row_offset);
    let center_col = area.x.saturating_add(area.width.saturating_sub(1) / 2);
    let [r, g, b] = draw.color;
    let (rx, ry) = rotation;
    let scale = match area.height {
        0..=11 => 0.26,
        12..=19 => 0.32,
        _ => 0.38,
    };
    format!(
        "\x1b_ratty;g;p;id={};row={};col={};w={};h={};animate=0;scale={};depth=0.9;color={r:02x}{g:02x}{b:02x};brightness={};px=0;py=0;pz=0;rx={rx:.2};ry={ry:.2};rz=0;sx=1;sy=1.05;sz=0.82\x1b\\",
        draw.id,
        center_row,
        center_col,
        area.width.max(1),
        area.height.max(1),
        scale,
        draw.brightness,
    )
    .into_bytes()
}

fn delete_command(id: u32) -> Vec<u8> {
    format!("\x1b_ratty;g;d;id={id}\x1b\\").into_bytes()
}

#[derive(Clone, Copy, Debug, Default)]
struct Vec3 {
    x: f32,
    y: f32,
    z: f32,
}

impl Vec3 {
    const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    fn dot(self, rhs: Self) -> f32 {
        self.x * rhs.x + self.y * rhs.y + self.z * rhs.z
    }

    fn cross(self, rhs: Self) -> Self {
        Self {
            x: self.y * rhs.z - self.z * rhs.y,
            y: self.z * rhs.x - self.x * rhs.z,
            z: self.x * rhs.y - self.y * rhs.x,
        }
    }

    fn length(self) -> f32 {
        self.dot(self).sqrt()
    }

    fn normalized(self) -> Self {
        let len = self.length();
        if len <= f32::EPSILON {
            Self::new(0.0, 1.0, 0.0)
        } else {
            self * (1.0 / len)
        }
    }
}

impl std::ops::Add for Vec3 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y, self.z + rhs.z)
    }
}

impl std::ops::Sub for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y, self.z - rhs.z)
    }
}

impl std::ops::Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::new(self.x * rhs, self.y * rhs, self.z * rhs)
    }
}

#[derive(Debug, Default)]
struct Mesh {
    vertices: Vec<Vec3>,
    faces: Vec<Vec<usize>>,
}

impl Mesh {
    fn is_empty(&self) -> bool {
        self.vertices.is_empty() || self.faces.is_empty()
    }

    fn add_vertex(&mut self, vertex: Vec3) -> usize {
        self.vertices.push(vertex);
        self.vertices.len() - 1
    }

    fn add_face(&mut self, face: impl Into<Vec<usize>>) {
        self.faces.push(face.into());
    }

    fn to_obj_with_bounds(
        &self,
        name: &str,
        calibration_bounds: Option<(Vec3, Vec3)>,
    ) -> Option<Vec<u8>> {
        if self.is_empty() {
            return None;
        }
        let mut out = String::new();
        let _ = writeln!(out, "o {name}");

        let calibration_vertices = calibration_bounds.map(calibration_vertices);
        let vertex_offset = calibration_vertices
            .as_ref()
            .map_or(0, |vertices| vertices.len());
        if let Some(vertices) = &calibration_vertices {
            for vertex in vertices {
                let _ = writeln!(out, "v {:.4} {:.4} {:.4}", vertex.x, vertex.y, vertex.z);
            }
        }
        for vertex in &self.vertices {
            let _ = writeln!(out, "v {:.4} {:.4} {:.4}", vertex.x, vertex.y, vertex.z);
        }
        if let Some(vertices) = &calibration_vertices {
            let last = vertices.len();
            let _ = writeln!(out, "f 1 1 1");
            let _ = writeln!(out, "f {last} {last} {last}");
        }
        for face in &self.faces {
            out.push('f');
            for idx in face {
                let _ = write!(out, " {}", idx + 1 + vertex_offset);
            }
            out.push('\n');
        }
        Some(out.into_bytes())
    }
}

#[derive(Default)]
struct MeshSet {
    branches: Mesh,
    leaves: Mesh,
    pot: Mesh,
    selected: Mesh,
}

fn build_meshes(state: &BonsaiV2State) -> MeshSet {
    let mut set = MeshSet::default();
    let mut z_by_branch_id = BTreeMap::new();
    let mut branches = state.graph.branches.iter().collect::<Vec<_>>();
    branches.sort_by_key(|branch| branch.id);

    for branch in branches {
        let start_z = branch
            .parent_id
            .and_then(|parent_id| z_by_branch_id.get(&parent_id).copied())
            .unwrap_or(0.0);
        let end_z = (start_z + branch_z_delta(state.seed, branch.id)).clamp(-1.35, 1.35);
        z_by_branch_id.insert(branch.id, end_z);

        if matches!(branch.status, BranchStatus::Cut) {
            continue;
        }

        let start = world_point(branch.start_x, branch.start_y, start_z);
        let mut end = world_point(branch.end_x, branch.end_y, end_z);
        if (end - start).length() < 0.05 {
            end = end + Vec3::new(0.0, 0.34, 0.0);
        }

        let radius = branch_radius(branch);
        add_tube(&mut set.branches, start, end, radius, 7);

        if Some(branch.id) == state.selected_branch_id
            && !matches!(branch.status, BranchStatus::Deadwood)
        {
            add_tube(&mut set.selected, start, end, radius * 1.75, 7);
        }

        let is_tip = state.graph.is_tip(branch.id);
        match branch.status {
            BranchStatus::LeafPad => add_leaf_cluster(&mut set.leaves, end, state.seed, branch.id),
            BranchStatus::NeedsPinch => add_octahedron(
                &mut set.leaves,
                end + Vec3::new(0.0, 0.05, 0.0),
                Vec3::new(0.07, 0.08, 0.07),
            ),
            BranchStatus::Growing | BranchStatus::Wired if is_tip => {
                add_tip_bud(&mut set.leaves, end, state.seed, branch.id);
            }
            _ => {}
        }
    }

    add_pot(&mut set.pot);
    set
}

fn world_point(x: i16, y: i16, z: f32) -> Vec3 {
    Vec3::new(x as f32 * 0.28, y as f32 * 0.36, z)
}

fn branch_radius(branch: &Branch) -> f32 {
    let base = 0.032 + f32::from(branch.thickness.min(4)) * 0.014;
    if matches!(branch.status, BranchStatus::Deadwood) {
        base * 0.72
    } else {
        base
    }
}

fn branch_z_delta(seed: i64, branch_id: i32) -> f32 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    branch_id.hash(&mut hasher);
    3_u8.hash(&mut hasher);
    let roll = (hasher.finish() % 11) as i32 - 5;
    roll as f32 * 0.045
}

fn add_tube(mesh: &mut Mesh, start: Vec3, end: Vec3, radius: f32, sides: usize) {
    let dir = end - start;
    let len = dir.length();
    if len <= f32::EPSILON || sides < 3 {
        return;
    }

    let axis = dir * (1.0 / len);
    let helper = if axis.y.abs() > 0.88 {
        Vec3::new(1.0, 0.0, 0.0)
    } else {
        Vec3::new(0.0, 1.0, 0.0)
    };
    let u = axis.cross(helper).normalized();
    let v = axis.cross(u).normalized();

    let mut start_ring = Vec::with_capacity(sides);
    let mut end_ring = Vec::with_capacity(sides);
    for i in 0..sides {
        let angle = TAU * i as f32 / sides as f32;
        let offset = u * (angle.cos() * radius) + v * (angle.sin() * radius);
        start_ring.push(mesh.add_vertex(start + offset));
        end_ring.push(mesh.add_vertex(end + offset));
    }

    for i in 0..sides {
        let next = (i + 1) % sides;
        mesh.add_face(vec![
            start_ring[i],
            start_ring[next],
            end_ring[next],
            end_ring[i],
        ]);
    }

    let start_center = mesh.add_vertex(start);
    let end_center = mesh.add_vertex(end);
    for i in 0..sides {
        let next = (i + 1) % sides;
        mesh.add_face(vec![start_center, start_ring[i], start_ring[next]]);
        mesh.add_face(vec![end_center, end_ring[next], end_ring[i]]);
    }
}

fn add_leaf_cluster(mesh: &mut Mesh, center: Vec3, seed: i64, branch_id: i32) {
    let flip = if branch_hash(seed, branch_id, 1) % 2 == 0 {
        1.0
    } else {
        -1.0
    };
    let lean = ((branch_hash(seed, branch_id, 2) % 7) as f32 - 3.0) * 0.018;
    let offsets = [
        Vec3::new(0.0, 0.02, 0.0),
        Vec3::new(0.16 * flip, 0.02, 0.02),
        Vec3::new(-0.14 * flip, 0.04, -0.03),
        Vec3::new(0.02, 0.14, 0.13),
        Vec3::new(-0.02, 0.12, -0.14),
    ];
    for (idx, offset) in offsets.into_iter().enumerate() {
        let scale = 0.09 + idx as f32 * 0.006;
        add_octahedron(
            mesh,
            center + offset + Vec3::new(lean, 0.0, -lean),
            Vec3::new(scale * 1.25, scale, scale * 0.95),
        );
    }
}

fn add_tip_bud(mesh: &mut Mesh, center: Vec3, seed: i64, branch_id: i32) {
    let flip = if branch_hash(seed, branch_id, 8) % 2 == 0 {
        1.0
    } else {
        -1.0
    };
    let offsets = [
        Vec3::new(0.0, 0.065, 0.0),
        Vec3::new(0.075 * flip, 0.03, 0.035),
        Vec3::new(-0.055 * flip, 0.035, -0.03),
    ];
    for (idx, offset) in offsets.into_iter().enumerate() {
        let scale = 0.044 + idx as f32 * 0.004;
        add_octahedron(mesh, center + offset, Vec3::new(scale * 1.2, scale, scale));
    }
}

fn add_octahedron(mesh: &mut Mesh, center: Vec3, radius: Vec3) {
    let top = mesh.add_vertex(center + Vec3::new(0.0, radius.y, 0.0));
    let bottom = mesh.add_vertex(center - Vec3::new(0.0, radius.y, 0.0));
    let left = mesh.add_vertex(center - Vec3::new(radius.x, 0.0, 0.0));
    let right = mesh.add_vertex(center + Vec3::new(radius.x, 0.0, 0.0));
    let front = mesh.add_vertex(center + Vec3::new(0.0, 0.0, radius.z));
    let back = mesh.add_vertex(center - Vec3::new(0.0, 0.0, radius.z));

    mesh.add_face(vec![top, right, front]);
    mesh.add_face(vec![top, front, left]);
    mesh.add_face(vec![top, left, back]);
    mesh.add_face(vec![top, back, right]);
    mesh.add_face(vec![bottom, front, right]);
    mesh.add_face(vec![bottom, left, front]);
    mesh.add_face(vec![bottom, back, left]);
    mesh.add_face(vec![bottom, right, back]);
}

fn add_pot(mesh: &mut Mesh) {
    add_box(
        mesh,
        Vec3::new(-0.92, -0.42, -0.36),
        Vec3::new(0.92, -0.05, 0.36),
    );
    add_box(
        mesh,
        Vec3::new(-1.10, -0.09, -0.43),
        Vec3::new(1.10, 0.05, 0.43),
    );
}

fn add_box(mesh: &mut Mesh, min: Vec3, max: Vec3) {
    let v000 = mesh.add_vertex(Vec3::new(min.x, min.y, min.z));
    let v001 = mesh.add_vertex(Vec3::new(min.x, min.y, max.z));
    let v010 = mesh.add_vertex(Vec3::new(min.x, max.y, min.z));
    let v011 = mesh.add_vertex(Vec3::new(min.x, max.y, max.z));
    let v100 = mesh.add_vertex(Vec3::new(max.x, min.y, min.z));
    let v101 = mesh.add_vertex(Vec3::new(max.x, min.y, max.z));
    let v110 = mesh.add_vertex(Vec3::new(max.x, max.y, min.z));
    let v111 = mesh.add_vertex(Vec3::new(max.x, max.y, max.z));

    mesh.add_face(vec![v000, v100, v110, v010]);
    mesh.add_face(vec![v001, v011, v111, v101]);
    mesh.add_face(vec![v000, v001, v101, v100]);
    mesh.add_face(vec![v010, v110, v111, v011]);
    mesh.add_face(vec![v000, v010, v011, v001]);
    mesh.add_face(vec![v100, v101, v111, v110]);
}

fn normalize_meshes(set: &mut MeshSet) {
    let Some((min, max)) = mesh_bounds(set) else {
        return;
    };
    let center = Vec3::new(
        (min.x + max.x) * 0.5,
        (min.y + max.y) * 0.5,
        (min.z + max.z) * 0.5,
    );
    let span = (max.x - min.x)
        .max(max.y - min.y)
        .max(max.z - min.z)
        .max(0.1);
    let scale = 1.35 / span;

    for mesh in [
        &mut set.branches,
        &mut set.leaves,
        &mut set.pot,
        &mut set.selected,
    ] {
        for vertex in &mut mesh.vertices {
            *vertex = (*vertex - center) * scale;
        }
    }
}

fn mesh_bounds(set: &MeshSet) -> Option<(Vec3, Vec3)> {
    let mut min = Vec3::new(f32::INFINITY, f32::INFINITY, f32::INFINITY);
    let mut max = Vec3::new(f32::NEG_INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY);
    let mut seen = false;
    for mesh in [&set.branches, &set.leaves, &set.pot, &set.selected] {
        for vertex in &mesh.vertices {
            min.x = min.x.min(vertex.x);
            min.y = min.y.min(vertex.y);
            min.z = min.z.min(vertex.z);
            max.x = max.x.max(vertex.x);
            max.y = max.y.max(vertex.y);
            max.z = max.z.max(vertex.z);
            seen = true;
        }
    }
    seen.then_some((min, max))
}

fn calibration_vertices((min, max): (Vec3, Vec3)) -> [Vec3; 8] {
    [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(min.x, max.y, max.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(max.x, max.y, max.z),
    ]
}

fn branch_hash(seed: i64, branch_id: i32, salt: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    seed.hash(&mut hasher);
    branch_id.hash(&mut hasher);
    salt.hash(&mut hasher);
    hasher.finish()
}

fn signature_with_kind(signature: u64, kind: u64) -> u64 {
    let mut hasher = DefaultHasher::new();
    signature.hash(&mut hasher);
    kind.hash(&mut hasher);
    hasher.finish()
}

fn state_signature(state: &BonsaiV2State) -> u64 {
    let mut hasher = DefaultHasher::new();
    state.seed.hash(&mut hasher);
    state.is_alive.hash(&mut hasher);
    state.vigor.hash(&mut hasher);
    state.water_stress.hash(&mut hasher);
    state.selected_branch_id.hash(&mut hasher);
    state.graph.version.hash(&mut hasher);
    state.graph.next_id.hash(&mut hasher);
    for branch in &state.graph.branches {
        branch.id.hash(&mut hasher);
        branch.parent_id.hash(&mut hasher);
        branch.start_x.hash(&mut hasher);
        branch.start_y.hash(&mut hasher);
        branch.end_x.hash(&mut hasher);
        branch.end_y.hash(&mut hasher);
        branch.thickness.hash(&mut hasher);
        branch.age.hash(&mut hasher);
        branch.vigor.hash(&mut hasher);
        branch_status_code(branch.status).hash(&mut hasher);
        branch.bend_x.hash(&mut hasher);
        branch.bend_y.hash(&mut hasher);
        branch.last_pruned_day.hash(&mut hasher);
        branch.ramification.hash(&mut hasher);
        branch.last_pinched_age.hash(&mut hasher);
    }
    hasher.finish()
}

fn branch_status_code(status: BranchStatus) -> u8 {
    match status {
        BranchStatus::Growing => 0,
        BranchStatus::Wired => 1,
        BranchStatus::Pinched => 2,
        BranchStatus::NeedsPinch => 3,
        BranchStatus::Cut => 4,
        BranchStatus::Deadwood => 5,
        BranchStatus::LeafPad => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_payload_chunks_large_obj() {
        let commands = register_payload_commands(42, "bonsai.obj", &[b'v'; 4096]);
        assert!(commands.len() > 1);
        let first = String::from_utf8_lossy(&commands[0]);
        assert!(first.contains("source=payload"));
        assert!(first.contains("more=1"));
        assert!(first.contains("name=bonsai.obj"));
        let last = String::from_utf8_lossy(commands.last().expect("last command"));
        assert!(last.contains("more=0"));
    }

    #[test]
    fn mesh_exports_obj_vertices_and_faces() {
        let mut mesh = Mesh::default();
        add_box(
            &mut mesh,
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        );
        let obj =
            String::from_utf8(mesh.to_obj_with_bounds("box", None).expect("obj")).expect("utf8");
        assert!(obj.contains("o box"));
        assert!(obj.contains("\nv "));
        assert!(obj.contains("\nf "));
    }

    #[test]
    fn cleanup_deletes_all_bonsai_objects() {
        let joined = cleanup_commands()
            .into_iter()
            .map(|command| String::from_utf8(command).expect("utf8"))
            .collect::<Vec<_>>()
            .join("");
        for id in OBJECT_IDS {
            assert!(joined.contains(&format!("id={id}")));
        }
    }
}
