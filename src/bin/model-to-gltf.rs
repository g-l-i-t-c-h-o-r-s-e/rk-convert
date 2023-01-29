use std::collections::HashMap;
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use gltf_json::{Root, Index, Node, Mesh, Scene, Accessor, Buffer};
use gltf_json::accessor::{self, ComponentType, GenericComponentType};
use gltf_json::buffer::{self, View};
use gltf_json::mesh::{self, Primitive, Semantic};
use gltf_json::validation::Checked;
use rkengine::anim::AnimFile;
use rkengine::anim_extra::{self, AnimRange};
use rkengine::model::ModelFile;
use rkengine::pvr::PvrFile;


pub struct GltfBuilder {
    root: Root,
    bin: Vec<u8>,
    bin_buffer: Index<Buffer>,
}

impl Default for GltfBuilder {
    fn default() -> GltfBuilder {
        let mut root = Root::default();
        root.buffers.push(Buffer {
            byte_length: 0,
            uri: None,
            name: None,
            extensions: None,
            extras: Default::default(),
        });
        GltfBuilder {
            root,
            bin: Vec::new(),
            bin_buffer: Index::new(0),
        }
    }
}

impl GltfBuilder {
    pub fn push_node(&mut self, node: Node) -> Index<Node> {
        let i = Index::new(self.root.nodes.len() as u32);
        self.root.nodes.push(node);
        i
    }

    pub fn push_mesh(&mut self, mesh: Mesh) -> Index<Mesh> {
        let i = Index::new(self.root.meshes.len() as u32);
        self.root.meshes.push(mesh);
        i
    }

    pub fn push_accessor(&mut self, accessor: Accessor) -> Index<Accessor> {
        let i = Index::new(self.root.accessors.len() as u32);
        self.root.accessors.push(accessor);
        i
    }

    pub fn push_view(&mut self, view: View) -> Index<View> {
        let i = Index::new(self.root.buffer_views.len() as u32);
        self.root.buffer_views.push(view);
        i
    }

    pub fn push_scene(&mut self, scene: Scene) -> Index<Scene> {
        let i = Index::new(self.root.scenes.len() as u32);
        self.root.scenes.push(scene);
        i
    }


    pub fn set_default_scene(&mut self, scene_idx: Index<Scene>) {
        self.root.scene = Some(scene_idx);
    }

    pub fn push_bin_view(
        &mut self,
        data: &[u8],
        target: buffer::Target,
    ) -> Index<View> {
        let offset = self.bin.len();
        self.bin.extend_from_slice(data);
        if data.len() % 4 != 0 {
            for i in (data.len() % 4) .. 4 {
                self.bin.push(0);
            }
        }

        self.push_view(View {
            buffer: self.bin_buffer,
            byte_length: data.len() as u32,
            byte_offset: Some(offset as u32),
            byte_stride: None,
            target: Some(Checked::Valid(target)),
            name: None,
            extensions: None,
            extras: Default::default(),
        })
    }

    pub fn push_prim_accessor<T: PrimType>(
        &mut self,
        data: &[T],
        buffer_target: buffer::Target,
        normalized: bool,
    ) -> Index<Accessor> {
        let byte_len = data.len() * T::SIZE;

        let offset = self.bin.len();
        self.bin.reserve(byte_len);
        for &x in data {
            x.push_bytes(&mut self.bin);
        }
        assert_eq!(self.bin.len() - offset, byte_len);
        while self.bin.len() % 4 != 0 {
            self.bin.push(0);
        }

        let view_idx = self.push_view(View {
            buffer: self.bin_buffer,
            byte_length: byte_len as u32,
            byte_offset: Some(offset as u32),
            byte_stride: None,
            target: Some(Checked::Valid(buffer_target)),
            name: None,
            extensions: None,
            extras: Default::default(),
        });
        self.push_accessor(Accessor {
            buffer_view: Some(view_idx),
            byte_offset: 0,
            count: data.len() as u32,
            component_type: Checked::Valid(T::COMPONENT_TYPE),
            type_: Checked::Valid(T::TYPE),
            min: None,
            max: None,
            normalized,
            sparse: None,
            name: None,
            extensions: None,
            extras: Default::default(),
        })
    }


    pub fn finish(mut self) -> Vec<u8> {
        self.root.buffers[self.bin_buffer.value()].byte_length = self.bin.len() as u32;

        let mut out = Vec::new();

        // File header
        out.extend_from_slice(b"glTF");
        out.extend_from_slice(&2_u32.to_le_bytes());
        let file_len_pos = out.len();
        out.extend_from_slice(&0_u32.to_le_bytes());

        // JSON chunk header
        let json_len_pos = out.len();
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(b"JSON");
        let start = out.len();
        self.root.to_writer(&mut out).unwrap();
        while out.len() % 4 != 0 {
            out.push(b' ');
        }
        let end = out.len();
        let len = end - start;
        out[json_len_pos .. json_len_pos + 4].copy_from_slice(&(len as u32).to_le_bytes());

        // Binary chunk header
        let bin_len_pos = out.len();
        out.extend_from_slice(&0_u32.to_le_bytes());
        out.extend_from_slice(b"BIN\0");
        let start = out.len();
        out.extend_from_slice(&self.bin);
        while out.len() % 4 != 0 {
            out.push(b' ');
        }
        let end = out.len();
        let len = end - start;
        out[bin_len_pos .. bin_len_pos + 4].copy_from_slice(&(len as u32).to_le_bytes());

        let len = out.len();
        out[file_len_pos .. file_len_pos + 4].copy_from_slice(&(len as u32).to_le_bytes());

        out
    }
}


pub trait PrimType: Copy {
    const COMPONENT_TYPE: GenericComponentType;
    const TYPE: accessor::Type;
    const SIZE: usize;
    fn push_bytes(self, v: &mut Vec<u8>);
}

impl PrimType for f32 {
    const COMPONENT_TYPE: GenericComponentType = GenericComponentType(ComponentType::F32);
    const TYPE: accessor::Type = accessor::Type::Scalar;
    const SIZE: usize = 4;
    fn push_bytes(self, v: &mut Vec<u8>) {
        v.extend_from_slice(&self.to_le_bytes());
    }
}

impl PrimType for [f32; 3] {
    const COMPONENT_TYPE: GenericComponentType = GenericComponentType(ComponentType::F32);
    const TYPE: accessor::Type = accessor::Type::Vec3;
    const SIZE: usize = 12;
    fn push_bytes(self, v: &mut Vec<u8>) {
        v.extend_from_slice(&self[0].to_le_bytes());
        v.extend_from_slice(&self[1].to_le_bytes());
        v.extend_from_slice(&self[2].to_le_bytes());
    }
}

impl PrimType for u32 {
    const COMPONENT_TYPE: GenericComponentType = GenericComponentType(ComponentType::U32);
    const TYPE: accessor::Type = accessor::Type::Scalar;
    const SIZE: usize = 4;
    fn push_bytes(self, v: &mut Vec<u8>) {
        v.extend_from_slice(&self.to_le_bytes());
    }
}


fn main() -> io::Result<()> {
    // Load model

    let args = env::args_os().collect::<Vec<_>>();
    assert!(
        args.len() == 2 || args.len() == 3,
        "usage: {} <model.rk> [anim.csv|anim.anim]",
        args[0].to_string_lossy(),
    );

    let model_path = Path::new(&args[1]);
    let anim_path = args.get(2).map(|s| Path::new(s));

    eprintln!("load object from {}", model_path.display());
    let mut mf = ModelFile::new(File::open(model_path)?);
    let mut o = mf.read_object()?;

    eprintln!("bones:");
    for b in &o.bones {
        if let Some(i) = b.parent {
            eprintln!("  {}, parent = {}", b.name, o.bones[i].name);
        } else {
            eprintln!("  {}", b.name);
        }
    }

    // TODO: read anim.xml as well to get subobject visibility info
    let (anim, anim_ranges) = if let Some(anim_path) = anim_path {
        if anim_path.extension() == Some(OsStr::new("csv")) {
            let ranges = anim_extra::read_anim_csv(anim_path)?;
            let mut af = AnimFile::new(File::open(anim_path.with_extension("anim"))?);
            (Some(af.read_anim()?), ranges)
        } else {
            let mut af = AnimFile::new(File::open(anim_path)?);
            let anim = af.read_anim()?;
            let ranges = vec![AnimRange {
                name: "all".into(),
                start: 0,
                end: anim.frames.len(),
                frame_rate: 15,
            }];
            (Some(anim), ranges)
        }
    } else {
        (None, Vec::new())
    };

    // XXX HACK
    o.models.retain(|m| !m.name.contains("eyes_") || m.name.contains("open"));
    o.models.retain(|m| m.name != "a_rainbowdash_cloud");

    let mut material_images = HashMap::new();
    for m in &o.models {
        eprintln!("model {} material = {}", m.name, m.material);
        if m.material.len() == 0 {
            continue;
        }
        if material_images.contains_key(&m.material) {
            continue;
        }
        let texture_path = model_path.with_file_name(format!("{}.pvr", m.material));
        eprintln!("  load {} from {}", m.material, texture_path.display());
        // TODO: read {name}.rkm and extract DiffuseTexture name
        let mut pf = PvrFile::new(File::open(texture_path)?);
        let img = pf.read_image()?;
        material_images.insert(&m.material, img);
    }


    // Build GLTF

    let mut gltf = GltfBuilder::default();

    let mut model_nodes = Vec::with_capacity(o.models.len());
    for m in &o.models {
        let pos_vec = m.verts.iter().map(|v| v.pos).collect::<Vec<_>>();
        let pos_acc = gltf.push_prim_accessor(&pos_vec, buffer::Target::ArrayBuffer, false);

        let idx_vec = m.tris.iter()
            .flat_map(|t| t.verts.iter())
            .map(|&i| i as u32)
            .collect::<Vec<_>>();
        let idx_acc = gltf.push_prim_accessor(&idx_vec, buffer::Target::ArrayBuffer, false);

        let prim = Primitive {
            indices: Some(idx_acc),
            attributes: vec![
                (Checked::Valid(Semantic::Positions), pos_acc),
            ].into_iter().collect(),
            material: None,
            mode: Checked::Valid(mesh::Mode::Triangles),
            targets: None,
            extensions: None,
            extras: Default::default(),
        };

        let mesh_idx = gltf.push_mesh(Mesh {
            primitives: vec![prim],
            weights: None,
            name: None,
            extensions: None,
            extras: Default::default(),
        });

        let node_idx = gltf.push_node(Node {
            mesh: Some(mesh_idx),
            camera: None,
            children: None,
            matrix: None,
            rotation: None,
            scale: None,
            translation: None,
            skin: None,
            weights: None,
            name: Some(m.name.clone()),
            extensions: None,
            extras: Default::default(),
        });

        model_nodes.push(node_idx);
    }

    let scene_idx = gltf.push_scene(Scene {
        nodes: model_nodes,
        name: None,
        extensions: None,
        extras: Default::default(),
    });
    gltf.set_default_scene(scene_idx);


    // Write output

    let gltf_bytes = gltf.finish();
    fs::write("out.glb", gltf_bytes)?;


    Ok(())
}