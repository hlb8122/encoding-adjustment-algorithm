use std::borrow::Cow;

use influent::client::{Client, ClientWriteResult, Credentials, http::HttpClient};
use influent::create_client;
use influent::measurement::{Measurement, Value};

use crate::utils::{CompressionType, ObjectType};

pub struct Monitor {
    client: HttpClient<'static>,
}

impl<'a> Monitor {
    pub fn new(credentials: Credentials<'static>, host: &'static str) -> Monitor {
        Monitor {
            client: create_client(credentials, vec![host]),
        }
    }

    pub fn write(
        &self,
        object_id: &str,
        object_type: ObjectType,
        ctype_opt: Option<CompressionType>,
        size: usize,
    ) -> ClientWriteResult {
        let ctype_str = match ctype_opt {
            Some(ctype) => ctype.into(),
            None => "none",
        };
        let mut measurement = Measurement::new(ctype_str);
        measurement.add_tag("object_type", Cow::Borrowed(object_type.into()));

        measurement.add_tag("id", object_id);
        measurement.add_field("size", Value::Integer(size as i64));

        self.client.write_one(measurement, None)
    }
}
