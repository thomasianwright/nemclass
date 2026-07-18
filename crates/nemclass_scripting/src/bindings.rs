//! Lua bindings: registers the global `nem` table and the userdata types that
//! wrap [`nemclass_sdk`] values.

use mlua::{Lua, Result as LuaResult, Table, UserData, UserDataMethods, UserDataRef};
use nemclass_sdk::{
    generate, infer, offset, target, FieldKind, FloatWidth, Lang, Pattern, Project, Target,
    TypeBuilder, TypeDef,
};
use std::cell::RefCell;
use std::path::Path;

/// Turn any SDK error into a Lua error.
fn ext(e: nemclass_sdk::SdkError) -> mlua::Error {
    mlua::Error::external(e)
}

/// Error for a failed memory access at `addr`.
fn access(addr: usize) -> mlua::Error {
    ext(nemclass_sdk::SdkError::Access { address: addr })
}

fn parse_width(s: &str) -> LuaResult<FloatWidth> {
    match s.to_ascii_lowercase().as_str() {
        "f32" | "float" => Ok(FloatWidth::F32),
        "f64" | "double" => Ok(FloatWidth::F64),
        other => Err(ext(nemclass_sdk::SdkError::Expr(format!(
            "unknown float width `{other}` (want f32/f64)"
        )))),
    }
}

fn lang(s: &str) -> LuaResult<Lang> {
    Lang::parse(s).ok_or_else(|| {
        ext(nemclass_sdk::SdkError::Project(format!(
            "unknown language `{s}` (want rust/cpp)"
        )))
    })
}

fn module_table(lua: &Lua, m: &target::ModuleInfo) -> LuaResult<Table> {
    let t = lua.create_table()?;
    t.set("base", m.base as i64)?;
    t.set("size", m.size as i64)?;
    t.set("name", m.name.clone())?;
    Ok(t)
}

// ---------------------------------------------------------------------------
// Userdata wrappers
// ---------------------------------------------------------------------------

struct TargetUd(Target);
struct PatternUd(Pattern);
struct KindUd(FieldKind);
struct ClassBuilderUd(RefCell<Option<TypeBuilder>>);
struct TypeUd(TypeDef);
struct ProjectUd(RefCell<Project>);

impl UserData for PatternUd {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("len", |_, this, ()| Ok(this.0.len() as i64));
        m.add_method("is_empty", |_, this, ()| Ok(this.0.is_empty()));
    }
}

impl UserData for KindUd {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("name", |_, this, ()| Ok(this.0.display_name().into_owned()));
        m.add_method("size", |_, this, ()| Ok(this.0.size() as i64));
    }
}

impl UserData for TypeUd {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("name", |_, this, ()| Ok(this.0.name.clone()));
        m.add_method("to_ron", |_, this, ()| {
            Project::from_types(vec![this.0.clone()]).to_ron().map_err(ext)
        });
        m.add_method("generate", |_, this, l: String| {
            Ok(generate(&Project::from_types(vec![this.0.clone()]), lang(&l)?))
        });
    }
}

impl UserData for ProjectUd {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("add", |_, this, ty: UserDataRef<TypeUd>| {
            this.0.borrow_mut().classes.push(ty.0.clone());
            Ok(())
        });
        m.add_method("to_ron", |_, this, ()| this.0.borrow().to_ron().map_err(ext));
        m.add_method("generate", |_, this, l: String| {
            Ok(generate(&this.0.borrow(), lang(&l)?))
        });
    }
}

impl UserData for ClassBuilderUd {
    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("field", |_, this, (name, kind): (String, UserDataRef<KindUd>)| {
            if let Some(b) = this.0.borrow_mut().as_mut() {
                b.field(name, kind.0);
            }
            Ok(())
        });
        m.add_method(
            "field_at",
            |_, this, (name, kind, off): (String, UserDataRef<KindUd>, i64)| {
                if let Some(b) = this.0.borrow_mut().as_mut() {
                    b.field_at(name, kind.0, off as usize);
                }
                Ok(())
            },
        );
        m.add_method("pad", |_, this, bytes: i64| {
            if let Some(b) = this.0.borrow_mut().as_mut() {
                b.pad(bytes as usize);
            }
            Ok(())
        });
        m.add_method("build", |_, this, ()| {
            this.0
                .borrow_mut()
                .take()
                .map(|b| TypeUd(b.build()))
                .ok_or_else(|| {
                    ext(nemclass_sdk::SdkError::Project(
                        "class builder already built".into(),
                    ))
                })
        });
    }
}

impl UserData for TargetUd {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // Typed reads. `read_<t>(addr) -> value`, erroring if the address is unreadable.
        macro_rules! read_int {
            ($name:literal, $t:ty) => {
                methods.add_method($name, |_, this, addr: i64| {
                    this.0
                        .read_pod::<$t>(addr as usize)
                        .map(|v| v as i64)
                        .ok_or_else(|| access(addr as usize))
                });
            };
        }
        macro_rules! read_float {
            ($name:literal, $t:ty) => {
                methods.add_method($name, |_, this, addr: i64| {
                    this.0
                        .read_pod::<$t>(addr as usize)
                        .map(|v| v as f64)
                        .ok_or_else(|| access(addr as usize))
                });
            };
        }
        read_int!("read_i8", i8);
        read_int!("read_i16", i16);
        read_int!("read_i32", i32);
        read_int!("read_i64", i64);
        read_int!("read_u8", u8);
        read_int!("read_u16", u16);
        read_int!("read_u32", u32);
        read_int!("read_u64", u64);
        read_float!("read_f32", f32);
        read_float!("read_f64", f64);

        methods.add_method("read_ptr", |_, this, addr: i64| {
            this.0
                .read_ptr(addr as usize)
                .map(|v| v as i64)
                .ok_or_else(|| access(addr as usize))
        });
        methods.add_method("read_bytes", |lua, this, (addr, len): (i64, i64)| {
            let bytes = this
                .0
                .read_bytes(addr as usize, len as usize)
                .ok_or_else(|| access(addr as usize))?;
            lua.create_string(&bytes)
        });
        methods.add_method("read_string", |_, this, addr: i64| {
            this.0
                .read_string(addr as usize)
                .ok_or_else(|| access(addr as usize))
        });

        // Typed writes.
        macro_rules! write_int {
            ($name:literal, $t:ty) => {
                methods.add_method($name, |_, this, (addr, val): (i64, i64)| {
                    let ok = this.0.write(addr as usize, &(val as $t).to_ne_bytes());
                    ok.then_some(()).ok_or_else(|| access(addr as usize))
                });
            };
        }
        macro_rules! write_float {
            ($name:literal, $t:ty) => {
                methods.add_method($name, |_, this, (addr, val): (i64, f64)| {
                    let ok = this.0.write(addr as usize, &(val as $t).to_ne_bytes());
                    ok.then_some(()).ok_or_else(|| access(addr as usize))
                });
            };
        }
        write_int!("write_i8", i8);
        write_int!("write_i16", i16);
        write_int!("write_i32", i32);
        write_int!("write_i64", i64);
        write_int!("write_u8", u8);
        write_int!("write_u16", u16);
        write_int!("write_u32", u32);
        write_int!("write_u64", u64);
        write_float!("write_f32", f32);
        write_float!("write_f64", f64);
        methods.add_method("write_bytes", |_, this, (addr, data): (i64, mlua::LuaString)| {
            let bytes = data.as_bytes().to_vec();
            let ok = this.0.write(addr as usize, &bytes);
            ok.then_some(()).ok_or_else(|| access(addr as usize))
        });

        methods.add_method("can_read", |_, this, addr: i64| {
            Ok(this.0.can_read(addr as usize))
        });
        methods.add_method("id", |_, this, ()| Ok(this.0.id() as i64));
        methods.add_method("name", |_, this, ()| Ok(this.0.name()));
        methods.add_method("pointer_size", |_, this, ()| Ok(this.0.pointer_size() as i64));
        methods.add_method("is_wine", |_, this, ()| Ok(this.0.is_wine()));

        methods.add_method("modules", |lua, this, ()| {
            let mods = this.0.modules().map_err(ext)?;
            let out = lua.create_table()?;
            for (i, m) in mods.iter().enumerate() {
                out.set(i + 1, module_table(lua, m)?)?;
            }
            Ok(out)
        });
        methods.add_method("module", |lua, this, name: String| {
            let m = this.0.module(&name).map_err(ext)?;
            module_table(lua, &m)
        });

        // Pattern scanning.
        methods.add_method(
            "scan_range",
            |_, this, (pat, start, len): (UserDataRef<PatternUd>, i64, i64)| {
                Ok(this
                    .0
                    .scan_range(&pat.0, start as usize, len as usize)
                    .into_iter()
                    .map(|a| a as i64)
                    .collect::<Vec<_>>())
            },
        );
        methods.add_method(
            "scan_module",
            |_, this, (pat, name): (UserDataRef<PatternUd>, String)| {
                Ok(this
                    .0
                    .scan_module(&pat.0, &name)
                    .map_err(ext)?
                    .into_iter()
                    .map(|a| a as i64)
                    .collect::<Vec<_>>())
            },
        );
        methods.add_method(
            "scan",
            |_, this, (pat, opts): (UserDataRef<PatternUd>, Option<Table>)| {
                let addrs = match opts {
                    Some(opts) => {
                        if let Some(module) = opts.get::<Option<String>>("module")? {
                            this.0.scan_module(&pat.0, &module).map_err(ext)?
                        } else {
                            let start: i64 = opts.get("start")?;
                            let len: i64 = opts.get("len")?;
                            this.0.scan_range(&pat.0, start as usize, len as usize)
                        }
                    }
                    None => {
                        return Err(ext(nemclass_sdk::SdkError::Expr(
                            "scan expects opts { module = .. } or { start = .., len = .. }".into(),
                        )))
                    }
                };
                Ok(addrs.into_iter().map(|a| a as i64).collect::<Vec<_>>())
            },
        );

        // Offset / address resolution.
        methods.add_method("resolve", |_, this, (base, offs): (i64, Vec<i64>)| {
            let offs: Vec<usize> = offs.into_iter().map(|o| o as usize).collect();
            offset::resolve(&this.0, base as usize, &offs)
                .map(|a| a as i64)
                .ok_or_else(|| access(base as usize))
        });
        methods.add_method("rip", |_, this, (rel, next): (i64, i64)| {
            offset::rip_relative(&this.0, rel as usize, next as usize)
                .map(|a| a as i64)
                .ok_or_else(|| access(rel as usize))
        });
        methods.add_method("eval", |_, this, expr: String| {
            offset::eval(&this.0, &expr).map(|a| a as i64).map_err(ext)
        });

        // Type inference: best-guess kind name at `addr`, or nil.
        methods.add_method("infer", |_, this, addr: i64| {
            let bytes = infer::read_window(&this.0, addr as usize);
            Ok(infer::infer_kind(&bytes, &this.0)
                .first()
                .map(|k| k.display_name().into_owned()))
        });
    }
}

// ---------------------------------------------------------------------------
// Registration
// ---------------------------------------------------------------------------

/// Installs the global `nem` table into `lua`.
pub fn register(lua: &Lua) -> LuaResult<()> {
    let nem = lua.create_table()?;

    nem.set(
        "processes",
        lua.create_function(|lua, ()| {
            let procs = target::processes().map_err(ext)?;
            let out = lua.create_table()?;
            for (i, p) in procs.iter().enumerate() {
                let t = lua.create_table()?;
                t.set("id", p.id as i64)?;
                t.set("name", p.name.clone())?;
                t.set("parent_id", p.parent_id as i64)?;
                out.set(i + 1, t)?;
            }
            Ok(out)
        })?,
    )?;

    nem.set(
        "attach",
        lua.create_function(|_, opts: Table| {
            let plugin: Option<String> = opts.get("plugin")?;
            let target = if let Some(pid) = opts.get::<Option<u32>>("pid")? {
                match plugin {
                    Some(p) => Target::attach_managed(pid, Path::new(&p)),
                    None => Target::attach_pid(pid),
                }
            } else if let Some(name) = opts.get::<Option<String>>("name")? {
                Target::attach_name(&name)
            } else {
                return Err(ext(nemclass_sdk::SdkError::Expr(
                    "attach expects { pid = .. } or { name = .. }".into(),
                )));
            };
            target.map(TargetUd).map_err(ext)
        })?,
    )?;

    nem.set(
        "open",
        lua.create_function(|_, pid: u32| Target::attach_pid(pid).map(TargetUd).map_err(ext))?,
    )?;

    nem.set(
        "pattern",
        lua.create_function(|_, (sig, style): (String, Option<String>)| {
            let p = match style.as_deref() {
                Some("peid") => Pattern::peid(&sig),
                _ => Pattern::ida(&sig),
            };
            p.map(PatternUd).map_err(ext)
        })?,
    )?;
    nem.set(
        "pattern_code",
        lua.create_function(|_, (bytes, mask): (mlua::LuaString, String)| {
            Pattern::code(&bytes.as_bytes(), &mask)
                .map(PatternUd)
                .map_err(ext)
        })?,
    )?;

    nem.set(
        "class",
        lua.create_function(|_, name: String| {
            Ok(ClassBuilderUd(RefCell::new(Some(TypeBuilder::new(name)))))
        })?,
    )?;
    nem.set(
        "project",
        lua.create_function(|_, ()| Ok(ProjectUd(RefCell::new(Project::new()))))?,
    )?;
    nem.set(
        "load_project",
        lua.create_function(|_, ron: String| {
            Project::from_ron(&ron)
                .map(|p| ProjectUd(RefCell::new(p)))
                .map_err(ext)
        })?,
    )?;
    nem.set(
        "generate",
        lua.create_function(|_, (proj, l): (UserDataRef<ProjectUd>, String)| {
            Ok(generate(&proj.0.borrow(), lang(&l)?))
        })?,
    )?;

    // Field-kind constructors.
    let kinds = lua.create_table()?;
    for (name, kind) in [
        ("i8", FieldKind::I8),
        ("i16", FieldKind::I16),
        ("i32", FieldKind::I32),
        ("i64", FieldKind::I64),
        ("u8", FieldKind::U8),
        ("u16", FieldKind::U16),
        ("u32", FieldKind::U32),
        ("u64", FieldKind::U64),
        ("f32", FieldKind::F32),
        ("f64", FieldKind::F64),
        ("bool", FieldKind::Bool),
        ("ptr", FieldKind::Ptr),
        ("strptr", FieldKind::StrPtr),
        ("hex8", FieldKind::Unk8),
        ("hex16", FieldKind::Unk16),
        ("hex32", FieldKind::Unk32),
        ("hex64", FieldKind::Unk64),
    ] {
        kinds.set(name, KindUd(kind))?;
    }
    kinds.set(
        "vec",
        lua.create_function(|_, (components, width): (u8, String)| {
            Ok(KindUd(FieldKind::Vector {
                components,
                width: parse_width(&width)?,
            }))
        })?,
    )?;
    kinds.set(
        "mat",
        lua.create_function(|_, (rows, cols, width): (u8, u8, String)| {
            Ok(KindUd(FieldKind::Matrix {
                rows,
                cols,
                width: parse_width(&width)?,
            }))
        })?,
    )?;
    nem.set("kinds", kinds)?;

    lua.globals().set("nem", nem)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::ScriptEngine;

    #[test]
    fn nem_api_is_registered() {
        let engine = ScriptEngine::new().unwrap();
        // Build a type declaration and generate Rust from it — no target needed.
        let out: String = engine
            .eval_str(
                r#"
                local c = nem.class("Player")
                c:field("health", nem.kinds.i32)
                c:field("pos", nem.kinds.vec(3, "f32"))
                local p = nem.project()
                p:add(c:build())
                return p:generate("rust")
            "#,
            )
            .unwrap();
        assert!(out.contains("pub struct Player"), "{out}");
        assert!(out.contains("pub health: i32"), "{out}");
        assert!(out.contains("pub pos: [f32; 3]"), "{out}");
    }

    #[test]
    fn pattern_parsing_errors_surface() {
        let engine = ScriptEngine::new().unwrap();
        assert!(engine.run_str(r#"nem.pattern("zz zz")"#).is_err());
        // A valid pattern builds fine.
        engine.run_str(r#"assert(nem.pattern("48 8B ?? 33"):len() == 4)"#).unwrap();
    }
}
