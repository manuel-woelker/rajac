use rajac_base::shared_string::SharedString;
use ristretto_classfile::attributes::Attribute;
use ristretto_classfile::{ClassFile, ConstantPool, Field, Method};

pub fn pretty_print_classfile(class_file: &ClassFile) -> SharedString {
    let mut out = String::new();

    let class_name_internal = class_file
        .constant_pool
        .try_get_class(class_file.this_class)
        .unwrap_or("<invalid:this_class>");
    let super_name_internal = class_file
        .constant_pool
        .try_get_class(class_file.super_class)
        .unwrap_or("<invalid:super_class>");

    let class_name = internal_to_java_name(class_name_internal);
    let super_name = internal_to_java_name(super_name_internal);

    out.push_str(&format!(
        "// version: {}.{} ({})\n",
        class_file.version.major(),
        class_file.version.minor(),
        class_file.version
    ));

    out.push_str(&class_file.access_flags.as_code());
    out.push(' ');
    out.push_str(&class_name);
    if super_name_internal != "java/lang/Object" {
        out.push_str(" extends ");
        out.push_str(&super_name);
    }
    out.push_str(" {\n");

    if !class_file.interfaces.is_empty() {
        out.push_str("  // implements\n");
        for iface in &class_file.interfaces {
            let iface_name = class_file
                .constant_pool
                .try_get_class(*iface)
                .map(internal_to_java_name)
                .unwrap_or_else(|_| "<invalid:interface>".to_string());
            out.push_str(&format!("  // - {}\n", iface_name));
        }
    }

    if !class_file.fields.is_empty() {
        out.push_str("\n  // fields\n");
        for field in &class_file.fields {
            pretty_print_field(&mut out, &class_file.constant_pool, field);
        }
    }

    out.push_str("\n  // methods\n");
    for method in &class_file.methods {
        pretty_print_method(&mut out, &class_file.constant_pool, method);
    }

    if !class_file.attributes.is_empty() {
        out.push_str("\n  // class attributes\n");
        for attribute in &class_file.attributes {
            match attribute {
                Attribute::SourceFile {
                    source_file_index, ..
                } => {
                    let source_file_name = class_file
                        .constant_pool
                        .try_get_utf8(*source_file_index)
                        .unwrap_or("<invalid:source_file>");
                    out.push_str(&format!("  // SourceFile: {}\n", source_file_name));
                }
                Attribute::InnerClasses { classes, .. } => {
                    out.push_str("  // InnerClasses:\n");
                    for entry in classes {
                        let inner_name =
                            resolve_class_name(&class_file.constant_pool, entry.class_info_index);
                        let outer_name = resolve_optional_class_name(
                            &class_file.constant_pool,
                            entry.outer_class_info_index,
                            "<none>",
                        );
                        let inner_simple = resolve_optional_utf8(
                            &class_file.constant_pool,
                            entry.name_index,
                            "<anonymous>",
                        );
                        out.push_str(&format!(
                            "  // - inner: {} outer: {} name: {} flags: {}\n",
                            inner_name, outer_name, inner_simple, entry.access_flags
                        ));
                    }
                }
                Attribute::NestHost {
                    host_class_index, ..
                } => {
                    let host_name =
                        resolve_class_name(&class_file.constant_pool, *host_class_index);
                    out.push_str(&format!("  // NestHost: {}\n", host_name));
                }
                Attribute::NestMembers { class_indexes, .. } => {
                    out.push_str("  // NestMembers:\n");
                    for class_index in class_indexes {
                        let member_name =
                            resolve_class_name(&class_file.constant_pool, *class_index);
                        out.push_str(&format!("  // - {}\n", member_name));
                    }
                }
                _ => {
                    out.push_str("  /* ");
                    out.push_str(&attribute.to_string().replace("\n", "\n  "));
                    out.push_str(" */\n");
                }
            }
        }
    }

    out.push_str("}\n");

    SharedString::from(out)
}

fn internal_to_java_name(internal: &str) -> String {
    internal.replace('/', ".")
}

fn resolve_class_name(constant_pool: &ConstantPool, index: u16) -> String {
    constant_pool
        .try_get_class(index)
        .map(internal_to_java_name)
        .unwrap_or_else(|_| "<invalid:class>".to_string())
}

fn resolve_optional_class_name(
    constant_pool: &ConstantPool,
    index: u16,
    empty_value: &str,
) -> String {
    if index == 0 {
        return empty_value.to_string();
    }
    resolve_class_name(constant_pool, index)
}

fn resolve_optional_utf8(constant_pool: &ConstantPool, index: u16, empty_value: &str) -> String {
    if index == 0 {
        return empty_value.to_string();
    }
    constant_pool
        .try_get_utf8(index)
        .map(|value| value.to_string())
        .unwrap_or_else(|_| "<invalid:utf8>".to_string())
}

fn pretty_print_field(out: &mut String, constant_pool: &ConstantPool, field: &Field) {
    let name = constant_pool
        .try_get_utf8(field.name_index)
        .unwrap_or("<invalid:name>");
    let descriptor = constant_pool
        .try_get_utf8(field.descriptor_index)
        .unwrap_or("<invalid:descriptor>");

    out.push_str(&format!(
        "  {} {} /* {} */;\n",
        field.access_flags.as_code(),
        name,
        descriptor
    ));
}

fn pretty_print_method(out: &mut String, constant_pool: &ConstantPool, method: &Method) {
    let name = constant_pool
        .try_get_utf8(method.name_index)
        .unwrap_or("<invalid:name>");
    let descriptor = constant_pool
        .try_get_utf8(method.descriptor_index)
        .unwrap_or("<invalid:descriptor>");

    out.push_str(&format!(
        "  {} {} /* {} */;\n",
        method.access_flags.as_code(),
        name,
        descriptor
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;
    use rajac_ast::{
        Ast, AstArena, ClassDecl, ClassKind, ClassMember, Field, Ident, Method, Modifiers, Type,
    };
    use ristretto_classfile::attributes::{InnerClass, NestedClassAccessFlags};
    use ristretto_classfile::{ClassAccessFlags, ConstantPool, JAVA_21};

    #[test]
    fn pretty_print_is_java_like_and_includes_details() {
        let mut arena = AstArena::new();
        let mut ast = Ast::new(SharedString::new("test"));

        let int_ty = arena.alloc_type(Type::Primitive(rajac_ast::PrimitiveType::Int));
        let void_ty = arena.alloc_type(Type::Primitive(rajac_ast::PrimitiveType::Void));

        let field = Field {
            name: Ident::new(SharedString::new("x")),
            ty: int_ty,
            initializer: None,
            modifiers: Modifiers(Modifiers::PUBLIC | Modifiers::STATIC | Modifiers::FINAL),
        };
        let method = Method {
            name: Ident::new(SharedString::new("f")),
            params: vec![],
            return_ty: void_ty,
            body: None,
            throws: vec![],
            modifiers: Modifiers(Modifiers::PUBLIC),
        };

        let field_member_id = arena.alloc_class_member(ClassMember::Field(field));
        let method_member_id = arena.alloc_class_member(ClassMember::Method(method));
        let class_id = arena.alloc_class_decl(ClassDecl {
            kind: ClassKind::Interface,
            name: Ident::new(SharedString::new("Foo")),
            type_params: vec![],
            extends: None,
            implements: vec![],
            permits: vec![],
            members: vec![field_member_id, method_member_id],
            modifiers: Modifiers(Modifiers::PUBLIC),
        });
        ast.classes.push(class_id);

        let class_file =
            crate::classfile::classfile_from_class_decl(&ast, &arena, class_id).unwrap();
        class_file.verify().unwrap();

        let printed = pretty_print_classfile(&class_file);
        let printed = printed.as_str();

        expect![[r#"
            // version: 65.0 (Java 21)
            public abstract interface Foo {

              // fields
              public static final x /* I */;

              // methods
              public abstract f /* ()V */;
            }
        "#]]
        .assert_eq(printed);
    }

    #[test]
    fn pretty_prints_inner_classes_and_nesthost_details() {
        let mut constant_pool = ConstantPool::default();
        let outer_class = constant_pool.add_class("p/Outer").unwrap();
        let inner_class = constant_pool.add_class("p/Outer$Inner").unwrap();
        let super_class = constant_pool.add_class("java/lang/Object").unwrap();
        let inner_name = constant_pool.add_utf8("Inner").unwrap();
        let inner_classes_name = constant_pool.add_utf8("InnerClasses").unwrap();
        let nest_host_name = constant_pool.add_utf8("NestHost").unwrap();

        let class_file = ClassFile {
            version: JAVA_21,
            access_flags: ClassAccessFlags::PUBLIC,
            constant_pool,
            this_class: inner_class,
            super_class,
            interfaces: vec![],
            fields: vec![],
            methods: vec![],
            attributes: vec![
                Attribute::InnerClasses {
                    name_index: inner_classes_name,
                    classes: vec![InnerClass {
                        class_info_index: inner_class,
                        outer_class_info_index: outer_class,
                        name_index: inner_name,
                        access_flags: NestedClassAccessFlags::PRIVATE,
                    }],
                },
                Attribute::NestHost {
                    name_index: nest_host_name,
                    host_class_index: outer_class,
                },
            ],
        };

        let printed = pretty_print_classfile(&class_file);
        let printed = printed.as_str();

        expect![[r#"
            // version: 65.0 (Java 21)
            public class p.Outer$Inner {

              // methods

              // class attributes
              // InnerClasses:
              // - inner: p.Outer$Inner outer: p.Outer name: Inner flags: (0x0002) ACC_PRIVATE
              // NestHost: p.Outer
            }
        "#]]
        .assert_eq(printed);
    }
}
