use std::fs::OpenOptions;
use std::io::Write as _;
use std::os::unix::fs::OpenOptionsExt as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use fleet_update::signature::{
    self, CHECKSUM_EXTENSION, PUBLIC_KEY, SIGNATURE_EXTENSION, checksum_line,
};
use fleet_update::{Error, Result};

const KEY_ENV: &str = "FLOCK_UPDATE_SIGNING_KEY";
const PRIVATE_MODE: u32 = 0o600;
const USAGE: &str = "fleet-sign keygen <private-key-file> | sign <zip> | verify <zip>";

fn sibling(file: &Path, extension: &str) -> PathBuf {
    let mut name = file.as_os_str().to_owned();
    name.push(".");
    name.push(extension);
    PathBuf::from(name)
}

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(Error::io(path))
}

fn read_text(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(Error::io(path))
}

fn keygen(private: &Path) -> Result<()> {
    let keys = signature::generate()?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(PRIVATE_MODE)
        .open(private)
        .map_err(Error::io(private))?;
    file.write_all(keys.private.as_bytes())
        .map_err(Error::io(private))?;
    println!("{}", keys.public);
    Ok(())
}

fn sign(zip: &Path) -> Result<()> {
    let key = std::env::var(KEY_ENV).map_err(|_| Error::MissingEnv(KEY_ENV))?;
    let data = read(zip)?;
    let name = zip
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(Error::Usage(USAGE))?;
    let signed = signature::sign(&data, &key)?;
    let checksum = sibling(zip, CHECKSUM_EXTENSION);
    std::fs::write(&checksum, checksum_line(&data, name)).map_err(Error::io(&checksum))?;
    let sig = sibling(zip, SIGNATURE_EXTENSION);
    std::fs::write(&sig, signed).map_err(Error::io(&sig))?;
    println!("{}", checksum.display());
    println!("{}", sig.display());
    Ok(())
}

fn verify(zip: &Path) -> Result<()> {
    let data = read(zip)?;
    let checksum = read_text(&sibling(zip, CHECKSUM_EXTENSION))?;
    signature::verify_checksum(&data, &checksum)?;
    let sig = read_text(&sibling(zip, SIGNATURE_EXTENSION))?;
    signature::verify(&data, &sig, PUBLIC_KEY)?;
    println!("{}: checksum and signature verified", zip.display());
    Ok(())
}

fn run(args: &[String]) -> Result<()> {
    match args {
        [command, path] if command == "keygen" => keygen(Path::new(path)),
        [command, path] if command == "sign" => sign(Path::new(path)),
        [command, path] if command == "verify" => verify(Path::new(path)),
        _ => Err(Error::Usage(USAGE)),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("fleet-sign: {error}");
            ExitCode::FAILURE
        }
    }
}
