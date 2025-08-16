use std::collections::VecDeque;

use las::point::Classification;
use las::{Bounds, Reader};
use log::info;
use spade::handles::FixedDirectedEdgeHandle;
use spade::{
    ConstrainedDelaunayTriangulation, FloatTriangulation, HasPosition, Point2, Triangulation,
};

use crate::error::Result;
use crate::util::get_raster_size;
use crate::{get_var, Variable, NODATA};

#[derive(Debug, Copy, Clone)]
struct Point {
    position: Point2<f64>,
    z: f64,
    value: f64,
}

impl Point {
    const fn new(x: f64, y: f64, z: f64, value: f64) -> Self {
        Self {
            position: Point2::new(x, y),
            z,
            value,
        }
    }
}

impl HasPosition for Point {
    type Scalar = f64;

    fn position(&self) -> Point2<Self::Scalar> {
        self.position
    }
}

type TriangulationType = ConstrainedDelaunayTriangulation<Point>;

pub fn triangulate(
    mut reader: Reader,
    bounds: Bounds,
    var: Variable,
    res: f64,
    freeze_distance: f64,
    insertion_buffer: f64,
) -> Result<Vec<f64>> {
    let mut points: Vec<Point> = Vec::with_capacity(reader.header().number_of_points() as usize);

    // To avoid unnessicary square roots.
    let freeze_distance_2 = freeze_distance * freeze_distance;
    let mut buffer_height = f64::MIN;

    for p in reader.points() {
        let point = p?;

        buffer_height = buffer_height.max(point.z);

        if point.classification == Classification::HighNoise {
            continue;
        }

        let var = get_var(&var, &point);

        points.push(Point::new(point.x, point.y, point.z, var));
    }

    info!("Sorting points...");
    // Sort by Z (Descending)
    points.sort_by(|a, b| b.z.partial_cmp(&a.z).unwrap());

    let mut t = TriangulationType::new();
    let mut constraint_buffer: VecDeque<FixedDirectedEdgeHandle> = VecDeque::new();

    info!("Building triangulation...");
    points.into_iter().try_for_each(|point| {
        for (i, edge) in constraint_buffer.iter().rev().enumerate() {
            let edge = t.directed_edge(*edge);

            if edge.from().data().z > buffer_height + insertion_buffer {
                constraint_buffer.drain(..=i).for_each(|e| {
                    let e = t.directed_edge(e);
                    let [a, b] = e.vertices();

                    let dist = (a.position().x - b.position().x).powi(2)
                        + (a.position().y - b.position().y).powi(2);

                    if dist < freeze_distance_2 {
                        t.add_constraint(a.fix(), b.fix());
                    }
                });

                break;
            }
        }

        let mut insert_vert = |t: &mut TriangulationType, buffer_height: &mut f64| -> Result<()> {
            *buffer_height = buffer_height.min(point.z);

            let vert = t.insert(point)?;

            constraint_buffer.extend(t.vertex(vert).out_edges().map(|e| e.fix()));

            Ok(())
        };

        match t.locate(point.position) {
            spade::PositionInTriangulation::OnFace(handle) => {
                let [a, b, c] = t.face(handle).adjacent_edges();

                if !(a.is_constraint_edge() && b.is_constraint_edge() && c.is_constraint_edge()) {
                    insert_vert(&mut t, &mut buffer_height)?;
                }
            }
            spade::PositionInTriangulation::OnVertex(handle) => {
                if point.z - t.vertex(handle).data().z > buffer_height {
                    insert_vert(&mut t, &mut buffer_height)?;
                }
            }
            spade::PositionInTriangulation::OnEdge(handle) => {
                if !t.is_constraint_edge(handle.as_undirected()) {
                    insert_vert(&mut t, &mut buffer_height)?;
                }
            }
            // No triangulation or outside hull
            _ => {
                insert_vert(&mut t, &mut buffer_height)?;
            }
        }

        Ok::<(), crate::error::Error>(())
    })?;

    let (width, height) = get_raster_size(&bounds, res);
    let mut ret: Vec<f64> = Vec::with_capacity(width * height);

    info!("Triangulating...");
    let interp = t.barycentric();
    for y in 0..height {
        // Center of pixel
        let p_y = bounds.min.y.round() + res * y as f64;
        for x in 0..width {
            let p_x = bounds.min.x.round() + res * x as f64;

            let p = interp
                .interpolate(|b| b.data().value, Point2 { x: p_x, y: p_y })
                .unwrap_or(NODATA);
            ret.push(p);
        }
    }

    Ok(ret)
}
