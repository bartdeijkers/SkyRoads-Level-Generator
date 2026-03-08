use crate::{Error, Result};

struct BitReader<'a> {
    data: &'a [u8],
    byte_offset: usize,
    bit_offset: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8], offset: usize) -> Self {
        Self {
            data,
            byte_offset: offset,
            bit_offset: 0,
        }
    }

    fn read_bits(&mut self, count: u8) -> Result<u32> {
        let mut value = 0_u32;
        for _ in 0..count {
            if self.byte_offset >= self.data.len() {
                return Err(Error::UnexpectedEof("compressed bitstream"));
            }
            let bit = (self.data[self.byte_offset] >> (7 - self.bit_offset)) & 1;
            value = (value << 1) | u32::from(bit);
            self.bit_offset += 1;
            if self.bit_offset == 8 {
                self.bit_offset = 0;
                self.byte_offset += 1;
            }
        }
        Ok(value)
    }

    fn bytes_consumed(&self, start_offset: usize) -> usize {
        let extra = usize::from(self.bit_offset != 0);
        (self.byte_offset + extra) - start_offset
    }
}

fn copy_from_history(
    output: &mut Vec<u8>,
    distance: usize,
    count: usize,
    limit: usize,
) -> Result<()> {
    if distance == 0 || distance > output.len() {
        return Err(Error::invalid_format(format!(
            "invalid back-reference distance {distance}"
        )));
    }
    for _ in 0..count {
        if output.len() >= limit {
            break;
        }
        let source_index = output.len() - distance;
        output.push(output[source_index]);
    }
    Ok(())
}

pub fn decompress_stream(
    data: &[u8],
    offset: usize,
    expected_size: Option<usize>,
    widths: (u8, u8, u8),
) -> Result<(Vec<u8>, usize)> {
    let (width1, width2, width3) = widths;
    let mut reader = BitReader::new(data, offset);
    let mut output = Vec::new();

    loop {
        if let Some(expected_size) = expected_size {
            if output.len() >= expected_size {
                break;
            }
        }

        let step = (|| -> Result<()> {
            let mut prefix = reader.read_bits(1)?;
            if prefix == 0 {
                let distance = reader.read_bits(width2)? as usize + 2;
                let count = reader.read_bits(width1)? as usize + 2;
                let limit = expected_size.unwrap_or(output.len() + count);
                copy_from_history(&mut output, distance, count, limit)?;
                return Ok(());
            }

            prefix = reader.read_bits(1)?;
            if prefix == 0 {
                let distance = reader.read_bits(width3)? as usize + 2 + (1_usize << width2);
                let count = reader.read_bits(width1)? as usize + 2;
                let limit = expected_size.unwrap_or(output.len() + count);
                copy_from_history(&mut output, distance, count, limit)?;
                return Ok(());
            }

            output.push(reader.read_bits(8)? as u8);
            Ok(())
        })();

        match step {
            Ok(()) => {}
            Err(Error::UnexpectedEof(_)) if expected_size.is_none() => break,
            Err(error) => return Err(error),
        }
    }

    Ok((output, reader.bytes_consumed(offset)))
}

#[cfg(test)]
mod tests {
    use super::decompress_stream;

    #[test]
    fn literal_only_stream_round_trips() {
        let data = [0xD0_u8, 0x40_u8];
        let (output, consumed) = decompress_stream(&data, 0, None, (4, 10, 13)).unwrap();
        assert_eq!(output, vec![0x41]);
        assert_eq!(consumed, 2);
    }
}
