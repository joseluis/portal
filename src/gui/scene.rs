use crate::code_generation::apply_template;
use crate::gui::glsl::*;

use crate::code_generation::*;
use crate::gui::animation::*;
use crate::gui::common::*;
use crate::gui::material::*;
use crate::gui::matrix::*;
use crate::gui::object::*;
use crate::gui::storage::*;
use crate::gui::texture::*;
use crate::gui::uniform::*;
use crate::shader_error_parser::*;

use egui::*;
use glam::*;
use macroquad::prelude::UniformType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CamSettings {
    pub look_at: Vec3,
    pub alpha: f32,
    pub beta: f32,
    pub r: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scene {
    pub description_en: String,
    pub description_ru: String,

    pub cam: CamSettings,

    pub uniforms: StorageWithNames<AnyUniformComboBox>,

    pub matrices: StorageWithNames<MatrixComboBox>,
    objects: StorageWithNames<ObjectComboBox>,

    pub textures: StorageWithNames<TextureName>,

    materials: StorageWithNames<MaterialComboBox>,
    library: StorageWithNames<LibraryCode>,

    user_uniforms: GlobalUserUniforms,
    animation_stages: StorageWithNames<AnimationStage>,

    current_stage: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OldScene {
    pub description_en: String,
    pub description_ru: String,

    pub uniforms: StorageWithNames<AnyUniformComboBox>,

    pub cam: CamSettings,

    pub matrices: StorageWithNames<MatrixComboBox>,
    objects: StorageWithNames<ObjectComboBox>,

    pub textures: StorageWithNames<TextureName>,

    materials: StorageWithNames<MaterialComboBox>,
    library: StorageWithNames<LibraryCode>,

    user_uniforms: GlobalUserUniforms,
    animation_stages: StorageWithNames<AnimationStage>,

    current_stage: usize,
}

impl From<OldScene> for Scene {
    fn from(old: OldScene) -> Scene {
        Scene {
            description_en: old.description_en,
            description_ru: old.description_ru,

            cam: old.cam,

            uniforms: old.uniforms,

            matrices: old.matrices,
            objects: old.objects,

            textures: old.textures,

            materials: old.materials,
            library: old.library,

            user_uniforms: old.user_uniforms,
            animation_stages: old.animation_stages,

            current_stage: old.current_stage,
        }
    }
}

impl Scene {
    pub fn init(&mut self, data: &mut Data) {
        for (_, object) in self.uniforms.iter() {
            if let AnyUniform::Formula(f) = &object.0 {
                data.formulas_cache.compile(&f.0);
            }
        }
        data.errors = Default::default();
        data.show_error_window = false;
        self.user_uniforms
            .uniforms
            .resize(self.uniforms.storage.len(), false);
        self.user_uniforms
            .matrices
            .resize(self.matrices.storage.len(), false);
        drop(self.init_stage(self.current_stage));
    }

    pub fn egui(
        &mut self,
        ui: &mut Ui,
        data: &mut Data,
        should_recompile: &mut bool,
    ) -> (
        WhatChanged,
        Option<Result<macroquad::material::Material, (String, String, ShaderErrors)>>,
    ) {
        let mut changed = WhatChanged::default();
        let mut material = None;

        ui.horizontal(|ui| {
            if ui.button("Export").clicked() {
                let s = serde_json::to_string(self).unwrap();
                data.to_export = Some(s);
            }
            if ui
                .add(Button::new("Recompile").enabled(*should_recompile))
                .clicked()
            {
                match self.get_new_material() {
                    Ok(m) => {
                        data.reload_textures = true;
                        material = Some(Ok(m));
                        *should_recompile = false;
                        changed.uniform = true;
                        data.errors = Default::default();
                        data.show_error_window = false;
                    }
                    Err(err) => {
                        material = Some(Err(err));
                    }
                }
            }
        });

        ui.separator();

        // other ui

        CollapsingHeader::new("Description")
            .default_open(false)
            .show(ui, |ui| {
                CollapsingHeader::new("English")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut data.description_en_edit, false, "View");
                            ui.selectable_value(&mut data.description_en_edit, true, "Edit");
                        });
                        if data.description_en_edit {
                            ui.add(
                                TextEdit::multiline(&mut self.description_en)
                                    .text_style(TextStyle::Monospace),
                            );
                        } else {
                            egui::experimental::easy_mark(ui, &self.description_en);
                        }
                    });
                CollapsingHeader::new("Russian")
                    .default_open(false)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.selectable_value(&mut data.description_ru_edit, false, "View");
                            ui.selectable_value(&mut data.description_ru_edit, true, "Edit");
                        });
                        if data.description_ru_edit {
                            ui.add(
                                TextEdit::multiline(&mut self.description_ru)
                                    .text_style(TextStyle::Monospace),
                            );
                        } else {
                            egui::experimental::easy_mark(ui, &self.description_ru);
                        }
                    });
            });

        changed |= self
            .uniforms
            .rich_egui(ui, &mut data.formulas_cache, "Uniforms");

        if changed.uniform {
            self.user_uniforms
                .uniforms
                .resize(self.uniforms.storage.len(), false);
             self.user_uniforms
                .matrices
                .resize(self.matrices.storage.len(), false);
        }

        ui.collapsing("Calculated uniforms", |ui| {
            for name in self.uniforms.names_iter() {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 0.;
                    ui.label(format!("{} = ", name));
                    use AnyUniformResult::*;
                    match self
                        .uniforms
                        .get(name, &self.uniforms, &data.formulas_cache)
                    {
                        GetEnum::Ok(x) => match x {
                            Bool(b) => ui.label(b.to_string()),
                            Int(b) => ui.label(b.to_string()),
                            Float(b) => ui.label(b.to_string()),
                        },
                        GetEnum::NotFound => ui.label("NotFound"),
                        GetEnum::Recursion => ui.label("Recursion"),
                    }
                });
            }
        });

        with_swapped!(x => (self.uniforms.names, data.matrix_recursion_error);
            changed |= self.matrices.rich_egui(ui, &mut x, "Matrices"));

        with_swapped!(x => (self.matrices.names, data.errors);
            changed |= self.objects.rich_egui(ui, &mut x, "Objects"));

        changed |= self.materials.rich_egui(ui, &mut data.errors, "Materials");

        changed |= self
            .textures
            .rich_egui(ui, &mut data.texture_errors, "Textures");

        changed |= self
            .library
            .rich_egui(ui, &mut data.errors, "User GLSL code");

        ui.collapsing("Global user uniforms", |ui| {
            changed |=
                self.user_uniforms
                    .egui(ui, &mut self.matrices.names, &mut self.uniforms.names);
        });

        with_swapped!(x => (self.matrices, self.uniforms, self.user_uniforms);
            changed |= self
                .animation_stages
                .rich_egui(ui, &mut x, "Animation stages"));

        ui.separator();

        ui.horizontal(|ui| {
            if ui.button("View GLSL library").clicked() {
                data.show_glsl_library = true;
            }
            if ui.button("View generated GLSL code").clicked() {
                data.show_compiled_code = Some(self.generate_shader_code().storage);
            }
        });

        if let Some(local_errors) = data.errors.0.get(&ErrId::default()).cloned() {
            ui.separator();
            ui.horizontal(|ui| {
                ui.label("Other errors:");
                if ui.button("Show full code and errors").clicked() {
                    data.show_error_window = true;
                }
            });
            egui_errors(ui, &local_errors);
        }

        (changed, material)
    }
}

impl Scene {
    pub fn errors_count(&mut self, _: usize, data: &mut Data) -> usize {
        with_swapped!(x => (self.uniforms.names, data.matrix_recursion_error);
            self.matrices.errors_count(0, &mut x))
            + with_swapped!(x => (self.matrices.names, data.errors);
                self.objects.errors_count(0, &mut x))
            + self.materials.errors_count(0, &mut data.errors)
            + self.library.errors_count(0, &mut data.errors)
            + if let Some(local_errors) = data.errors.0.get(&ErrId::default()).cloned() {
                local_errors.len()
            } else {
                0
            }
    }
}

pub trait UniformStruct {
    fn uniforms(&self) -> Vec<(String, UniformType)>;
    fn set_uniforms(&self, material: macroquad::material::Material);
}

impl Scene {
    pub fn textures(&self) -> Vec<String> {
        self.textures
            .names_iter()
            .cloned()
            .map(|x| TextureName::name(&x))
            .collect()
    }

    pub fn uniforms(&self) -> Vec<(String, UniformType)> {
        use Object::*;
        use ObjectType::*;

        let mut result = Vec::new();
        for (_, object) in self.objects.iter() {
            match &object.0 {
                DebugMatrix(matrix) => {
                    result.push(matrix.normal_name());
                    result.push(matrix.inverse_name());
                }
                Flat { kind, is_inside: _ } | Complex { kind, intersect: _ } => match kind {
                    Simple(matrix) => {
                        result.push(matrix.normal_name());
                        result.push(matrix.inverse_name());
                    }
                    Portal(a, b) => {
                        result.push(a.normal_name());
                        result.push(a.inverse_name());
                        result.push(b.normal_name());
                        result.push(b.inverse_name());
                        result.push(a.teleport_to_name(b));
                        if *b != *a {
                            result.push(b.teleport_to_name(a));
                        }
                    }
                },
            }
        }

        let mut result = result
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .map(|name| (name, UniformType::Mat4))
            .collect::<Vec<_>>();

        for (name, uniform) in self.uniforms.iter() {
            let name = format!("{}_u", name);
            match uniform.0 {
                AnyUniform::Bool(_) => result.push((name, UniformType::Int1)),
                AnyUniform::Int { .. } => result.push((name, UniformType::Int1)),
                AnyUniform::Float { .. } => result.push((name, UniformType::Float1)),
                AnyUniform::Angle { .. } => result.push((name, UniformType::Float1)),
                AnyUniform::Formula(_) => result.push((name, UniformType::Float1)),
            }
        }

        result.extend(vec![
            ("_camera".to_owned(), UniformType::Mat4),
            ("_resolution".to_owned(), UniformType::Float2),
            ("_ray_tracing_depth".to_owned(), UniformType::Int1),
            ("_offset_after_material".to_owned(), UniformType::Float1),
            ("_view_angle".to_owned(), UniformType::Float1),
            ("_use_panini_projection".to_owned(), UniformType::Int1),
            ("_panini_param".to_owned(), UniformType::Float1),
        ]);

        result
    }

    pub fn set_uniforms(
        &self,
        material: macroquad::material::Material,
        data: &mut Data,
        uniforms: &StorageWithNames<AnyUniformComboBox>,
    ) {
        data.matrix_recursion_error.0.clear();
        macro_rules! local_try {
            ($a:expr, $c:ident, $b: expr) => {
                match self.matrices.get(&$a.0, uniforms, &data.formulas_cache) {
                    GetEnum::Ok($c) => {
                        *data
                            .matrix_recursion_error
                            .0
                            .entry($a.clone())
                            .or_insert(false) = false;
                        $b
                    }
                    GetEnum::Recursion => {
                        *data
                            .matrix_recursion_error
                            .0
                            .entry($a.clone())
                            .or_insert(false) = true;
                    }
                    _ => {}
                }
            };
        }
        use Object::*;
        use ObjectType::*;
        for (_, object) in self.objects.iter() {
            match &object.0 {
                DebugMatrix(matrix) => {
                    local_try!(matrix, m, {
                        material.set_uniform(&matrix.normal_name(), m);
                        material.set_uniform(&matrix.inverse_name(), m.inverse());
                    })
                }
                Flat { kind, is_inside: _ } | Complex { kind, intersect: _ } => match kind {
                    Simple(matrix) => {
                        local_try!(matrix, m, {
                            material.set_uniform(&matrix.normal_name(), m);
                            material.set_uniform(&matrix.inverse_name(), m.inverse());
                        })
                    }
                    Portal(a, b) => {
                        local_try!(a, ma, {
                            local_try!(b, mb, {
                                material.set_uniform(&a.normal_name(), ma);
                                material.set_uniform(&a.inverse_name(), ma.inverse());
                                material.set_uniform(&b.normal_name(), mb);
                                material.set_uniform(&b.inverse_name(), mb.inverse());
                                material.set_uniform(&a.teleport_to_name(b), mb * ma.inverse());
                                if a != b {
                                    material.set_uniform(&b.teleport_to_name(a), ma * mb.inverse());
                                }
                            })
                        })
                    }
                },
            }
        }

        for name in self.uniforms.names_iter() {
            let name_u = format!("{}_u", name);
            match self.uniforms.get(&name, uniforms, &data.formulas_cache) {
                GetEnum::Ok(result) => match result {
                    AnyUniformResult::Bool(b) => material.set_uniform(&name_u, b as i32),
                    AnyUniformResult::Int(i) => material.set_uniform(&name_u, i),
                    AnyUniformResult::Float(f) => material.set_uniform(&name_u, f as f32),
                },
                _ => {
                    println!("Error getting `{}` uniform", name);
                }
            }
        }
    }
}

impl Scene {
    pub fn generate_shader_code(&self) -> StringStorage {
        let mut storages: BTreeMap<String, StringStorage> = BTreeMap::new();

        storages.insert("uniforms".to_owned(), {
            let mut result = StringStorage::default();
            for (name, kind) in self
                .uniforms()
                .into_iter()
                .filter(|(name, _)| !name.starts_with("_"))
            {
                result.add_string(format!(
                    "uniform {} {};\n",
                    match kind {
                        UniformType::Mat4 => "mat4",
                        UniformType::Float1 => "float",
                        UniformType::Int1 => "int",

                        UniformType::Float2 => unreachable!(),
                        UniformType::Float3 => unreachable!(),
                        UniformType::Float4 => unreachable!(),
                        UniformType::Int2 => unreachable!(),
                        UniformType::Int3 => unreachable!(),
                        UniformType::Int4 => unreachable!(),
                    },
                    name
                ))
            }
            result
        });

        storages.insert("textures".to_owned(), {
            let mut result = StringStorage::default();
            for name in self.textures.names_iter() {
                result.add_string(format!("uniform sampler2D {};\n", TextureName::name(name)));
            }
            result
        });

        let (material_processing, material_defines) = {
            let mut material_processing = StringStorage::default();
            let mut material_defines = StringStorage::default();
            let mut counter = 0;

            use Material::*;
            for (pos, (name, material)) in self.materials.iter().enumerate() {
                let name_m = format!("{}_M", name);

                material_defines.add_string(format!(
                    "#define {} (USER_MATERIAL_OFFSET + {})\n",
                    name_m, counter
                ));
                counter += 1;

                material_processing
                    .add_string(format!("}} else if (i.material == {}) {{\n", name_m));

                match &material.0 {
                    Simple {
                        color,
                        normal_coef,
                        grid,
                        grid_scale,
                        grid_coef,
                    } => {
                        material_processing.add_string(
                            format!(
                                "return material_simple(hit, r, vec3({:e}, {:e}, {:e}), {:e}, {}, {:e}, {:e});\n",
                                color[0], color[1], color[2], normal_coef, grid, grid_scale, grid_coef,
                            )
                        );
                    }
                    Reflect { add_to_color } => {
                        material_processing.add_string(format!(
                            "return material_reflect(hit, r, vec3({:e}, {:e}, {:e}));\n",
                            add_to_color[0], add_to_color[1], add_to_color[2],
                        ));
                    }
                    Refract {
                        refractive_index,
                        add_to_color,
                    } => {
                        material_processing.add_string(format!(
                            "return material_refract(hit, r, vec3({:e}, {:e}, {:e}), {:e});\n",
                            add_to_color[0], add_to_color[1], add_to_color[2], refractive_index,
                        ));
                    }
                    x @ Complex { .. } => {
                        let code = match x {
                            Complex { code } => code,
                            _ => unreachable!(),
                        };
                        material_processing.add_identifier_string(x.identifier(pos), &code.0.0);
                        material_processing.add_string("\n");
                    }
                };
            }
            for (pos, first, second) in
                self.objects
                    .iter()
                    .enumerate()
                    .filter_map(|(pos, (_, x))| match &x.0 {
                        Object::DebugMatrix { .. }
                        | Object::Flat {
                            kind: ObjectType::Simple { .. },
                            ..
                        }
                        | Object::Complex {
                            kind: ObjectType::Simple { .. },
                            ..
                        } => None,
                        Object::Flat {
                            kind: ObjectType::Portal(first, second),
                            ..
                        }
                        | Object::Complex {
                            kind: ObjectType::Portal(first, second),
                            ..
                        } => Some((pos, first, second)),
                    })
            {
                let name_m_1 = format!("teleport_{}_1_M", pos);
                let name_m_2 = format!("teleport_{}_2_M", pos);

                material_defines.add_string(format!(
                    "#define {} (USER_MATERIAL_OFFSET + {})\n",
                    name_m_1, counter
                ));
                counter += 1;
                material_defines.add_string(format!(
                    "#define {} (USER_MATERIAL_OFFSET + {})\n",
                    name_m_2, counter
                ));
                counter += 1;

                material_processing
                    .add_string(format!("}} else if (i.material == {}) {{\n", name_m_1));
                material_processing.add_string(format!(
                    "return material_teleport(hit, r, {});",
                    first.teleport_to_name(second)
                ));

                material_processing
                    .add_string(format!("}} else if (i.material == {}) {{\n", name_m_2));
                material_processing.add_string(format!(
                    "return material_teleport(hit, r, {});",
                    second.teleport_to_name(first)
                ));
            }
            (material_processing, material_defines)
        };

        storages.insert("material_processing".to_owned(), material_processing);
        storages.insert("materials_defines".to_owned(), material_defines);

        storages.insert("intersection_functions".to_owned(), {
            use Object::*;
            use ObjectType::*;
            let mut result = StringStorage::default();

            for (pos, (_, i)) in self.objects.iter().enumerate() {
                match &i.0 {
                    DebugMatrix(_) => {}
                    Flat { kind, is_inside } => {
                        if matches!(kind, Portal { .. }) {
                            result.add_string(format!(
                                "int is_inside_{}(vec4 pos, float x, float y, bool back, bool first) {{\n",
                                pos
                            ));
                        } else {
                            result.add_string(format!("int is_inside_{}(vec4 pos, float x, float y) {{\n", pos));
                        }
                        result.add_identifier_string(i.0.identifier(pos), &is_inside.0.0);
                        result.add_string("\n}\n");
                    }
                    Complex { kind, intersect } => {
                        if matches!(kind, Portal { .. }) {
                            result.add_string(format!(
                                "SceneIntersection intersect_{}(Ray r, bool first) {{\n",
                                pos
                            ));
                        } else {
                            result.add_string(format!("SceneIntersection intersect_{}(Ray r) {{\n", pos));
                        }
                        result.add_identifier_string(i.0.identifier(pos), &intersect.0.0);
                        result.add_string("\n}\n");
                    }
                }
            }
            result
        });

        storages.insert("intersections".to_owned(), {
            use Object::*;
            use ObjectType::*;
            let mut result = StringStorage::default();

            for (pos, (_, i)) in self.objects.iter().enumerate() {
                match &i.0 {
                    DebugMatrix(matrix) => {
                        result.add_string(format!(
                            "transformed_ray = transform({}, r);\nlen = length(transformed_ray.d);\ntransformed_ray.d = normalize(transformed_ray.d);",
                            matrix.inverse_name()
                        ));
                        result.add_string("ihit = debug_intersect(transformed_ray);\nihit.hit.t /= len;\n");
                        result.add_string(format!(
                            "if (nearer(i, ihit)) {{ i = ihit; i.hit.n = normalize(({} * vec4(i.hit.n, 0.)).xyz); }}\n\n",
                            matrix.normal_name()
                        ));
                    }
                    Flat { kind, is_inside: _ } => match kind {
                        Simple(matrix) => {
                            result.add_string(format!(
                                "hit = plane_intersect(r, {}, get_normal({}));\n",
                                matrix.inverse_name(),
                                matrix.normal_name()
                            ));
                            result.add_string(format!(
                                "if (nearer(i, hit)) {{ i = process_plane_intersection(i, hit, is_inside_{}(r.o + r.d * hit.t, hit.u, hit.v)); }}\n\n",
                                pos
                            ));
                        }
                        Portal(a, b) => {
                            let mut add = |matrix: &MatrixName, first, material| {
                                result.add_string(format!(
                                    "normal = {}get_normal({});\n",
                                    if first { "-" } else { "" },
                                    matrix.normal_name()
                                ));
                                result.add_string(format!(
                                    "hit = plane_intersect(r, {}, normal);\n",
                                    matrix.inverse_name()
                                ));
                                result.add_string(format!(
                                    "if (nearer(i, hit)) {{ i = process_portal_intersection(i, hit, is_inside_{}(r.o + r.d * hit.t, hit.u, hit.v, is_collinear(hit.n, normal), {}), {}); }}\n\n",
                                    pos, first, material
                                ));
                            };
                            add(a, true, format!("teleport_{}_1_M", pos));
                            add(b, false, format!("teleport_{}_2_M", pos));
                        }
                    },
                    Complex { kind, intersect: _ } => match kind {
                        Simple(matrix) => {
                            result.add_string(format!(
                                "transformed_ray = transform({}, r);\nlen = length(transformed_ray.d);\ntransformed_ray.d = normalize(transformed_ray.d);",
                                matrix.inverse_name()
                            ));
                            result.add_string(format!(
                                "ihit = intersect_{}(transformed_ray);\nihit.hit.t /= len;\n",
                                pos,
                            ));
                            result.add_string(format!(
                                "if (nearer(i, ihit)) {{ i = ihit; i.hit.n = normalize(({} * vec4(i.hit.n, 0.)).xyz); }}\n\n",
                                matrix.normal_name()
                            ));
                        }
                        Portal(a, b) => {
                            let mut add = |matrix: &MatrixName, first, material| {
                                result.add_string(format!(
                                    "transformed_ray = transform({}, r);\nlen = length(transformed_ray.d);\ntransformed_ray.d = normalize(transformed_ray.d);",
                                    matrix.inverse_name()
                                ));
                                result.add_string(format!(
                                    "ihit = intersect_{}(transformed_ray, {});\nihit.hit.t /= len;\n",
                                    pos, first
                                ));
                                result.add_string(format!(
                                    "if (nearer(i, ihit) && ihit.material != NOT_INSIDE) {{ if (ihit.material == TELEPORT) {{ ihit.material = {}; }} i = ihit; i.hit.n = normalize(({} * vec4(i.hit.n, 0.)).xyz); }}\n\n",
                                    material,
                                    matrix.normal_name()
                                ));
                            };
                            add(a, true, format!("teleport_{}_1_M", pos));
                            add(b, false, format!("teleport_{}_2_M", pos));
                        }
                    },
                }
                result.add_string("\n");
            }
            result
        });

        storages.insert("library".to_owned(), {
            let mut result = StringStorage::default();
            for (pos, (_, i)) in self.library.iter().enumerate() {
                result.add_identifier_string(i.identifier(pos), &i.0.0);
            }
            result
        });

        storages.insert("predefined_library".to_owned(), {
            let mut result = StringStorage::default();
            result.add_string(LIBRARY);
            result
        });

        apply_template(FRAGMENT_SHADER, storages)
    }

    pub fn get_new_material(
        &self,
    ) -> Result<macroquad::prelude::Material, (String, String, ShaderErrors)> {
        let code = self.generate_shader_code();

        use macroquad::prelude::load_material;
        use macroquad::prelude::MaterialParams;

        load_material(
            VERTEX_SHADER,
            &code.storage,
            MaterialParams {
                uniforms: self.uniforms(),
                textures: self.textures(),
                ..Default::default()
            },
        )
        .map_err(|err| {
            let error_message = match err {
                macroquad::prelude::miniquad::graphics::ShaderError::CompilationError {
                    error_message,
                    ..
                } => error_message,
                macroquad::prelude::miniquad::graphics::ShaderError::LinkError(msg) => msg,
                other => {
                    println!("unknown material compilation error: {:?}", other);
                    Default::default()
                }
            };
            let mut errors: BTreeMap<ErrId, Vec<(usize, String)>> = BTreeMap::new();
            for x in shader_error_parser(&error_message) {
                match x {
                    Ok((line_no, message)) => match code.line_numbers.get_identifier(line_no) {
                        Some((identifier, local_line_no)) => {
                            errors
                                .entry(identifier)
                                .or_insert_with(|| Default::default())
                                .push((local_line_no, message.to_owned()));
                        }
                        None => {
                            errors
                                .entry(ErrId::default())
                                .or_insert_with(|| Default::default())
                                .push((line_no, message.to_owned()));
                        }
                    },
                    Err(message) => {
                        errors
                            .entry(ErrId::default())
                            .or_insert_with(|| Default::default())
                            .push((usize::MAX, message.to_owned()));
                    }
                }
            }
            (code.storage, error_message, ShaderErrors(errors))
        })
    }
}

impl Scene {
    fn init_stage(&mut self, stage: usize) -> WhatChanged {
        let mut result = WhatChanged::default();
        if self.animation_stages.storage.len() > 0 {
            for (pos, uniform) in self.animation_stages.storage[stage]
                .uniforms
                .iter()
                .enumerate()
            {
                use Animation::*;
                match uniform {
                    Changed(x) | ChangedAndToUser(x) => {
                        result.uniform |= check_changed(&mut self.uniforms.storage[pos].0, |u| {
                            *u = x.clone();
                        });
                    }
                    ProvidedToUser | Remains => {}
                }
            }
        }
        if self.animation_stages.storage.len() > 0 {
            for (pos, matrix) in self.animation_stages.storage[stage]
                .matrices
                .iter()
                .enumerate()
            {
                use Animation::*;
                match matrix {
                    Changed(x) | ChangedAndToUser(x) => {
                        result.uniform |= check_changed(&mut self.matrices.storage[pos].0, |u| {
                            *u = x.clone();
                        });
                    }
                    ProvidedToUser | Remains => {}
                }
            }
        }
        result
    }

    pub fn control_egui(&mut self, ui: &mut Ui, _: &mut Data) -> WhatChanged {
        let mut result = WhatChanged::default();
        if self.user_uniforms.uniforms.iter().any(|x| *x) {
            for ((uniform, name), _) in self
                .uniforms
                .storage
                .iter_mut()
                .zip(self.uniforms.names.iter())
                .zip(self.user_uniforms.uniforms.iter())
                .filter(|(_, x)| **x)
            {
                ui.horizontal(|ui| {
                    ui.label(name);
                    result |= uniform.0.simple_egui(ui);
                });
            }
            ui.separator();
        }

        if self.user_uniforms.matrices.iter().any(|x| *x) {
            for ((matrix, name), _) in self
                .matrices
                .storage
                .iter_mut()
                .zip(self.matrices.names.iter())
                .zip(self.user_uniforms.matrices.iter())
                .filter(|(_, x)| **x)
            {
                ui.separator();
                ui.label(name);
                result |= matrix.0.simple_egui(ui);
            }
            ui.separator();
        }

        if !self.animation_stages.storage.is_empty() {
            let mut current_stage = self.current_stage;
            result.uniform |= check_changed(&mut current_stage, |stage| {
                let previous = *stage;
                for (pos, name) in self.animation_stages.names.clone().iter().enumerate() {
                    ui.radio_value(stage, pos, name);
                    if *stage != previous && *stage == pos {
                        result |= self.init_stage(*stage);
                    }
                }
            });
            self.current_stage = current_stage;
            if self.current_stage >= self.animation_stages.storage.len() {
                self.current_stage = self.animation_stages.storage.len() - 1;
            }
            ui.separator();
            let uniforms = &mut self.uniforms;
            for (pos, uniform) in self.animation_stages.storage[self.current_stage]
                .uniforms
                .iter()
                .enumerate()
            {
                use Animation::*;
                match uniform {
                    ProvidedToUser | ChangedAndToUser(_) => drop(ui.horizontal(|ui| {
                        ui.label(&uniforms.names[pos]);
                        result |= uniforms.storage[pos].0.simple_egui(ui)
                    })),
                    Remains => {}
                    Changed(_) => {}
                }
            }
            ui.separator();
            let matrices = &mut self.matrices;
            for (pos, matrix) in self.animation_stages.storage[self.current_stage]
                .matrices
                .iter()
                .enumerate()
            {
                use Animation::*;
                match matrix {
                    ProvidedToUser | ChangedAndToUser(_) => {
                        ui.separator();
                        ui.label(&matrices.names[pos]);
                        result |= matrices.storage[pos].0.simple_egui(ui)
                    },
                    Remains => {}
                    Changed(_) => {}
                }
            }
        }
        result
    }
}

const FRAGMENT_SHADER: &'static str = include_str!("../frag.glsl");

pub const LIBRARY: &'static str = include_str!("../library.glsl");

const VERTEX_SHADER: &'static str = "#version 100
attribute vec3 position;
attribute vec2 texcoord;

varying lowp vec2 uv;
varying lowp vec2 uv_screen;

uniform mat4 Model;
uniform mat4 Projection;

uniform vec2 Center;
uniform vec2 _resolution;

void main() {
    vec4 res = Projection * Model * vec4(position, 1);

    uv_screen = (position.xy - _resolution/2.) / min(_resolution.x, _resolution.y) * 2.;
    uv = texcoord;

    gl_Position = res;
}
";
