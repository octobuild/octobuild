use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

use petgraph::Graph;

#[test]
fn test_parse_smoke() {
    let f = PathBuf::from(file!())
        .parent()
        .unwrap()
        .join(PathBuf::from("graph-parser.xml"));
    let reader = BufReader::new(File::open(f).unwrap());
    octobuild::xg::parser::parse(&mut Graph::new(), reader).unwrap();
}
