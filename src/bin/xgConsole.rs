#![allow(non_snake_case)]

use std::error::Error;

mod ib_console;

fn main() -> Result<(), Box<dyn Error>> {
    ib_console::main()
}
