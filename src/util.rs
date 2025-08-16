use las::Bounds;

/// (width, height)
pub fn get_raster_size(bounds: &Bounds, res: f64) -> (usize, usize) {
    let width: usize = ((bounds.max.x - bounds.min.x) / res).ceil() as usize;
    let height: usize = ((bounds.max.y - bounds.min.y) / res).ceil() as usize;

    (width, height)
}
