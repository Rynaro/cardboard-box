//! Public skill examples are executable documentation: accept them with the
//! same parser used by `cbox validate` so schema drift fails CI.

use cbox::boxfile::parse_file;

#[test]
fn public_skill_examples_parse_and_validate() {
    for path in [
        "skills/cbox-boxfile/examples/minimal/Boxfile.toml",
        "skills/cbox-boxfile/examples/full/Boxfile.toml",
    ] {
        parse_file(path).unwrap_or_else(|error| panic!("{path} must be parser-valid: {error}"));
    }
}
