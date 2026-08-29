use std::{
    fs::File,
    io::{BufReader, Read},
    path::Path,
};

use zip::ZipArchive;

pub struct ROM {
    pub program: [u8; 0x1800],
    pub vector: [u8; 0x800],
    pub dvg_state_prom: [u8; 0x100],
}

impl ROM {
    pub fn load_mame(path: &Path) -> Self {
        let file = File::open(&path).expect("Could not open file");
        let file = BufReader::new(file);
        let mut zip = zip::ZipArchive::new(file).expect("Could not read zip");

        let mut program = [0; 0x1800];
        let mut vector = [0; 0x800];
        let mut dvg_state_prom = [0; 0x100];

        read_from_zip_file(
            &mut zip,
            &mut program[0x0..0x800],
            "035145-04e.ef2",
            "0x6800 PROM",
            0x800,
        );

        read_from_zip_file(
            &mut zip,
            &mut program[0x800..0x1000],
            "035144-04e.h2",
            "0x7000 PROM",
            0x800,
        );

        read_from_zip_file(
            &mut zip,
            &mut program[0x1000..0x1800],
            "035143-02.j2",
            "0x7800 PROM",
            0x800,
        );

        read_from_zip_file(&mut zip, &mut vector, "035127-02.np3", "Vector ROM", 0x800);

        read_from_zip_file(
            &mut zip,
            &mut dvg_state_prom,
            "034602-01.c8",
            "DVG ROM",
            0x100,
        );

        ROM {
            program,
            vector,
            dvg_state_prom,
        }
    }
}

fn read_from_zip_file(
    zip: &mut ZipArchive<BufReader<File>>,
    buffer: &mut [u8],
    file_name: &str,
    semantic_name: &str,
    expected_size: usize,
) {
    let mut file = zip
        .by_name(file_name)
        .expect(&format!("Could not find {file_name} ({semantic_name})"));

    let size = file.size() as usize;
    if size != expected_size {
        panic!("Invalid size for file {file_name} ({semantic_name}): {size}");
    }

    file.read_exact(buffer)
        .expect(&format!("Could not read {file_name} ({semantic_name})"));
}
