MEMORY
{
  /* build-raytac.sh supplies a start matching the installed SoftDevice.
     The original Holyiot build keeps its existing 0x26000 start. */
  FLASH : ORIGIN = DEFINED(COREX_RAYTAC_FLASH_BASE) ? COREX_RAYTAC_FLASH_BASE : 0x26000,
          LENGTH = 0xB0000 - (DEFINED(COREX_RAYTAC_FLASH_BASE) ? COREX_RAYTAC_FLASH_BASE : 0x26000)
  RAM : ORIGIN = 0x20000008, LENGTH = 255K
}
