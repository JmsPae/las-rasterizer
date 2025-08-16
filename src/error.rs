use std::io;

use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO Error: {0}")]
    Disconnect(#[from] io::Error),

    #[error("Las Error: {0}")]
    Las(#[from] las::Error),

    #[error("GDAL Error: {0}")]
    Gdal(#[from] gdal::errors::GdalError),

    #[error("Triangulation Insertion Error: {0}")]
    Insertion(#[from] spade::InsertionError),

    #[error("Couldn't find a valid GDAL driver for extension '{0}'")]
    NoDriverForExtension(String),

    #[error("Something happened that really shouldn't: {0}")]
    ShouldntHappen(String),
}

pub type Result<T> = core::result::Result<T, Error>;
