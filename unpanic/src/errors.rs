#[derive(Debug)]
pub enum Error {
    InvalidOutDir,
    CrateNameMissing,
    TargetPathMissing,
    SrcLocationMissing,
}
