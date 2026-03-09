#[cfg(test)]
mod test {
    use rajac_base::result::RajacResult;
    use ristretto_classfile::attributes::Attribute;
    use ristretto_classfile::{
        ClassAccessFlags, ClassFile, ConstantPool, Field, FieldAccessFlags, FieldType, JAVA_8,
        Method, MethodAccessFlags,
    };
    use std::fs;

    #[test]
    fn create_classfile() -> RajacResult<()> {
        let mut constant_pool = ConstantPool::default();
        let this_class = constant_pool.add_class("HelloWorld")?;
        let super_class = constant_pool.add_class("java/lang/Object")?;

        let main_name = constant_pool.add_utf8("main")?;
        let main_descriptor = constant_pool.add_utf8("([Ljava/lang/String;)V")?;
        let code_name = constant_pool.add_utf8("Code")?;

        let system_class = constant_pool.add_class("java/lang/System")?;
        let print_stream_class = constant_pool.add_class("java/io/PrintStream")?;

        let out_index =
            constant_pool.add_field_ref(system_class, "out", "Ljava/io/PrintStream;")?;
        let hello_world_index = constant_pool.add_string("Hello world!!!")?;
        let println_index =
            constant_pool.add_method_ref(print_stream_class, "println", "(Ljava/lang/String;)V")?;

        let out_name = constant_pool.add_utf8("out")?;
        let out_descriptor = constant_pool.add_utf8("Ljava/io/PrintStream;")?;

        let static_field = Field {
            access_flags: FieldAccessFlags::PUBLIC | FieldAccessFlags::STATIC,
            name_index: out_name,
            descriptor_index: out_descriptor,
            field_type: FieldType::parse("Ljava/io/PrintStream;").unwrap(),
            attributes: vec![],
        };

        let code_attribute = Attribute::Code {
            name_index: code_name,
            max_stack: 2,
            max_locals: 1,
            code: vec![
                ristretto_classfile::attributes::Instruction::Getstatic(out_index),
                ristretto_classfile::attributes::Instruction::Ldc_w(hello_world_index),
                ristretto_classfile::attributes::Instruction::Invokevirtual(println_index),
                ristretto_classfile::attributes::Instruction::Return,
            ],
            exception_table: vec![],
            attributes: vec![],
        };

        let main_method = Method {
            access_flags: MethodAccessFlags::PUBLIC | MethodAccessFlags::STATIC,
            name_index: main_name,
            descriptor_index: main_descriptor,
            attributes: vec![code_attribute],
        };

        let class_file = ClassFile {
            version: JAVA_8,
            access_flags: ClassAccessFlags::PUBLIC,
            constant_pool,
            this_class,
            super_class,
            fields: vec![static_field],
            methods: vec![main_method],
            ..Default::default()
        };

        class_file.verify()?;

        let mut buffer = Vec::new();
        class_file.to_bytes(&mut buffer)?;

        fs::create_dir_all("../../target/classes")?;
        fs::write("../../target/classes/HelloWorld.class", buffer)?;
        Ok(())
    }
}
