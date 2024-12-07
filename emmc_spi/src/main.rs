mod enums;
mod reader;
mod error;
mod mpsse_ext;
use libftd2xx::Ft4232h;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let device = Ft4232h::with_description("Facet2 FabA+ A")?;

    let mut reader = reader::EmmcReader::new(device);
    reader.init().unwrap();

    Ok(())
}