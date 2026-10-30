//! Buffered JSON parsing. Callers retain their existing byte limits and validation.
use serde::de::DeserializeOwned;
use std::io::{BufReader, Read};

pub(crate) fn read<T: DeserializeOwned>(source: impl Read) -> serde_json::Result<T> {
    // serde_json consumes a Read byte by byte. Buffer below the parser so files
    // do not perform a syscall for every byte; Take still bounds the source.
    serde_json::from_reader(BufReader::with_capacity(64 * 1024, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, io::Cursor};

    #[test]
    fn preserves_values_and_amortizes_source_reads() {
        struct Counted<'a> {
            data: Cursor<Vec<u8>>,
            calls: &'a Cell<usize>,
        }
        impl Read for Counted<'_> {
            fn read(&mut self, target: &mut [u8]) -> std::io::Result<usize> {
                self.calls.set(self.calls.get() + 1);
                self.data.read(target)
            }
        }
        let value = serde_json::json!({
            "extension": [null, true, "中文 😀", 1.2345678901234567],
            "source": "shader".repeat(50_000),
        });
        let calls = Cell::new(0);
        let restored: serde_json::Value = read(Counted {
            data: Cursor::new(serde_json::to_vec(&value).unwrap()),
            calls: &calls,
        })
        .unwrap();
        assert_eq!(restored, value);
        assert!(calls.get() < 512, "{} source reads", calls.get());
    }

    #[test]
    fn keeps_byte_limits_and_parser_error_positions() {
        for source in [
            b"{\n\"x\": true}".as_slice(),
            b"[1, 2] trailing",
            b"[\"\xff\"]",
        ] {
            for limit in 0..=source.len() {
                let original =
                    serde_json::from_reader::<_, serde_json::Value>(source.take(limit as u64));
                let buffered = read::<serde_json::Value>(source.take(limit as u64));
                match (original, buffered) {
                    (Ok(a), Ok(b)) => assert_eq!(a, b),
                    (Err(a), Err(b)) => {
                        assert_eq!(a.classify(), b.classify());
                        assert_eq!((a.line(), a.column()), (b.line(), b.column()));
                    }
                    result => panic!("Different parse result at {limit}: {result:?}"),
                }
            }
        }
    }
}
