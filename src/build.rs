fn main() {
    capnpc::CompilerCommand::new()
        .src_prefix("src/schema")
        .file("src/schema/builder.capnp")
        .run()
        .unwrap();
}
