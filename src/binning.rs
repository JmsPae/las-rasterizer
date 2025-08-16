use las::{Bounds, Reader};

use crate::error::{Error, Result};
use crate::util::get_raster_size;
use crate::{get_var, Function, Variable, NODATA};

pub fn collapse_cell(points: Vec<f64>, function: &Function) -> f64 {
    let len = points.len();
    if len == 0 {
        return NODATA;
    }

    match *function {
        Function::Mean => points.into_iter().sum::<f64>() / len as f64,
        Function::Median => {
            if len == 1 {
                return points[0];
            }

            let mut points = points;
            points.sort_by(|a, b| a.partial_cmp(b).unwrap());

            match points.len() % 2 == 0 {
                true => (points[len / 2 - 1] + points[len / 2]) / 2.0,
                false => points[len / 2],
            }
        }
        Function::Min => points.into_iter().fold(f64::MAX, |acc, p| acc.min(p)),
        Function::Max => points.into_iter().fold(f64::MIN, |acc, p| acc.max(p)),
        Function::Count => len as f64,
    }
}

pub fn bin_points(
    mut reader: Reader,
    bounds: Bounds,
    res: f64,
    class: Option<u8>,
    var: Variable,
    func: Function,
) -> Result<Vec<f64>> {
    // Calculate the outpur raster's width and height
    let (width, height) = get_raster_size(&bounds, res);
    let len = width * height;

    // Allocate the bins
    let mut data: Vec<Vec<f64>> = vec![Vec::new(); len];

    for point in reader.points() {
        let point = point?;

        // Filter out points if filter is present.
        if let Some(c) = class {
            if u8::from(point.classification) != c {
                continue;
            }
        }

        // Get an array index from the point's x, y position.
        let x_idx = ((point.x - bounds.min.x) / res).floor() as usize;
        let y_idx = ((point.y - bounds.min.y) / res).floor() as usize;
        let i = y_idx * width + x_idx;

        // Get the array of values for a given cell (along with some classic error handling ;) )
        let cell = data.get_mut(i).ok_or(Error::ShouldntHappen(format!(
            "Couldn't get index {i}/{len}: {x_idx}, {y_idx} {width}, {height}"
        )))?;

        // Append a variable (the point's Z value by default) to the cell bin
        cell.push(get_var(&var, &point));
    }

    // Return an "Ok" result, collapsing each cell into a single value given a certain function,
    // by default the cell bin's median.
    Ok(data
        .into_iter()
        .map(|cell| collapse_cell(cell, &func))
        .collect::<Vec<f64>>())
}
