use std::borrow::Cow;

use influent::client::{http::HttpClient, Client, Credentials};
use influent::create_client;
use influent::measurement::{Measurement, Value};
use tokio::prelude::*;

use crate::utils::ObjectType;

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
        raw_size: usize,
        wo_dict_size: usize,
        w_dict_size: usize
    ) {

        let mut measurement = Measurement::new("compression");
        measurement.add_tag("object_type", Cow::Borrowed(object_type.into()));
        measurement.add_tag("id", Cow::Borrowed(object_id));
        measurement.add_field("raw_size", Value::Integer(raw_size as i64));
        measurement.add_field("wo_dict_size", Value::Integer(wo_dict_size as i64));
        measurement.add_field("w_dict_size", Value::Integer(w_dict_size as i64));

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
