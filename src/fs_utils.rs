use std::path::Path;
use flate2::read::GzDecoder;
use tar::Archive;
use crate::{registry::ImageLayerData, ContainerError};

pub fn decompress_layer<P: AsRef<Path>>(
    layer: ImageLayerData,
    dest: P,
) -> Result<(), ContainerError> {
    let gz = GzDecoder::new(layer.0.as_ref());
    let mut archive = Archive::new(gz);
    archive.unpack(dest)?;
    Ok(())
}
