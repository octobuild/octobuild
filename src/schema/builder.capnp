@0xc72f8d2845c11e17;

interface Coordinator {
  ping @0 () -> ();
}

struct CompileRequest {
  toolchain @0 :Text;
  args @1 :List(Text);
  precompiledHash @2: Text;
}

struct SourceRequest {
  body @0 :Text;
}
