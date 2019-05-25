use std::borrow::Cow;
use std::sync::Arc;

use influent::client::{Client, ClientWriteResult, Credentials};
use influent::create_client;
use influent::measurement::{Measurement, Value};

pub enum ObjectType {
    Block,
    Transaction,
}

impl Into<&str> for ObjectType {
    fn into(self) -> &'static str {
        match self {
            Block => "block",
            Transaction => "transaction",
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

#[derive(Clone)]
struct Monitor {
    client: Arc<Client>,
}

impl<'a> Monitor {
    pub fn new(credentials: Credentials<'static>, host: &'static str) -> Monitor {
        Monitor {
            client: Arc::new(create_client(credentials, vec![host])),
        }
    }

    pub fn write(
        &self,
        object_id: &str,
        object_type: ObjectType,
        ctype_opt: Option<CompressionType>,
        size: u64,
    ) -> ClientWriteResult {
        let mut measurement = Measurement::new(object_id);
        measurement.add_tag("object_type", Cow::Borrowed(object_type.into()));
        let ctype_str = match ctype_opt {
            Some(ctype) => ctype.into(),
            None => "none",
        };
        measurement.add_tag("compression_type", ctype_str);
        measurement.add_field("size", Value::Integer(size as i64));

        self.client.write_one(measurement, None)
    }
}
