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
    error @1: ErrorInfo;
  }
}

struct OutputInfo {
  status @0: Int32;
  undefined @1: Bool;
  stdout @2: Data;
  stderr @3: Data;
  content @4: Data;
}

struct OptionalContent {
  hash @0 :Text;
  data @1 :Data;
}

struct ErrorInfo {
}