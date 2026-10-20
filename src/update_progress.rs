//! Download progress measured from bytes successfully written to disk.
use std::io::{self, Read, Write};

#[derive(Clone, Copy, Debug, Default)]
pub struct Progress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub extracting: bool,
}

impl Progress {
    pub fn percent(self) -> Option<u32> {
        self.total
            .filter(|total| *total > 0)
            .map(|total| ((u128::from(self.downloaded) * 100 / u128::from(total)).min(100)) as u32)
    }
}

pub fn copy_download(
    reader: &mut impl Read,
    writer: &mut impl Write,
    total: Option<u64>,
    mut report: impl FnMut(Progress),
) -> io::Result<()> {
    let mut progress = Progress {
        total,
        ..Progress::default()
    };
    report(progress);
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if read == 0 {
            break;
        }
        writer.write_all(&buffer[..read])?;
        progress.downloaded += read as u64;
        report(progress);
    }
    writer.flush()?;
    if let Some(total) = total.filter(|total| *total > 0)
        && progress.downloaded != total
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Incomplete update download",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copies_real_bytes_with_monotonic_progress() {
        let bytes = vec![42u8; 200_000];
        let mut output = Vec::new();
        let mut values = Vec::new();
        copy_download(
            &mut &bytes[..],
            &mut output,
            Some(bytes.len() as u64),
            |p| values.push(p.percent().unwrap()),
        )
        .unwrap();
        assert_eq!(output, bytes);
        assert_eq!(values.first(), Some(&0));
        assert_eq!(values.last(), Some(&100));
        assert!(values.windows(2).all(|p| p[0] <= p[1]));
        assert!(values.iter().any(|p| *p > 0 && *p < 100));
    }
    #[test]
    fn unknown_length_does_not_invent_a_percentage() {
        for total in [None, Some(0)] {
            copy_download(&mut &b"update"[..], &mut Vec::new(), total, |p| {
                assert_eq!(p.percent(), None)
            })
            .unwrap();
        }
    }
    #[test]
    fn truncated_download_fails_before_extraction() {
        assert!(copy_download(&mut &b"short"[..], &mut Vec::new(), Some(100), |_| {}).is_err());
    }
    #[test]
    fn write_failure_is_reported() {
        struct Broken;
        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::other("disk full"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert!(copy_download(&mut &b"update"[..], &mut Broken, Some(6), |_| {}).is_err());
    }
}
