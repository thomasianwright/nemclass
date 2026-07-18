//! Folder-based project format.
//!
//! A project is a directory containing:
//! - `project.nemproj` — a TOML [`Manifest`] (name + optional [`AutoAttach`]).
//! - `classes/*.toml` — one class layout per file (see [`ClassFile`]).
//! - `scripts/*.lua` — Lua scripts, version-controlled alongside the project.

use crate::error::{Result, SdkError};
use crate::schema::{FieldDef, Project, TypeDef};
use crate::types::FieldKind;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The manifest file name inside a project folder.
pub const MANIFEST_FILE: &str = "project.nemproj";
/// The subfolder holding one TOML file per class.
pub const CLASSES_DIR: &str = "classes";
/// The subfolder holding Lua scripts.
pub const SCRIPTS_DIR: &str = "scripts";

/// Optional auto-attach configuration. When present in the manifest,
/// `process_name` is required; `module_name` is an optional filter that selects,
/// among processes with that name, the instance which has that module loaded
/// (e.g. the Wine process actually running `test.dll`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutoAttach {
    /// Process image name to attach to (e.g. `wine64-preloader`).
    pub process_name: String,
    /// Optional module the target process must have loaded (e.g. `test.dll`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub module_name: Option<String>,
}

/// The `project.nemproj` manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Human-readable project name.
    pub name: String,
    /// Optional auto-attach configuration.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_attach: Option<AutoAttach>,
}

impl Manifest {
    /// A fresh manifest with the given name and no auto-attach.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            auto_attach: None,
        }
    }
}

/// A whole project loaded from disk: manifest, class layouts, and the paths of
/// the scripts found under `scripts/`.
#[derive(Debug, Clone)]
pub struct LoadedProject {
    /// The project directory.
    pub dir: PathBuf,
    /// The parsed manifest.
    pub manifest: Manifest,
    /// The class layouts.
    pub classes: Project,
    /// Paths of `scripts/*.lua`, sorted.
    pub scripts: Vec<PathBuf>,
}

// --- On-disk TOML representation of a class -------------------------------

/// One field, as stored in a class TOML file. `kind` is the compact string form
/// (`"I32"`, `"Vec3f"`, ...); `target` is a pointer's referenced class name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldFile {
    /// Field name.
    pub name: String,
    /// Byte offset from the start of the class.
    pub offset: usize,
    /// Field kind (compact string form).
    pub kind: String,
    /// For pointers, the referenced class name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// A class layout, as stored in `classes/<Name>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassFile {
    /// Class name.
    pub name: String,
    /// Fields (serialized as `[[field]]` tables).
    #[serde(default, rename = "field")]
    pub fields: Vec<FieldFile>,
}

impl ClassFile {
    fn from_type(t: &TypeDef) -> Self {
        Self {
            name: t.name.clone(),
            fields: t
                .fields
                .iter()
                .map(|f| FieldFile {
                    name: f.name.clone(),
                    offset: f.offset,
                    kind: f.kind.to_kind_string(),
                    target: f.metadata.clone(),
                })
                .collect(),
        }
    }

    fn into_type(self) -> Result<TypeDef> {
        let mut fields = Vec::with_capacity(self.fields.len());
        for f in self.fields {
            let kind = FieldKind::from_kind_string(&f.kind)
                .ok_or_else(|| SdkError::Project(format!("unknown field kind `{}`", f.kind)))?;
            fields.push(FieldDef {
                name: f.name,
                offset: f.offset,
                kind,
                metadata: f.target,
            });
        }
        Ok(TypeDef {
            name: self.name,
            fields,
        })
    }
}

/// Replaces characters that are awkward in file names with `_`.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect()
}

// --- Load / save ----------------------------------------------------------

/// Returns `true` if `dir` looks like a project folder (has a manifest).
pub fn is_project_dir(dir: &Path) -> bool {
    dir.join(MANIFEST_FILE).is_file()
}

/// Loads a project from `dir`.
pub fn load_dir(dir: &Path) -> Result<LoadedProject> {
    let manifest_text = fs::read_to_string(dir.join(MANIFEST_FILE))?;
    let manifest: Manifest =
        toml::from_str(&manifest_text).map_err(|e| SdkError::Project(e.to_string()))?;

    let mut classes = Vec::new();
    let classes_dir = dir.join(CLASSES_DIR);
    if classes_dir.is_dir() {
        let mut entries: Vec<PathBuf> = fs::read_dir(&classes_dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        entries.sort();
        for path in entries {
            let text = fs::read_to_string(&path)?;
            let cf: ClassFile =
                toml::from_str(&text).map_err(|e| SdkError::Project(e.to_string()))?;
            classes.push(cf.into_type()?);
        }
    }

    let scripts = list_scripts(dir);

    Ok(LoadedProject {
        dir: dir.to_owned(),
        manifest,
        classes: Project::from_types(classes),
        scripts,
    })
}

/// Lists `scripts/*.lua` under `dir`, sorted.
pub fn list_scripts(dir: &Path) -> Vec<PathBuf> {
    let scripts_dir = dir.join(SCRIPTS_DIR);
    let mut scripts: Vec<PathBuf> = match fs::read_dir(&scripts_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|x| x == "lua"))
            .collect(),
        Err(_) => vec![],
    };
    scripts.sort();
    scripts
}

/// Writes `manifest` + `classes` into `dir`, creating the folder structure if
/// needed. Class TOML files no longer backed by a class are removed, so the
/// folder mirrors the current set of classes.
pub fn save_dir(dir: &Path, manifest: &Manifest, classes: &Project) -> Result<()> {
    let classes_dir = dir.join(CLASSES_DIR);
    fs::create_dir_all(&classes_dir)?;
    fs::create_dir_all(dir.join(SCRIPTS_DIR))?;

    write_manifest(dir, manifest)?;

    // Write each class, tracking file names so stale files can be pruned.
    let mut written = std::collections::HashSet::new();
    for class in &classes.classes {
        let file = format!("{}.toml", sanitize(&class.name));
        let text = toml::to_string(&ClassFile::from_type(class))
            .map_err(|e| SdkError::Project(e.to_string()))?;
        fs::write(classes_dir.join(&file), text)?;
        written.insert(file);
    }

    // Prune class files that no longer correspond to a class.
    if let Ok(rd) = fs::read_dir(&classes_dir) {
        for path in rd.filter_map(|e| e.ok().map(|e| e.path())) {
            let is_toml = path.extension().is_some_and(|x| x == "toml");
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            if is_toml && !written.contains(name) {
                let _ = fs::remove_file(&path);
            }
        }
    }

    Ok(())
}

/// Writes just the manifest file.
pub fn write_manifest(dir: &Path, manifest: &Manifest) -> Result<()> {
    let text = toml::to_string(manifest).map_err(|e| SdkError::Project(e.to_string()))?;
    fs::write(dir.join(MANIFEST_FILE), text)?;
    Ok(())
}

/// Scaffolds a new empty project at `dir` (manifest + empty `classes/`/`scripts/`).
pub fn create_dir(dir: &Path, name: &str) -> Result<LoadedProject> {
    fs::create_dir_all(dir.join(CLASSES_DIR))?;
    fs::create_dir_all(dir.join(SCRIPTS_DIR))?;
    let manifest = Manifest::new(name);
    write_manifest(dir, &manifest)?;
    Ok(LoadedProject {
        dir: dir.to_owned(),
        manifest,
        classes: Project::new(),
        scripts: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::TypeBuilder;
    use crate::types::FloatWidth;

    fn sample_classes() -> Project {
        let mut p = Project::new();
        let mut b = TypeBuilder::new("Player");
        b.field("health", FieldKind::I32)
            .field("pos", FieldKind::Vector { components: 3, width: FloatWidth::F32 })
            .field_with_meta("target", FieldKind::Ptr, Some("Enemy".into()));
        p.push(b.build());
        p.push(TypeBuilder::new("Enemy").build());
        p
    }

    #[test]
    fn class_file_roundtrip_toml() {
        let classes = sample_classes();
        let player = &classes.classes[0];
        let toml_text = toml::to_string(&ClassFile::from_type(player)).unwrap();
        assert!(toml_text.contains("kind = \"Vec3f\""), "{toml_text}");
        assert!(toml_text.contains("target = \"Enemy\""), "{toml_text}");

        let back: ClassFile = toml::from_str(&toml_text).unwrap();
        let ty = back.into_type().unwrap();
        assert_eq!(&ty, player);
    }

    #[test]
    fn save_load_roundtrip_and_prune() {
        let dir = std::env::temp_dir().join("nemclass_sdk_roundtrip_test");
        let _ = fs::remove_dir_all(&dir);

        let mut manifest = Manifest::new("roundtrip");
        manifest.auto_attach = Some(AutoAttach {
            process_name: "wine64-preloader".into(),
            module_name: Some("test.dll".into()),
        });
        let classes = sample_classes();

        save_dir(&dir, &manifest, &classes).unwrap();
        assert!(is_project_dir(&dir));

        let loaded = load_dir(&dir).unwrap();
        assert_eq!(loaded.manifest, manifest);
        // Classes come back sorted by file name, so compare order-independently.
        let sort = |p: &Project| {
            let mut v = p.classes.clone();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };
        assert_eq!(sort(&loaded.classes), sort(&classes));

        // Saving with a class removed prunes its file.
        let only_player = Project::from_types(vec![classes.classes[0].clone()]);
        save_dir(&dir, &manifest, &only_player).unwrap();
        assert!(!dir.join(CLASSES_DIR).join("Enemy.toml").exists());
        assert!(dir.join(CLASSES_DIR).join("Player.toml").exists());

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_toml_shape() {
        let mut m = Manifest::new("proj");
        m.auto_attach = Some(AutoAttach {
            process_name: "wine64-preloader".into(),
            module_name: Some("test.dll".into()),
        });
        let text = toml::to_string(&m).unwrap();
        assert!(text.contains("name = \"proj\""), "{text}");
        assert!(text.contains("[auto_attach]"), "{text}");
        assert_eq!(toml::from_str::<Manifest>(&text).unwrap(), m);

        // module_name omitted when absent.
        let m2 = Manifest::new("bare");
        let text2 = toml::to_string(&m2).unwrap();
        assert!(!text2.contains("auto_attach"), "{text2}");
    }
}
