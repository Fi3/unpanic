#[derive(Debug)]
pub enum Error {
    InvalidOutDir,
    CrateNameMissing,
    NoEnvTargetCrateSet,
    TargetPathMissing,
    SrcLocationMissing,
}
