@0xc72f8d2845c11e17;

interface Coordinator {
  ping @0 () -> ();
}

struct CompileRequest {
  toolchain @0 :Text;
  args @1 :List(Text);
  preprocessed @2: Data;
  precompiled @3: OptionalContent;
}

struct CompileResponse {
  union {
    success @0 :OutputInfo;
    error @1: Error;
  }
}

struct OutputInfo {
  status @0: Int32;
  stdout @1: Data;
  stderr @2: Data;
}

struct OptionalContent {
  hash @0 :Text;
  data @1 :Data;
}

struct Error {
}