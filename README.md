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
  spritesheet  Generate sprite sheets from a folder of images
  icon         Generate a mipmap icon from a folder of images
  help         Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Spritesheet

```
~$ spritter help spritesheet
Generate sprite sheets from a folder of images

Usage: spritter spritesheet [OPTIONS] <SOURCE> <OUTPUT>

Arguments:
  <SOURCE>  Folder containing the individual sprites
  <OUTPUT>  Output folder

Options:
  -l, --lua
          Enable lua output generation
  -p, --prefix <PREFIX>
          Prefix to add to the output file name [default: ]
  -r, --recursive
          Recursive search for images. Each folder will be a separate sprite sheet
  -t, --tile-resolution <TILE_RESOLUTION>
          Resolution in pixel per tile [default: 32]
      --hr
          Set when this is considered a high resolution texture
      --no-crop
          Set when the sprites should not be cropped
  -s, --scale <SCALE>
          Set a scaling factor to rescale the used sprites by.
          Values < 1.0 will shrink the sprites. Values > 1.0 will enlarge them. [default: 1]
      --scale-filter <SCALE_FILTER>
          The scaling filter to use when scaling sprites [default: catmull-rom] [possible values: nearest, triangle, catmull-rom, gaussian, lanczos3]
```

### Icon

```
~$ spritter help icon
Generate a mipmap icon from a folder of images

The individual images are used as the respective mip levels and combined into a single image

Usage: spritter icon [OPTIONS] <SOURCE> <OUTPUT>

Arguments:
  <SOURCE> Folder containing the individual sprites
  <OUTPUT> Output folder

Options:
  -l, --lua
          Enable lua output generation
  -p, --prefix <PREFIX>
          Prefix to add to the output file name
```
