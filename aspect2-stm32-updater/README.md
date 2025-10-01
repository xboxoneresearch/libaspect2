# ASPECT2 stm32 firmware updater

Updater for the STM32 chip used on the [ASPECT2-PCB](https://github.com/XboxOneResearch/ASPECT2-PCB) for handling POST-codes.

## Usage

```
Usage: aspect2-stm32-updater <COMMAND>

Commands:
  flash  Flash STM32 chip
  info   Retrieve metadata of currently flashed firmware components
  wipe   Wipe the whole flash memory
  help   Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

Get info

```
aspect2-stm32-updater info
```

Flash preloader firmware

```
aspect2-stm32-updater flash preloader preloader.bin
```

Flash userapp firmware

```
aspect2-stm32-updater flash user-app userapp.bin
```

Wipe whole flash
(This will FORCE the STM32 into Bootloader-mode)

```
aspect2-stm32-updater wipe
```

## Build

Requires installed rust toolchain.

```
cargo build --release
```