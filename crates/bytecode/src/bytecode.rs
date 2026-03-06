

#[cfg(test)]
mod test {
    use std::fs;
    use ristretto_classfile::{ClassAccessFlags, ClassFile, ConstantPool, JAVA_21, JAVA_8};
    use rajac_base::result::FelicoResult;

    #[test]
    fn create_classfile() -> FelicoResult<()>{
        // Create a new class file
        let mut constant_pool = ConstantPool::default();
        let this_class = constant_pool.add_class("HelloWorld")?;
        let super_class = constant_pool.add_class("java/lang/Object")?;

        let class_file = ClassFile {
            version: JAVA_8,
            access_flags: ClassAccessFlags::PUBLIC,
            constant_pool,
            this_class,
            super_class,
            ..Default::default()
        };

        // Verify the class file is valid
        class_file.verify()?;

        // Write the class file to a vector of bytes
        let mut buffer = Vec::new();
        class_file.to_bytes(&mut buffer)?;

        // Now you can save these bytes to a file
        fs::create_dir_all("../../target/classes")?;
        fs::write("../../target/classes/HelloWorld.class", buffer)?;
        Ok(())
    }
}