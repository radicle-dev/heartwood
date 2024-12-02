use std::io::Write;
use serde::Serialize;
use serde_json::Error;

/// Serializes given values to a writer, one value per line.
/// See <https://jsonlines.org/>.
pub fn to_writer_lines<W, I>(mut writer: &mut W, values: I) -> Result<(), Error>
where
    W: Write,
    I: IntoIterator<Item: Serialize>,
{
    for item in values {
	serde_json::to_writer(&mut writer, &item)?;
	writeln!(&mut writer).map_err(Error::io)?;
    }
    Ok(())
}