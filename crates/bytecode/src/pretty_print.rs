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
        "  {}{}{} /* {} */;\n",
        method.access_flags.as_code(),
        if name == "<init>" { "" } else { " " },
        name,
        descriptor
    ));

    // Print bytecode if Code attribute is present
    for attribute in &method.attributes {
        if let Attribute::Code {
            max_stack,
            max_locals,
            code,
            exception_table,
            ..
        } = attribute
        {
            out.push_str("    Code:\n");
            out.push_str(&format!("     max_stack = {}\n", max_stack));
            out.push_str(&format!("     max_locals = {}\n", max_locals));

            if !code.is_empty() {
                out.push_str("     Code:\n");
                for (i, instruction) in code.iter().enumerate() {
                    let offset = i;
                    let instruction_str = format_instruction(instruction, constant_pool);
                    out.push_str(&format!("      {}: {}\n", offset, instruction_str));
                }
            }

            if !exception_table.is_empty() {
                out.push_str("     ExceptionTable:\n");
                for (i, exception) in exception_table.iter().enumerate() {
                    out.push_str(&format!(
                        "      {} {} {} {} {}\n",
                        i,
                        exception.range_pc.start,
                        exception.range_pc.end,
                        exception.handler_pc,
                        exception.catch_type
                    ));
                }
            }

            // Print Code attributes (like LineNumberTable, LocalVariableTable)
            // This is a placeholder - we'd need to handle Code sub-attributes
            // but the current structure doesn't expose them directly
        }
    }
}

fn format_instruction(
    instruction: &ristretto_classfile::attributes::Instruction,
    constant_pool: &ConstantPool,
) -> String {
    use ristretto_classfile::attributes::Instruction;

    match instruction {
        Instruction::Nop => "nop".to_string(),
        Instruction::Aconst_null => "aconst_null".to_string(),
        Instruction::Iconst_m1 => "iconst_m1".to_string(),
        Instruction::Iconst_0 => "iconst_0".to_string(),
        Instruction::Iconst_1 => "iconst_1".to_string(),
        Instruction::Iconst_2 => "iconst_2".to_string(),
        Instruction::Iconst_3 => "iconst_3".to_string(),
        Instruction::Iconst_4 => "iconst_4".to_string(),
        Instruction::Iconst_5 => "iconst_5".to_string(),
        Instruction::Lconst_0 => "lconst_0".to_string(),
        Instruction::Lconst_1 => "lconst_1".to_string(),
        Instruction::Fconst_0 => "fconst_0".to_string(),
        Instruction::Fconst_1 => "fconst_1".to_string(),
        Instruction::Fconst_2 => "fconst_2".to_string(),
        Instruction::Dconst_0 => "dconst_0".to_string(),
        Instruction::Dconst_1 => "dconst_1".to_string(),
        Instruction::Bipush(byte) => format!("bipush {}", byte),
        Instruction::Sipush(short) => format!("sipush {}", short),
        Instruction::Ldc(index) => match constant_pool.try_get_utf8(u16::from(*index)) {
            Ok(value) => format!("ldc \"{}\"", value),
            Err(_) => match constant_pool.try_get_string(u16::from(*index)) {
                Ok(value) => format!("ldc \"{}\"", value),
                Err(_) => format!("ldc #{}", index),
            },
        },
        Instruction::Ldc_w(index) => match constant_pool.try_get_utf8(*index) {
            Ok(value) => format!("ldc_w \"{}\"", value),
            Err(_) => match constant_pool.try_get_string(*index) {
                Ok(value) => format!("ldc_w \"{}\"", value),
                Err(_) => format!("ldc_w #{}", index),
            },
        },
        Instruction::Ldc2_w(index) => match constant_pool.try_get_utf8(*index) {
            Ok(value) => format!("ldc2_w \"{}\"", value),
            Err(_) => match constant_pool.try_get_string(*index) {
                Ok(value) => format!("ldc2_w \"{}\"", value),
                Err(_) => format!("ldc2_w #{}", index),
            },
        },
        Instruction::Iload(index) => format!("iload {}", index),
        Instruction::Lload(index) => format!("lload {}", index),
        Instruction::Fload(index) => format!("fload {}", index),
        Instruction::Dload(index) => format!("dload {}", index),
        Instruction::Aload(index) => format!("aload {}", index),
        Instruction::Iload_0 => "iload_0".to_string(),
        Instruction::Iload_1 => "iload_1".to_string(),
        Instruction::Iload_2 => "iload_2".to_string(),
        Instruction::Iload_3 => "iload_3".to_string(),
        Instruction::Lload_0 => "lload_0".to_string(),
        Instruction::Lload_1 => "lload_1".to_string(),
        Instruction::Lload_2 => "lload_2".to_string(),
        Instruction::Lload_3 => "lload_3".to_string(),
        Instruction::Fload_0 => "fload_0".to_string(),
        Instruction::Fload_1 => "fload_1".to_string(),
        Instruction::Fload_2 => "fload_2".to_string(),
        Instruction::Fload_3 => "fload_3".to_string(),
        Instruction::Dload_0 => "dload_0".to_string(),
        Instruction::Dload_1 => "dload_1".to_string(),
        Instruction::Dload_2 => "dload_2".to_string(),
        Instruction::Dload_3 => "dload_3".to_string(),
        Instruction::Aload_0 => "aload_0".to_string(),
        Instruction::Aload_1 => "aload_1".to_string(),
        Instruction::Aload_2 => "aload_2".to_string(),
        Instruction::Aload_3 => "aload_3".to_string(),
        Instruction::Iaload => "iaload".to_string(),
        Instruction::Laload => "laload".to_string(),
        Instruction::Faload => "faload".to_string(),
        Instruction::Daload => "daload".to_string(),
        Instruction::Aaload => "aaload".to_string(),
        Instruction::Baload => "baload".to_string(),
        Instruction::Caload => "caload".to_string(),
        Instruction::Saload => "saload".to_string(),
        Instruction::Istore(index) => format!("istore {}", index),
        Instruction::Lstore(index) => format!("lstore {}", index),
        Instruction::Fstore(index) => format!("fstore {}", index),
        Instruction::Dstore(index) => format!("dstore {}", index),
        Instruction::Astore(index) => format!("astore {}", index),
        Instruction::Istore_0 => "istore_0".to_string(),
        Instruction::Istore_1 => "istore_1".to_string(),
        Instruction::Istore_2 => "istore_2".to_string(),
        Instruction::Istore_3 => "istore_3".to_string(),
        Instruction::Lstore_0 => "lstore_0".to_string(),
        Instruction::Lstore_1 => "lstore_1".to_string(),
        Instruction::Lstore_2 => "lstore_2".to_string(),
        Instruction::Lstore_3 => "lstore_3".to_string(),
        Instruction::Fstore_0 => "fstore_0".to_string(),
        Instruction::Fstore_1 => "fstore_1".to_string(),
        Instruction::Fstore_2 => "fstore_2".to_string(),
        Instruction::Fstore_3 => "fstore_3".to_string(),
        Instruction::Dstore_0 => "dstore_0".to_string(),
        Instruction::Dstore_1 => "dstore_1".to_string(),
        Instruction::Dstore_2 => "dstore_2".to_string(),
        Instruction::Dstore_3 => "dstore_3".to_string(),
        Instruction::Astore_0 => "astore_0".to_string(),
        Instruction::Astore_1 => "astore_1".to_string(),
        Instruction::Astore_2 => "astore_2".to_string(),
        Instruction::Astore_3 => "astore_3".to_string(),
        Instruction::Iastore => "iastore".to_string(),
        Instruction::Lastore => "lastore".to_string(),
        Instruction::Fastore => "fastore".to_string(),
        Instruction::Dastore => "dastore".to_string(),
        Instruction::Aastore => "aastore".to_string(),
        Instruction::Bastore => "bastore".to_string(),
        Instruction::Castore => "castore".to_string(),
        Instruction::Sastore => "sastore".to_string(),
        Instruction::Pop => "pop".to_string(),
        Instruction::Pop2 => "pop2".to_string(),
        Instruction::Dup => "dup".to_string(),
        Instruction::Dup_x1 => "dup_x1".to_string(),
        Instruction::Dup_x2 => "dup_x2".to_string(),
        Instruction::Dup2 => "dup2".to_string(),
        Instruction::Dup2_x1 => "dup2_x1".to_string(),
        Instruction::Dup2_x2 => "dup2_x2".to_string(),
        Instruction::Swap => "swap".to_string(),
        Instruction::Iadd => "iadd".to_string(),
        Instruction::Ladd => "ladd".to_string(),
        Instruction::Fadd => "fadd".to_string(),
        Instruction::Dadd => "dadd".to_string(),
        Instruction::Isub => "isub".to_string(),
        Instruction::Lsub => "lsub".to_string(),
        Instruction::Fsub => "fsub".to_string(),
        Instruction::Dsub => "dsub".to_string(),
        Instruction::Imul => "imul".to_string(),
        Instruction::Lmul => "lmul".to_string(),
        Instruction::Fmul => "fmul".to_string(),
        Instruction::Dmul => "dmul".to_string(),
        Instruction::Idiv => "idiv".to_string(),
        Instruction::Ldiv => "ldiv".to_string(),
        Instruction::Fdiv => "fdiv".to_string(),
        Instruction::Ddiv => "ddiv".to_string(),
        Instruction::Irem => "irem".to_string(),
        Instruction::Lrem => "lrem".to_string(),
        Instruction::Frem => "frem".to_string(),
        Instruction::Drem => "drem".to_string(),
        Instruction::Ineg => "ineg".to_string(),
        Instruction::Lneg => "lneg".to_string(),
        Instruction::Fneg => "fneg".to_string(),
        Instruction::Dneg => "dneg".to_string(),
        Instruction::Ishl => "ishl".to_string(),
        Instruction::Lshl => "lshl".to_string(),
        Instruction::Ishr => "ishr".to_string(),
        Instruction::Lshr => "lshr".to_string(),
        Instruction::Iushr => "iushr".to_string(),
        Instruction::Lushr => "lushr".to_string(),
        Instruction::Iand => "iand".to_string(),
        Instruction::Land => "land".to_string(),
        Instruction::Ior => "ior".to_string(),
        Instruction::Lor => "lor".to_string(),
        Instruction::Ixor => "ixor".to_string(),
        Instruction::Lxor => "lxor".to_string(),
        Instruction::Iinc(index, value) => format!("iinc {} {}", index, value),
        Instruction::I2l => "i2l".to_string(),
        Instruction::I2f => "i2f".to_string(),
        Instruction::I2d => "i2d".to_string(),
        Instruction::L2i => "l2i".to_string(),
        Instruction::L2f => "l2f".to_string(),
        Instruction::L2d => "l2d".to_string(),
        Instruction::F2i => "f2i".to_string(),
        Instruction::F2l => "f2l".to_string(),
        Instruction::F2d => "f2d".to_string(),
        Instruction::D2i => "d2i".to_string(),
        Instruction::D2l => "d2l".to_string(),
        Instruction::D2f => "d2f".to_string(),
        Instruction::I2b => "i2b".to_string(),
        Instruction::I2c => "i2c".to_string(),
        Instruction::I2s => "i2s".to_string(),
        Instruction::Lcmp => "lcmp".to_string(),
        Instruction::Fcmpl => "fcmpl".to_string(),
        Instruction::Fcmpg => "fcmpg".to_string(),
        Instruction::Dcmpl => "dcmpl".to_string(),
        Instruction::Dcmpg => "dcmpg".to_string(),
        Instruction::Ifeq(branch) => format!("ifeq {}", branch),
        Instruction::Ifne(branch) => format!("ifne {}", branch),
        Instruction::Iflt(branch) => format!("iflt {}", branch),
        Instruction::Ifge(branch) => format!("ifge {}", branch),
        Instruction::Ifgt(branch) => format!("ifgt {}", branch),
        Instruction::Ifle(branch) => format!("ifle {}", branch),
        Instruction::If_icmpeq(branch) => format!("if_icmpeq {}", branch),
        Instruction::If_icmpne(branch) => format!("if_icmpne {}", branch),
        Instruction::If_icmplt(branch) => format!("if_icmplt {}", branch),
        Instruction::If_icmpge(branch) => format!("if_icmpge {}", branch),
        Instruction::If_icmpgt(branch) => format!("if_icmpgt {}", branch),
        Instruction::If_icmple(branch) => format!("if_icmple {}", branch),
        Instruction::If_acmpeq(branch) => format!("if_acmpeq {}", branch),
        Instruction::If_acmpne(branch) => format!("if_acmpne {}", branch),
        Instruction::Goto(branch) => format!("goto {}", branch),
        Instruction::Jsr(branch) => format!("jsr {}", branch),
        Instruction::Ret(index) => format!("ret {}", index),
        Instruction::Tableswitch(table_switch) => {
            format!(
                "tableswitch {{ {} to {} }}",
                table_switch.low, table_switch.high
            )
        }
        Instruction::Lookupswitch(lookup_switch) => {
            format!("lookupswitch {{ {} entries }}", lookup_switch.pairs.len())
        }
        Instruction::Ireturn => "ireturn".to_string(),
        Instruction::Lreturn => "lreturn".to_string(),
        Instruction::Freturn => "freturn".to_string(),
        Instruction::Dreturn => "dreturn".to_string(),
        Instruction::Areturn => "areturn".to_string(),
        Instruction::Return => "return".to_string(),
        Instruction::Getstatic(index) => match constant_pool.try_get_field_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "getstatic {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("getstatic #{}", index),
        },
        Instruction::Putstatic(index) => match constant_pool.try_get_field_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "putstatic {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("putstatic #{}", index),
        },
        Instruction::Getfield(index) => match constant_pool.try_get_field_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "getfield {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("getfield #{}", index),
        },
        Instruction::Putfield(index) => match constant_pool.try_get_field_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "putfield {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("putfield #{}", index),
        },
        Instruction::Invokevirtual(index) => match constant_pool.try_get_method_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "invokevirtual {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("invokevirtual #{}", index),
        },
        Instruction::Invokespecial(index) => match constant_pool.try_get_method_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "invokespecial {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("invokespecial #{}", index),
        },
        Instruction::Invokestatic(index) => match constant_pool.try_get_method_ref(*index) {
            Ok((class_index, name_and_type_index)) => {
                let class = constant_pool
                    .try_get_class(*class_index)
                    .unwrap_or("<invalid:class>");
                let (name_index, descriptor_index) = constant_pool
                    .try_get_name_and_type(*name_and_type_index)
                    .unwrap_or((&0, &0));
                let name = constant_pool
                    .try_get_utf8(*name_index)
                    .unwrap_or("<invalid:name>");
                let descriptor = constant_pool
                    .try_get_utf8(*descriptor_index)
                    .unwrap_or("<invalid:descriptor>");
                format!(
                    "invokestatic {}.{}:{}",
                    internal_to_java_name(class),
                    name,
                    descriptor
                )
            }
            Err(_) => format!("invokestatic #{}", index),
        },
        Instruction::Invokeinterface(index, count) => {
            match constant_pool.try_get_interface_method_ref(*index) {
                Ok((class_index, name_and_type_index)) => {
                    let class = constant_pool
                        .try_get_class(*class_index)
                        .unwrap_or("<invalid:class>");
                    let (name_index, descriptor_index) = constant_pool
                        .try_get_name_and_type(*name_and_type_index)
                        .unwrap_or((&0, &0));
                    let name = constant_pool
                        .try_get_utf8(*name_index)
                        .unwrap_or("<invalid:name>");
                    let descriptor = constant_pool
                        .try_get_utf8(*descriptor_index)
                        .unwrap_or("<invalid:descriptor>");
                    format!(
                        "invokeinterface {}.{}:{} {}",
                        internal_to_java_name(class),
                        name,
                        descriptor,
                        count
                    )
                }
                Err(_) => format!("invokeinterface #{} {}", index, count),
            }
        }
        Instruction::Invokedynamic(index) => match constant_pool.try_get_invoke_dynamic(*index) {
            Ok((name, descriptor)) => {
                format!("invokedynamic {}:{}", name, descriptor)
            }
            Err(_) => format!("invokedynamic #{}", index),
        },
        Instruction::New(index) => match constant_pool.try_get_class(*index) {
            Ok(class) => {
                format!("new {}", internal_to_java_name(class))
            }
            Err(_) => format!("new #{}", index),
        },
        Instruction::Newarray(array_type) => {
            let type_name = match array_type {
                ristretto_classfile::attributes::ArrayType::Boolean => "boolean",
                ristretto_classfile::attributes::ArrayType::Char => "char",
                ristretto_classfile::attributes::ArrayType::Float => "float",
                ristretto_classfile::attributes::ArrayType::Double => "double",
                ristretto_classfile::attributes::ArrayType::Byte => "byte",
                ristretto_classfile::attributes::ArrayType::Short => "short",
                ristretto_classfile::attributes::ArrayType::Int => "int",
                ristretto_classfile::attributes::ArrayType::Long => "long",
            };
            format!("newarray {}", type_name)
        }
        Instruction::Anewarray(index) => match constant_pool.try_get_class(*index) {
            Ok(class) => {
                format!("anewarray {}", internal_to_java_name(class))
            }
            Err(_) => format!("anewarray #{}", index),
        },
        Instruction::Arraylength => "arraylength".to_string(),
        Instruction::Athrow => "athrow".to_string(),
        Instruction::Checkcast(index) => match constant_pool.try_get_class(*index) {
            Ok(class) => {
                format!("checkcast {}", internal_to_java_name(class))
            }
            Err(_) => format!("checkcast #{}", index),
        },
        Instruction::Instanceof(index) => match constant_pool.try_get_class(*index) {
            Ok(class) => {
                format!("instanceof {}", internal_to_java_name(class))
            }
            Err(_) => format!("instanceof #{}", index),
        },
        Instruction::Monitorenter => "monitorenter".to_string(),
        Instruction::Monitorexit => "monitorexit".to_string(),
        Instruction::Wide => "wide".to_string(),
        Instruction::Multianewarray(index, dimensions) => {
            match constant_pool.try_get_class(*index) {
                Ok(class) => {
                    format!(
                        "multianewarray {} {}",
                        internal_to_java_name(class),
                        dimensions
                    )
                }
                Err(_) => format!("multianewarray #{} {}", index, dimensions),
            }
        }
        Instruction::Ifnull(branch) => format!("ifnull {}", branch),
        Instruction::Ifnonnull(branch) => format!("ifnonnull {}", branch),
        Instruction::Goto_w(branch) => format!("goto_w {}", branch),
        Instruction::Jsr_w(branch) => format!("jsr_w {}", branch),
        Instruction::Breakpoint => "breakpoint".to_string(),
        Instruction::Impdep1 => "impdep1".to_string(),
        Instruction::Impdep2 => "impdep2".to_string(),
        _ => format!("unknown instruction: {:?}", instruction),
    }
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
