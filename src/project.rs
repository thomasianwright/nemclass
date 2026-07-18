/// This module contains structures that serialize/deserialize project data(i.e. classes).
use crate::{
    class::{Class, ClassId, ClassList},
    field::{allocate_padding, CodegenData, Field, FieldKind, PointerField},
    generator::Generator,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct DataField {
    name: String,
    offset: usize,
    kind: FieldKind,
    metadata: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct DataClass {
    name: String,
    fields: Vec<DataField>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[non_exhaustive]
pub struct ProjectData {
    classes: Vec<DataClass>,
}

#[derive(Default, Clone)]
struct ProjectDataGenerator {
    classes: Vec<DataClass>,
    offset: usize,
    last_offset: usize,
}

impl Generator for &mut ProjectDataGenerator {
    fn begin_class(&mut self, name: &str) {
        self.classes.push(DataClass {
            name: name.into(),
            fields: vec![],
        });
    }

    fn add_field(&mut self, name: &str, kind: FieldKind, metadata: Option<&str>) {
        let size = kind.size();

        self.classes.last_mut().unwrap().fields.push(DataField {
            metadata: metadata.map(|s| s.to_owned()),
            name: name.to_owned(),
            offset: self.offset,
            kind,
        });

        self.offset += size;
        self.last_offset = self.offset;
    }

    fn add_offset(&mut self, offset: usize) {
        self.offset += offset;
    }

    fn end_class(&mut self) {
        self.offset = 0;
        self.last_offset = 0;
    }

    fn finilize(&mut self) -> String {
        unimplemented!()
    }
}

impl ProjectData {
    pub fn store(classes: &[Class]) -> Self {
        let mut datagen = ProjectDataGenerator::default();
        let dynam = &mut &mut datagen as &mut dyn Generator;
        let data = CodegenData { classes };

        for class in classes {
            dynam.begin_class(&class.name);
            for f in class.fields.iter() {
                f.codegen(dynam, &data);
            }
            dynam.end_class();
        }

        Self {
            classes: datagen.classes,
        }
    }

    pub fn load(self) -> ClassList {
        let mut list = ClassList::EMPTY;

        self.classes
            .iter()
            .for_each(|cl| _ = list.add_empty_class(cl.name.to_string()));

        self.classes.into_iter().for_each(|mut dataclass| {
            dataclass.fields.sort_by_key(|f| f.offset);

            let cid = list.by_name(&dataclass.name).unwrap().id();
            let mut current_offset = 0;

            for DataField {
                offset: field_offset,
                name,
                kind,
                metadata,
            } in dataclass.fields
            {
                let class = list.by_id_mut(cid).unwrap();
                if field_offset > current_offset {
                    class
                        .fields
                        .extend(allocate_padding(field_offset - current_offset));
                }

                match kind {
                    FieldKind::Ptr => {
                        let classname = metadata.as_deref();
                        if let Some(refclass) = classname.and_then(|name| list.by_name(name)) {
                            let refid = refclass.id();
                            let class = list.by_id_mut(cid).unwrap();
                            class
                                .fields
                                .push(Box::new(PointerField::new_with_class_id(name, refid))
                                    as Box<dyn Field>);
                        } else {
                            let new_cid = list.add_class(
                                classname
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| format!("C{:X}", field_offset)),
                            );
                            let class = list.by_id_mut(cid).unwrap();
                            class
                                .fields
                                .push(Box::new(PointerField::new_with_class_id(name, new_cid))
                                    as Box<dyn Field>);
                        }
                    }
                    other => class.fields.push(other.into_field(Some(name))),
                }

                current_offset = field_offset + kind.size();
            }

            if current_offset % 8 != 0 {
                list.by_id_mut(cid)
                    .unwrap()
                    .fields
                    .extend(allocate_padding(8 - (current_offset % 8)));
            }
        });

        list
    }

    pub fn from_str(text: &str) -> Option<Self> {
        ron::from_str(text).ok()
    }

    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        ron::to_string(self).unwrap()
    }
}

/// Serializes `fields` into flat [`DataField`]s (with 0-based offsets), reusing the exact
/// project codegen path so pointer/nested-class metadata is captured. Used for clipboard copy.
pub(crate) fn store_fields<'a>(
    fields: impl IntoIterator<Item = &'a dyn Field>,
    classes: &[Class],
) -> Vec<DataField> {
    let mut datagen = ProjectDataGenerator::default();
    let dynam = &mut &mut datagen as &mut dyn Generator;
    let data = CodegenData { classes };

    dynam.begin_class("_clipboard");
    for f in fields {
        f.codegen(dynam, &data);
    }
    dynam.end_class();

    datagen
        .classes
        .into_iter()
        .next()
        .map(|c| c.fields)
        .unwrap_or_default()
}

/// Reconstructs `fields` (produced by [`store_fields`]) into the class `cid`, inserting them at
/// `pos`. Mirrors [`ProjectData::load`]'s pointer/nested-class handling. Used for clipboard paste.
pub(crate) fn load_fields_into(
    list: &mut ClassList,
    cid: ClassId,
    pos: usize,
    mut fields: Vec<DataField>,
) {
    fields.sort_by_key(|f| f.offset);
    let base = fields.first().map(|f| f.offset).unwrap_or(0);

    let mut built: Vec<Box<dyn Field>> = vec![];
    let mut current_offset = 0;

    for DataField {
        offset,
        name,
        kind,
        metadata,
    } in fields
    {
        let rel = offset - base;
        if rel > current_offset {
            built.extend(allocate_padding(rel - current_offset));
        }

        match kind {
            FieldKind::Ptr => {
                let classname = metadata.as_deref();
                let refid = if let Some(refclass) = classname.and_then(|n| list.by_name(n)) {
                    refclass.id()
                } else {
                    list.add_class(
                        classname
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("C{offset:X}")),
                    )
                };
                built.push(
                    Box::new(PointerField::new_with_class_id(name, refid)) as Box<dyn Field>
                );
            }
            other => built.push(other.into_field(Some(name))),
        }

        current_offset = rel + kind.size();
    }

    if let Some(class) = list.by_id_mut(cid) {
        let insert_at = pos.min(class.fields.len());
        for (i, field) in built.into_iter().enumerate() {
            class.fields.insert(insert_at + i, field);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{load_fields_into, store_fields, DataField};
    use crate::{class::ClassList, field::FieldKind};

    /// Copies typed fields from one class, round-trips them through the same RON path the
    /// clipboard uses, and pastes them into another class — mirroring structural copy/paste.
    #[test]
    fn fields_clipboard_roundtrip() {
        let mut list = ClassList::EMPTY;
        let src = list.add_empty_class("Src".into());
        {
            let class = list.by_id_mut(src).unwrap();
            class.fields.push(FieldKind::F32.into_field(Some("speed".into())));
            class.fields.push(FieldKind::F64.into_field(Some("pos".into())));
            class.fields.push(FieldKind::Bool.into_field(Some("alive".into())));
        }

        // Copy: serialize the source fields, then RON round-trip like `clipboard::write`/`parse`.
        let data = {
            let class = list.by_id(src).unwrap();
            store_fields(class.fields.iter().map(|f| f.as_ref()), list.classes())
        };
        assert_eq!(
            data.iter().map(|f| f.kind).collect::<Vec<_>>(),
            vec![FieldKind::F32, FieldKind::F64, FieldKind::Bool],
        );
        let text = ron::to_string(&data).unwrap();
        let back: Vec<DataField> = ron::from_str(&text).unwrap();

        // Paste into a fresh class and confirm the fields are reconstructed in order.
        let dst = list.add_empty_class("Dst".into());
        load_fields_into(&mut list, dst, 0, back);

        let kinds = list
            .by_id(dst)
            .unwrap()
            .fields
            .iter()
            .map(|f| f.kind())
            .collect::<Vec<_>>();
        assert_eq!(kinds, vec![FieldKind::F32, FieldKind::F64, FieldKind::Bool]);
    }
}
