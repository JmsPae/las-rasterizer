use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, Subcommand, ValueEnum};
use gdal::raster::Buffer;
use gdal::DriverManager;
use las::{Bounds, Point, Reader, Vector};
use log::info;

use self::binning::bin_points;
use self::error::Error;
use self::triangulation::triangulate;
use self::util::get_raster_size;

mod error;
mod util;

mod binning;
mod triangulation;

#[derive(Debug, ValueEnum, Clone)]
enum Variable {
    X,
    Y,
    Z,
    Intensity,
}

#[derive(Debug, ValueEnum, Clone)]
enum Function {
    Mean,
    Median,

    Min,
    Max,

    Count,
}

fn extent_parser(s: &str) -> Result<Bounds, String> {
    let split: Vec<&str> = s.split(',').collect();

    if split.len() != 6 {
        return Err(format!("'{s}' has an insufficient number of coordinates"));
    }

    fn parse(num: &str) -> Result<f64, String> {
        num.parse()
            .map_err(|e: <f64 as FromStr>::Err| e.to_string())
    }

    fn check_min_max(min: &[f64; 3], max: &[f64; 3]) -> Result<(), String> {
        for i in 0..3 {
            if min[i] > max[i] {
                return Err(format!(
                    "Invalid extent. {} is greater than {}",
                    min[i], max[i]
                ));
            }
        }

        Ok(())
    }

    let min: [f64; 3] = [parse(split[0])?, parse(split[1])?, parse(split[2])?];

    let max: [f64; 3] = [parse(split[3])?, parse(split[4])?, parse(split[5])?];

    check_min_max(&min, &max)?;

    Ok(Bounds {
        min: Vector {
            x: min[0],
            y: min[1],
            z: min[2],
        },
        max: Vector {
            x: max[0],
            y: max[1],
            z: max[2],
        },
    })
}

#[derive(Subcommand)]
enum Commands {
    /// Use raw point cloud values via binning.
    Bin {
        /// Binning function. Default: median
        #[arg(short, long)]
        func: Option<Function>,
    },
    Triangulate {
        /// Triangles past the buffer will be 'frozen' if all three edges are less than this
        /// distance, blocking any points below from the triangulation.
        #[arg(short, long)]
        freeze_distance: f64,

        /// The insertion_buffer will block triangles from freezing for a time blocking any
        /// premature freezing.
        #[arg(short, long)]
        insertion_buffer: f64,
    },
}

#[derive(Parser)]
#[command(version, about = "Generates a raster from a las/laz file", long_about = None)]
/// Chungus
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to las/laz file.
    #[arg(short, long)]
    input: PathBuf,

    /// Resolution of the outut raster.
    #[arg(short, long)]
    res: f64,

    /// Optional classification filter.
    #[arg(short, long)]
    class: Option<u8>,

    /// Variable to rasterize. Default: z
    #[arg(short, long)]
    var: Option<Variable>,

    /// Extent of the output raster. Default: bounds of the source las/laz [min x, y, z, max x, y, z]
    #[arg(short, long, value_parser = extent_parser)]
    extent: Option<Bounds>,

    /// Output GeoTIFF path
    output: PathBuf,
}

fn get_var(var: &Variable, point: &Point) -> f64 {
    match *var {
        Variable::X => point.x,
        Variable::Y => point.y,
        Variable::Z => point.z,
        Variable::Intensity => point.intensity as f64,
    }
}

pub const NODATA: f64 = -9999.0;

fn main() -> Result<(), Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .init();
    let cli = Cli::parse();

    let reader = Reader::from_path(&cli.input)?;
    let bounds = reader.header().bounds();
    let (width, height) = get_raster_size(&reader, cli.res);

    let data = match &cli.command {
        Commands::Bin { func } => bin_points(reader, &cli, func)?,
        Commands::Triangulate {
            freeze_distance,
            insertion_buffer,
        } => triangulate(
            reader,
            cli.var.unwrap_or(Variable::Z),
            cli.res,
            *freeze_distance,
            *insertion_buffer,
        )?,
    };

    info!("Writing...");
    let mut ds = DriverManager::get_driver_by_name("GTiff")?
        .create_with_band_type::<f64, _>(cli.output, width, height, 1)?;

    ds.set_geo_transform(&[bounds.min.x, cli.res, 0.0, bounds.min.y, 0.0, cli.res])?;
    let mut rb = ds.rasterband(1)?;
    rb.set_no_data_value(Some(NODATA))?;
    rb.write(
        (0, 0),
        (width, height),
        &mut Buffer::new((width, height), data),
    )?;

    info!("Done!");
    Ok(())
}
