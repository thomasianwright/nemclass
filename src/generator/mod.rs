use crate::field::FieldKind;

mod rust;
pub use rust::*;
mod cpp;
pub use cpp::*;

pub trait Generator {
    fn begin_class(&mut self, name: &str);
    fn end_class(&mut self);

    fn add_field(&mut self, name: &str, kind: FieldKind, metadata: Option<&str>);
    fn add_offset(&mut self, offset: usize);

    fn finilize(&mut self) -> String;
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub enum AvailableGenerator {
    #[default]
    Rust,
    Cpp,
}

impl AvailableGenerator {
    pub const ALL: &[AvailableGenerator] = &[AvailableGenerator::Rust, AvailableGenerator::Cpp];

    pub fn label(&self) -> &'static str {
        match self {
            Self::Rust => "Rust",
            Self::Cpp => "C++",
        }
    }

    pub fn generator(&self) -> Box<dyn Generator> {
        match self {
            Self::Rust => Box::<RustGenerator>::default(),
            Self::Cpp => Box::<CppGenerator>::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CppGenerator, Generator, RustGenerator};
    use crate::{
        class::Class,
        field::{CodegenData, FieldKind, FloatWidth},
    };

    /// Vectors and matrices must generate fixed-array declarations in both backends, with the C++
    /// array declarator placed after the field name.
    #[test]
    fn vector_matrix_codegen() {
        let vec3 = FieldKind::Vector {
            components: 3,
            width: FloatWidth::F32,
        }
        .into_field(Some("pos".into()));
        let mat4 = FieldKind::Matrix {
            rows: 4,
            cols: 4,
            width: FloatWidth::F64,
        }
        .into_field(Some("view".into()));

        let classes: Vec<Class> = vec![];
        let data = CodegenData { classes: &classes };

        let mut rust = RustGenerator::default();
        rust.begin_class("Camera");
        vec3.codegen(&mut rust, &data);
        mat4.codegen(&mut rust, &data);
        rust.end_class();
        let rust_out = rust.finilize();
        assert!(rust_out.contains("pos: [f32; 3]"), "{rust_out}");
        assert!(rust_out.contains("view: [[f64; 4]; 4]"), "{rust_out}");

        let mut cpp = CppGenerator::default();
        cpp.begin_class("Camera");
        vec3.codegen(&mut cpp, &data);
        mat4.codegen(&mut cpp, &data);
        cpp.end_class();
        let cpp_out = cpp.finilize();
        assert!(cpp_out.contains("float pos[3];"), "{cpp_out}");
        assert!(cpp_out.contains("double view[4][4];"), "{cpp_out}");
    }
}
