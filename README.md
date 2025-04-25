![actions status](https://img.shields.io/github/actions/workflow/status/fgardt/factorio-spritter/rust.yml)
[![release](https://img.shields.io/github/v/release/fgardt/factorio-spritter)](https://github.com/fgardt/factorio-spritter/releases)
[![ko-fi](https://img.shields.io/badge/Ko--fi-Donate%20-hotpink?logo=kofi&logoColor=white)](https://ko-fi.com/fgardt)

# Spritter

A simple CLI tool to combine individual sprites into spritesheets for factorio.

## Usage

```
~$ spritter help
Spritesheet generator for factorio

Usage: spritter <COMMAND>

Commands:
  spritesheet  Generate spritesheets from a folder of images
  icon         Generate a mipmap icon from a folder of images
  gif          Generate a gif from a folder of images
  optimize     Optimize an image or a folder of images
  split        Split a spritesheet into individual images
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Spritesheet

```
~$ spritter help spritesheet
Generate spritesheets from a folder of images

Usage: spritter spritesheet [OPTIONS] <SOURCE> <OUTPUT>

Arguments:
  <SOURCE>  Folder containing the individual sprites
  <OUTPUT>  Output folder

Options:
  -l, --lua
          Enable lua output generation
  -j, --json
          Enable json output generation
  -p, --prefix <PREFIX>
          Prefix to add to the output file name [default: ]
      --lossy
          Allow lossy compression for the output images. This is using pngquant / imagequant internally
  -r, --recursive
          Recursive search for images. Each folder will be a separate spritesheet
  -t, --tile-resolution <TILE_RESOLUTION>
          Resolution of the input sprites in pixels / tile [default: 64]
      --no-crop
          Set when the sprites should not be cropped
  -a, --crop-alpha <CROP_ALPHA>
          Sets the max alpha value to consider a pixel as transparent [0-255].
          Use a higher value in case your inputs have slightly transparent pixels and don't crop nicely. [default: 0]
  -b, --transparent-black <TRANSPARENT_BLACK>
          Sets the max channel value to consider a pixel as black.
          All "black" pixels will be turned fully transparent.
  -d, --deduplicate-empty-frames
          Remove duplicate empty frames before building the spritesheet.
          This will generate a frame_sequence in the data output to restore the original frame order.
          Make sure to have the --lua or --json flag set to receive the data output!
  -s, --scale <SCALE>
          Set a scaling factor to rescale the used sprites by.
          Values < 1.0 will shrink the sprites. Values > 1.0 will enlarge them. [default: 1]
      --scale-filter <SCALE_FILTER>
          The scaling filter to use when scaling sprites [default: catmull-rom] [possible values: nearest, triangle, catmull-rom, gaussian, lanczos3]
      --single-sheet-split-mode
          Automatically split each frame into multiple subframes if the frames would not fit on a single sheet.
          This allows you to use large sprites for graphic types that do not allow to specify multiple files for a single layer.
  -m, --max-sheet-size <MAX_SHEET_SIZE>
          Maximum size of a single sheet in frames per axis.
          A value of 0 means unlimited. [default: 0]
  -w, --max-sheet-width <MAX_SHEET_WIDTH>
          Maximum width of a single sheet in frames.
          A value of 0 means unlimited.
          Use this in combination with --max-sheet-size to precisely control the size of sheets. [default: 0]
      --layout-mode <LAYOUT_MODE>
          The sheet layout mode to use.
          This affects how many rows and columns are used to arrange the sprites on the sheet.
          "square" tries to arrange the sprites in a way that the resulting sheet is as square as possible.
          "fill-row" maximizes the number of columns while "fill-column" maximizes the number of rows. [default: square] [possible values: square, fill-row, fill-column]
```

### Icon

```
~$ spritter help icon
Generate a mipmap icon from a folder of images.

The individual images are used as the respective mip levels and combined into a single image.

Usage: spritter icon [OPTIONS] <SOURCE> <OUTPUT>

Arguments:
  <SOURCE>
          Folder containing the individual sprites

  <OUTPUT>
          Output folder

Options:
  -l, --lua
          Enable lua output generation

  -j, --json
          Enable json output generation

  -p, --prefix <PREFIX>
          Prefix to add to the output file name
          
          [default: ]

      --lossy
          Allow lossy compression for the output images. This is using pngquant / imagequant internally
```

### Gif
```
~$ spritter help gif
Generate a gif from a folder of images.

Note: Don't use gifs for in-game graphics. This is meant for documentation / preview purposes only.

Usage: spritter gif [OPTIONS] <SOURCE> <OUTPUT>

Arguments:
  <SOURCE>
          Folder containing the individual sprites

  <OUTPUT>
          Output folder

Options:
  -p, --prefix <PREFIX>
          Prefix to add to the output file name
          
          [default: ]

      --lossy
          Allow lossy compression for the output images. This is using pngquant / imagequant internally

  -s, --animation-speed <ANIMATION_SPEED>
          Animation speed to use for the gif.
          This is identical to in-game speed. 1.0 means 60 frames per second.
          Note: GIFs frame delay is in steps of 10ms, so the actual speed might be slightly different.
          
          [default: 1.0]

  -a, --alpha-threshold <ALPHA_THRESHOLD>
          Alpha threshold to consider a pixel as transparent [0-255].
          Since GIFS only support 1-bit transparency, this is used to determine which pixels are transparent.
          
          [default: 0]
```

### Optimize
```
~$ spritter help optimize
Optimize an image or a folder of images.

This is using oxipng (and optionally pngquant / imagequant when lossy is enabled). Note: the original images will be replaced with the optimized versions.

Usage: spritter optimize [OPTIONS] <TARGET>

Arguments:
  <TARGET>

Options:
  -r, --recursive
          Recursively search for images in the target folder

  -g, --group
          Treat images as a group and optimize them together instead of individually.
          This only has an effect with lossy compression.

      --lossy
          Allow lossy compression
```

### Split

```
~$ spritter help split
Split a spritesheet into individual images

Usage: spritter split <SOURCE> <WIDTH> <HEIGHT> <OUTPUT>

Arguments:
  <SOURCE>  The spritesheet to split into individual frames
  <WIDTH>   Number of frames horizontally
  <HEIGHT>  Number of frames vertically
  <OUTPUT>  Output folder
```
