use std::borrow::Cow;

use influent::client::{http::HttpClient, Client, Credentials};
use influent::create_client;
use influent::measurement::{Measurement, Value};
use tokio::prelude::*;

use crate::utils::{CompressionType, ObjectType};

pub struct Monitor<'a> {
    client: HttpClient<'a>,
}

impl<'a> Monitor<'a> {
    pub fn new(credentials: Credentials<'a>, host: &'a str) -> Monitor<'a> {
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
    ) {
        let ctype_str = match ctype_opt {
            Some(ctype) => ctype.into(),
            None => "none",
        };
        let mut measurement = Measurement::new(ctype_str);
        measurement.add_tag("object_type", Cow::Borrowed(object_type.into()));

        measurement.add_tag("id", object_id);
        measurement.add_field("size", Value::Integer(size as i64));

        let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
        rt.block_on(
            self.client
                .write_one(measurement, None)
                .then(move |_| self.client.query("select * from \"sut\"".to_string(), None))
                .map_err(|e| println!("{:?}", e)),
        )
        .unwrap();
    }
}
