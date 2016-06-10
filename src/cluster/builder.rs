use capnp;
use capnp::message::{Allocator, Builder, ReaderOptions};
use capnp::serialize_packed;

use ::builder_capnp::compile_request;
use ::builder_capnp::optional_content;

use std::io;
use std::io::{BufRead, Write};

#[derive(Debug)]
pub struct CompileRequest {
    pub toolchain: String,
    pub args: Vec<String>,
    pub preprocessed: Vec<u8>,
    pub precompiled: Option<OptionalContent>,
}

#[derive(Debug)]
pub struct OptionalContent {
    hash: String,
    data: Option<Vec<u8>>,
}

impl CompileRequest {
    pub fn read<R: BufRead>(stream: &mut R, options: ReaderOptions) -> Result<Self, capnp::Error> {
        let reader = try!(serialize_packed::read_message(stream, options));
        CompileRequest::from(try!(reader.get_root::<compile_request::Reader>()))
    }

    pub fn write<W: Write, A: Allocator>(&self, stream: &mut W, builder: &mut Builder<A>) -> Result<(), io::Error> {
        self.copy(builder.init_root::<compile_request::Builder>());
        serialize_packed::write_message(stream, builder)
    }


    pub fn from(reader: compile_request::Reader) -> Result<Self, capnp::Error> {
        let args = try!(reader.get_args());
        Ok(CompileRequest {
            toolchain: try!(reader.get_toolchain()).to_string(),
            args: try!((0..args.len())
                .map(|index| args.get(index).map(|value| value.to_string()))
                .collect()),
            preprocessed: try!(reader.get_preprocessed()).to_vec(),
            precompiled: match reader.has_precompiled() {
                true => Some(try!(OptionalContent::from(try!(reader.get_precompiled())))),
                false => None,
            },
        })
    }

    pub fn copy(&self, mut builder: compile_request::Builder) {
        builder.set_toolchain(&self.toolchain);
        builder.set_preprocessed(&self.preprocessed);
        {
            let mut args = builder.borrow().init_args(self.args.len() as u32);
            for index in 0..self.args.len() {
                args.borrow().set(index as u32, &self.args.get(index).unwrap());
            }
        }
        match self.precompiled {
            Some(ref value) => {
                value.copy(builder.borrow().init_precompiled());
            }
            None => {}
        }
    }
}

impl OptionalContent {
    pub fn from(reader: optional_content::Reader) -> Result<Self, capnp::Error> {
        Ok(OptionalContent {
            hash: try!(reader.get_hash()).to_string(),
            data: match reader.has_data() {
                true => Some(try!(reader.get_data()).to_vec()),
                false => None,
            },
        })
    }
    pub fn copy(&self, mut builder: optional_content::Builder) {
        builder.set_hash(&self.hash);
        match self.data {
            Some(ref value) => {
                builder.set_data(&value);
            }
            None => {}
        }
    }
}
