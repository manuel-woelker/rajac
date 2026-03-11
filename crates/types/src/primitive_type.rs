#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveType {
    Boolean,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
    Void,
}

impl PrimitiveType {
    pub fn descriptor(&self) -> char {
        match self {
            PrimitiveType::Boolean => 'Z',
            PrimitiveType::Byte => 'B',
            PrimitiveType::Char => 'C',
            PrimitiveType::Short => 'S',
            PrimitiveType::Int => 'I',
            PrimitiveType::Long => 'J',
            PrimitiveType::Float => 'F',
            PrimitiveType::Double => 'D',
            PrimitiveType::Void => 'V',
        }
    }

    pub fn is_integral(&self) -> bool {
        matches!(
            self,
            PrimitiveType::Boolean
                | PrimitiveType::Byte
                | PrimitiveType::Char
                | PrimitiveType::Short
                | PrimitiveType::Int
                | PrimitiveType::Long
        )
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            PrimitiveType::Byte
                | PrimitiveType::Char
                | PrimitiveType::Short
                | PrimitiveType::Int
                | PrimitiveType::Long
                | PrimitiveType::Float
                | PrimitiveType::Double
        )
    }
}
