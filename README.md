# LAS Rasterizer
A simple CLI tool for rasterizing LAS/LAZ point clouds.

Use-cases include DEM/DSM generation, density analysis, etc. and should be able to output to any GDAL raster driver with writing support.

Use the `--help` flag for a more detailed explanation.

## Availiable methods/commands

### Binning
Simple method of rasterization accounting only for the points within any given pixel. The points can be 'collapsed' into a pixel as a mean, median, min, max of points or a point count.

### Triangulation
Currently via a spike-free triangulation methodolgy specified in A. Khosravipour et al. 2016.

Should be the same method as in [LAStools](https://rapidlasso.de/generating-spike-free-digital-surface-models-from-lidar/).

## Requirements
GDAL installation (compiled for 3.10, but most versions should work.)
