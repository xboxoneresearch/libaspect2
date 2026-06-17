# libaspect2

## ASPECT2 stm32 firmware updater

To update the firmware of the STM32 of the [ASPECT2-PCB](https://github.com/XboxOneResearch/ASPECT2-PCB), check out [aspect2-stm32-updater](./aspect2-stm32-updater/).

## Usage of CLI tools

### eMMC

```
flash_cli emmc info
flash_cli emmc --freq 25.0 read dump.bin 0x0 0x10000
flash_cli emmc write image.bin
flash_cli emmc dump-fuses
flash_cli emmc reset
```

### SPI NOR

```
flash_cli nor info
flash_cli nor --spi-clock 5000 read flash.bin
flash_cli nor read flash.bin 0x10000 0x8000   # offset + length
flash_cli nor write firmware.bin              # erase then program
flash_cli nor erase 0x0 0x10000              # sector-by-sector
flash_cli nor chip-erase
```

# Different FTDI device

```
flash_cli --device "My Board B" nor info
```
