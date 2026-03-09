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
}
