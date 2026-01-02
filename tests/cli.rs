use assert_cmd::cargo::*;
use predicates::prelude::*;

#[test]
fn empty_address() -> Result<(), Box<dyn std::error::Error>> {
    let mut cmd = cargo_bin_cmd!("otot");

    cmd.arg("open").arg("");
    cmd.assert().failure().stderr(predicate::str::contains(
        "provided address must be a non-empty string",
    ));

    Ok(())
}
