use crate::TriangleIterator;
use crate::types::{Normal, Triangle, Vertex};
use std::io::{BufRead, BufReader, Read, Result, Seek};

/// Struct for ascii STL reader.
pub struct AsciiStlReader<'a> {
    reader: BufReader<&'a mut dyn Read>,
    line: String,
}

impl<'a> Iterator for AsciiStlReader<'a> {
    type Item = Result<Triangle>;
    fn next(&mut self) -> Option<Self::Item> {
        match self.next_face() {
            Ok(None) => None,
            Ok(Some(t)) => Some(Ok(t)),
            Err(e) => Some(Err(e)),
        }
    }
}

impl<'a> AsciiStlReader<'a> {
    /// Test whether or not read is an ascii STL file.
    pub fn probe<F: Read + Seek>(read: &mut F) -> Result<()> {
        let mut header = String::new();
        let maybe_read_error = BufReader::new(&mut *read).read_line(&mut header);
        // Try to seek back to start before evaluating potential read errors.
        read.seek(std::io::SeekFrom::Start(0))?;
        maybe_read_error?;
        if !header.starts_with("solid ") {
            Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "ascii STL does not start with \"solid \"",
            ))
        } else {
            Ok(())
        }
    }
    /// Factory to create a new ascii STL Reader from read.
    pub fn create_triangle_iterator(
        read: &'a mut dyn Read,
    ) -> Result<Box<dyn TriangleIterator<Item = Result<Triangle>> + 'a>> {
        let mut reader = BufReader::new(read);
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Err(e) => return Err(e),
            Ok(0) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "empty file?",
                ));
            }
            Ok(_) if !header.starts_with("solid ") => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "ascii STL does not start with \"solid \"",
                ));
            }
            _ => {}
        }
        Ok(Box::new(AsciiStlReader {
            reader,
            line: String::new(),
        })
            as Box<dyn TriangleIterator<Item = Result<Triangle>>>)
    }
    // Reuse only the line storage; Unicode tokenization and errors stay unchanged.
    fn next_line(&mut self) -> Option<Result<&str>> {
        loop {
            self.line.clear();
            match self.reader.read_line(&mut self.line) {
                Ok(0) => return None,
                Err(error) => return Some(Err(error)),
                Ok(_) if self.line.trim().is_empty() => continue,
                Ok(_) => return Some(Ok(&self.line)),
            }
        }
    }
    // Good lines need no token allocation; errors still show every original token.
    fn tokens<const N: usize>(line: &str) -> Option<[&str; N]> {
        let mut tokens = line.split_whitespace();
        let mut result = [""; N];
        for token in &mut result {
            *token = tokens.next()?;
        }
        tokens.next().is_none().then_some(result)
    }
    // Tries to read a triangle.
    fn next_face(&mut self) -> Result<Option<Triangle>> {
        let Some(face_header) = self.next_line() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "EOF while expecting facet or endsolid.",
            ));
        };
        let face_header = face_header?;
        if face_header.split_whitespace().next() == Some("endsolid") {
            return Ok(None);
        }
        let Some(face_header) = Self::tokens::<5>(face_header)
            .filter(|tokens| tokens[0] == "facet" && tokens[1] == "normal")
        else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "invalid facet header: {:?}",
                    face_header.split_whitespace().collect::<Vec<_>>()
                ),
            ));
        };
        let mut result_normal = Normal::default();
        AsciiStlReader::tokens_to_f32(&face_header[2..5], &mut result_normal.0[0..3])?;
        self.expect_static(&["outer", "loop"])?;
        let mut result_vertices = [Vertex::default(); 3];
        for vertex_result in &mut result_vertices {
            let Some(line) = self.next_line() else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "EOF while expecting vertex",
                ));
            };
            let line = line?;
            let Some(line) = Self::tokens::<4>(line).filter(|tokens| tokens[0] == "vertex") else {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!(
                        "vertex f32 f32 f32, got {:?}",
                        line.split_whitespace().collect::<Vec<_>>()
                    ),
                ));
            };
            AsciiStlReader::tokens_to_f32(&line[1..4], &mut vertex_result.0[0..3])?;
        }
        self.expect_static(&["endloop"])?;
        self.expect_static(&["endfacet"])?;
        Ok(Some(Triangle {
            normal: result_normal,
            vertices: result_vertices,
        }))
    }
    fn tokens_to_f32(tokens: &[&str], output: &mut [f32]) -> Result<()> {
        assert_eq!(tokens.len(), output.len());
        for (token, out) in tokens.iter().zip(output.iter_mut()) {
            let f = token
                .parse::<f32>()
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
            if !f.is_finite() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("expected finite f32, got {f} which is {:?}", f.classify()),
                ));
            }
            *out = f;
        }
        Ok(())
    }
    fn expect_static(&mut self, expectation: &[&str]) -> Result<()> {
        let Some(line) = self.next_line() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                format!("EOF while expecting {expectation:?}"),
            ));
        };
        let line = line?;
        if !line.split_whitespace().eq(expectation.iter().copied()) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "expected {expectation:?}, got {:?}",
                    line.split_whitespace().collect::<Vec<_>>()
                ),
            ));
        }
        Ok(())
    }
}

impl<'a> TriangleIterator for AsciiStlReader<'a> {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, ErrorKind};

    const FACET: &str = "facet normal 0 0 1\nouter loop\nvertex 0.1 -0 1e-45\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\n";

    #[test]
    fn unicode_whitespace_and_small_coordinates_survive_borrowed_tokens() {
        let source = format!(
            "solid unicode test\n\u{2003}\n{}endsolid unicode test\n",
            FACET.replace(' ', "\u{2003}")
        );
        let mut input = Cursor::new(source.as_bytes());
        let mut reader = AsciiStlReader::create_triangle_iterator(&mut input).unwrap();
        let triangle = reader.next().unwrap().unwrap();
        assert_eq!(triangle.vertices[0][0], 0.1_f32);
        assert_eq!(triangle.vertices[0][1].to_bits(), (-0.0_f32).to_bits());
        assert_eq!(triangle.vertices[0][2].to_bits(), 1);
        assert!(reader.next().is_none());
    }

    #[test]
    fn malformed_header_keeps_exact_error_and_next_facet_position() {
        let source =
            format!("solid continuation\nfacet nope 0 0 1\n{FACET}endsolid continuation\n");
        let mut input = Cursor::new(source.as_bytes());
        let mut reader = AsciiStlReader::create_triangle_iterator(&mut input).unwrap();
        let error = reader.next().unwrap().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            "invalid facet header: [\"facet\", \"nope\", \"0\", \"0\", \"1\"]"
        );
        assert!(reader.next().unwrap().is_ok());
        assert!(reader.next().is_none());
    }
}

#[cfg(test)]
mod stack_token_tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn extra_and_missing_tokens_keep_exact_errors() {
        for (line, expected) in [
            (
                "facet normal 0 0 1 extra",
                "invalid facet header: [\"facet\", \"normal\", \"0\", \"0\", \"1\", \"extra\"]",
            ),
            (
                "facet normal 0 0",
                "invalid facet header: [\"facet\", \"normal\", \"0\", \"0\"]",
            ),
        ] {
            let input = format!("solid errors\n{line}\nendsolid errors");
            let mut input = Cursor::new(input.into_bytes());
            let mut reader = AsciiStlReader::create_triangle_iterator(&mut input).unwrap();
            assert_eq!(reader.next().unwrap().unwrap_err().to_string(), expected);
            assert!(reader.next().is_none());
        }
    }
}
#[cfg(test)]
mod reusable_line_tests {
    use super::*;
    use std::io::{Cursor, ErrorKind};

    #[test]
    fn invalid_utf8_keeps_iterator_continuation_after_consumed_line() {
        let mut source = b"solid continuation\n\xff\n".to_vec();
        source.extend_from_slice(b"facet normal 0 0 1\r\nouter loop\r\nvertex 0 0 0\r\nvertex 1 0 0\r\nvertex 0 1 0\r\nendloop\r\nendfacet\r\nendsolid continuation");
        let mut input = Cursor::new(source);
        let mut reader = AsciiStlReader::create_triangle_iterator(&mut input).unwrap();
        let error = reader.next().unwrap().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidData);
        assert_eq!(error.to_string(), "stream did not contain valid UTF-8");
        assert!(reader.next().unwrap().is_ok());
        assert!(reader.next().is_none());
    }
}

#[cfg(test)]
mod read_error_tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    struct InterruptedLine {
        step: usize,
        offset: usize,
    }
    impl Read for InterruptedLine {
        fn read(&mut self, out: &mut [u8]) -> Result<usize> {
            if out.is_empty() {
                return Ok(0);
            }
            if self.step == 2 {
                self.step += 1;
                return Err(Error::other("injected reader failure"));
            }
            let bytes: &[u8] = match self.step {
                0 => b"solid continuation\r\n",
                1 => b"facet normal ",
                3 => b"0 0 1\r\n",
                4 => b"facet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\n",
                5 => b"endsolid continuation",
                _ => return Ok(0),
            };
            let count = out.len().min(bytes.len() - self.offset);
            out[..count].copy_from_slice(&bytes[self.offset..self.offset + count]);
            self.offset += count;
            if self.offset == bytes.len() {
                self.step += 1;
                self.offset = 0;
            }
            Ok(count)
        }
    }

    #[test]
    fn read_error_discards_partial_line_and_keeps_next_facet_position() {
        let mut input = InterruptedLine { step: 0, offset: 0 };
        let mut reader = AsciiStlReader::create_triangle_iterator(&mut input).unwrap();
        let error = reader.next().unwrap().unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Other);
        assert_eq!(error.to_string(), "injected reader failure");
        assert_eq!(
            reader.next().unwrap().unwrap_err().to_string(),
            "invalid facet header: [\"0\", \"0\", \"1\"]"
        );
        assert!(reader.next().unwrap().is_ok());
        assert!(reader.next().is_none());
    }
}
