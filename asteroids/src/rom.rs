use std::{
    fmt::{self, Display, Formatter},
    fs::File,
    io::{BufReader, Cursor, Read, Seek},
    path::Path,
};

use zip::ZipArchive;

pub struct ROM {
    pub program: [u8; 0x1800],
    pub vector: [u8; 0x800],
    pub dvg_state_prom: [u8; 0x100],
}

#[derive(Debug)]
pub enum RomError {
    /// The zip could not be opened
    MalformedZip(String),
    /// An expected part of the ROM zip is missing
    MissingFile {
        file_name: &'static str,
        semantic_name: &'static str,
    },
    /// An expected part of the ROM zip is present, but is incorrectly sized
    InvalidSize {
        file_name: &'static str,
        semantic_name: &'static str,
        expected: usize,
        actual: usize,
    },
}

impl Display for RomError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            RomError::MalformedZip(message) => write!(f, "Could not read archive: {message}"),
            RomError::MissingFile {
                file_name,
                semantic_name,
            } => write!(f, "Could not find {file_name} ({semantic_name})"),
            RomError::InvalidSize {
                file_name,
                semantic_name,
                expected,
                actual,
            } => write!(
                f,
                "Invalid size for file {file_name} ({semantic_name}): {actual}, expected {expected}"
            ),
        }
    }
}

impl std::error::Error for RomError {}

impl ROM {
    pub fn load_mame(path: &Path) -> Self {
        let file = File::open(path).expect("Could not open file");

        Self::load_mame_reader(BufReader::new(file)).expect("Could not load ROM")
    }

    pub fn load_mame_bytes(data: &[u8]) -> Result<Self, RomError> {
        Self::load_mame_reader(Cursor::new(data))
    }

    fn load_mame_reader<R: Read + Seek>(reader: R) -> Result<Self, RomError> {
        let mut zip = ZipArchive::new(reader).map_err(|e| RomError::MalformedZip(e.to_string()))?;

        let mut program = [0; 0x1800];
        let mut vector = [0; 0x800];
        let mut dvg_state_prom = [0; 0x100];

        read_from_zip_file(
            &mut zip,
            &mut program[0x0..0x800],
            "035145-04e.ef2",
            "0x6800 PROM",
            0x800,
        )?;

        read_from_zip_file(
            &mut zip,
            &mut program[0x800..0x1000],
            "035144-04e.h2",
            "0x7000 PROM",
            0x800,
        )?;

        read_from_zip_file(
            &mut zip,
            &mut program[0x1000..0x1800],
            "035143-02.j2",
            "0x7800 PROM",
            0x800,
        )?;

        read_from_zip_file(&mut zip, &mut vector, "035127-02.np3", "Vector ROM", 0x800)?;

        read_from_zip_file(
            &mut zip,
            &mut dvg_state_prom,
            "034602-01.c8",
            "DVG ROM",
            0x100,
        )?;

        Ok(ROM {
            program,
            vector,
            dvg_state_prom,
        })
    }
}

fn read_from_zip_file<R: Read + Seek>(
    zip: &mut ZipArchive<R>,
    buffer: &mut [u8],
    file_name: &'static str,
    semantic_name: &'static str,
    expected_size: usize,
) -> Result<(), RomError> {
    let mut file = zip.by_name(file_name).map_err(|_| RomError::MissingFile {
        file_name,
        semantic_name,
    })?;

    let size = file.size() as usize;
    if size != expected_size {
        return Err(RomError::InvalidSize {
            file_name,
            semantic_name,
            expected: expected_size,
            actual: size,
        });
    }

    file.read_exact(buffer).map_err(|_| RomError::MissingFile {
        file_name,
        semantic_name,
    })
}
