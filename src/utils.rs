use std::num::ParseIntError;

pub enum ObjectType {
    Block,
    Transaction,
}

impl Into<&str> for ObjectType {
    fn into(self) -> &'static str {
        match self {
            ObjectType::Block => "block",
            ObjectType::Transaction => "transaction",
        }
    }
}

pub enum CompressionType {
    Dict,
    NoDict,
}

impl Into<&'static str> for CompressionType {
    fn into(self) -> &'static str {
        match self {
            CompressionType::Dict => "dict",
            CompressionType::NoDict => "no_dict",
        }
    }
}

pub fn decode_hex(s: &str) -> Result<Vec<u8>, ParseIntError> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect()
}
