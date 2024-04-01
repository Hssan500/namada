use std::collections::BTreeMap;
use std::error::Error;

use namada_sdk::borsh::BorshSchema;
use namada_sdk::tx::Tx;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: tx-schema <schema.txt>");
        return Result::Err("Incorrect command line arguments.".into());
    }
    let mut definitions = BTreeMap::new();
    Tx::add_definitions_recursively(&mut definitions);
    std::fs::write(&args[1], format!("{:#?}", definitions))
        .expect("unable to save schema");
    Ok(())
}
