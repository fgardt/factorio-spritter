[![release](https://github.com/fgardt/factorio-spritter/actions/workflows/release.yml/badge.svg)](https://github.com/fgardt/factorio-spritter/releases)

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
  -r, --recursive
          Recursive search for images. Each folder will be a separate sprite sheet
  -t, --tile-resolution <TILE_RESOLUTION>
          Resolution in pixel per tile [default: 32]
      --hr
          Set when this is considered a high resolution texture
      --no-crop
          Set when the sprites should not be cropped
```

### Icon

```
~$ spritter help icon
Generate a mipmap icon from a folder of images

The individual images are used as the respective mip levels and combined into a single image

Usage: spritter icon <SOURCE> <OUTPUT>

Arguments:
  <SOURCE>
          Folder containing the individual sprites

  <OUTPUT>
          Output folder
```

## Todo

- Lua output generation
