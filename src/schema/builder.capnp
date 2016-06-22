@0xc72f8d2845c11e17;

interface Coordinator {
  ping @0 () -> ();
}

struct CompileRequest {
  toolchain @0 :Text;
  args @1 :List(Text);
  preprocessedData @2: Data;
  precompiledHash @3: Text;
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

struct ErrorInfo {
}