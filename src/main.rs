use std::collections::HashMap;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, Subcommand, ValueEnum};
use gdal::raster::Buffer;
use gdal::{Driver, DriverManager, Metadata};
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

    if !(split.len() == 6 || split.len() == 4) {
        return Err(format!("'{s}' has an invalid number of coordinates"));
    }

    let use_z: bool = split.len() == 6;

    fn parse(num: &str) -> Result<f64, String> {
        num.parse()
            .map_err(|e: <f64 as FromStr>::Err| e.to_string())
    }

    fn check_min_max(min: &[f64; 3], max: &[f64; 3]) -> Result<(), String> {
        for i in 0..3 {
            if min[i] > max[i] {
                return Err(format!(
                    "Invalid extent, {} is greater than {}",
                    min[i], max[i]
                ));
            }
        }

        Ok(())
    }

    let min_x = parse(split[0])?;
    let min_y = parse(split[1])?;

    let min_z = match use_z {
        true => parse(split[2])?,
        false => f64::MIN,
    };

    let max_x = match use_z {
        true => parse(split[2]),
        false => parse(split[3]),
    }?;

    let max_y = match use_z {
        true => parse(split[3]),
        false => parse(split[4]),
    }?;

    let max_z = match use_z {
        true => parse(split[5])?,
        false => f64::MAX,
    };

    let min: [f64; 3] = [min_x, min_y, min_z];

    let max: [f64; 3] = [max_x, max_y, max_z];

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
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Path to las/laz file.
    #[arg(short, long)]
    input: PathBuf,

    /// Resolution of the outut raster.
    #[arg(short, long)]
    res: f64,

    /// Optional LAS classification code filter [see LAS specification: https://www.asprs.org/wp-content/uploads/2019/03/LAS_1_4_r14.pdf#page=22]
    #[arg(short, long)]
    class: Option<u8>,

    /// Variable to rasterize. Default: z
    #[arg(short, long)]
    var: Option<Variable>,

    /// Extent of the output raster. Default: bounds of the source las/laz [min x, y, z, max x, y, z]
    #[arg(short, long, value_parser = extent_parser)]
    extent: Option<Bounds>,

    /// Specific NODATA value. Default: -9999.0
    #[arg(short, long)]
    nodata: Option<f64>,

    /// Output raster path
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
    let bounds = cli.extent.unwrap_or(reader.header().bounds());

    let data = match &cli.command {
        Commands::Bin { func } => bin_points(
            reader,
            bounds,
            cli.res,
            cli.class,
            cli.var.unwrap_or(Variable::Z),
            func.clone().unwrap_or(Function::Median),
        )?,
        Commands::Triangulate {
            freeze_distance,
            insertion_buffer,
        } => triangulate(
            reader,
            bounds,
            cli.var.unwrap_or(Variable::Z),
            cli.res,
            *freeze_distance,
            *insertion_buffer,
        )?,
    };

    // Collect availiable GDAL raster drivers.
    let drivers: Vec<Driver> = DriverManager::all()
        .filter(|d| {
            d.metadata_item("DCAP_RASTER", "").is_some()
                && d.metadata_item("DCAP_CREATE", "").is_some()
                && d.metadata_item("DMD_EXTENSIONS", "").is_some()
        })
        .collect();

    let mut driver_map: HashMap<String, &Driver> = HashMap::new();

    for driver in drivers.iter() {
        let items = driver.metadata_item("DMD_EXTENSIONS", "").unwrap();
        let exts = items.split(' ').collect::<Vec<&str>>();

        for ext in exts {
            driver_map.insert(ext.to_string(), driver);
        }
    }

    let out_ext = cli
        .output
        .extension()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();

    let driver = driver_map
        .get(
            &cli.output
                .extension()
                .map(|o| o.to_str().unwrap().to_string())
                .unwrap(),
        )
        .ok_or(Error::NoDriverForExtension(format!("{:?}", out_ext)))?;

    info!("Writing {:?} ...", driver.short_name());

    let (width, height) = get_raster_size(&bounds, cli.res);

    let mut ds = driver.create_with_band_type::<f64, _>(cli.output, width, height, 1)?;

    ds.set_geo_transform(&[bounds.min.x, cli.res, 0.0, bounds.min.y, 0.0, cli.res])?;
    let mut rb = ds.rasterband(1)?;
    rb.set_no_data_value(Some(cli.nodata.unwrap_or(NODATA)))?;
    rb.write(
        (0, 0),
        (width, height),
        &mut Buffer::new((width, height), data),
    )?;

    info!("Done!");
    Ok(())
}
