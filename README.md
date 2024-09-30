# g12_tablet_linux_driver
Actual for SB20240929/V1G0 DEXP Ombra M Driver

## Common info
For start this builded driver run into shell with default preset settings:
```
exec ./tablet_driver
```
For start this builded driver run into shell with full path to preset file to pickup preferences of tablet:
```
exec ./tablet_driver preset="/full/path/to/preset/file/anyname.preset"
```
But remember - this driver locked main thread of terminal - while you dont call Ctrl+C. Use & UNIX return caller (see BASH docs) or startup scripts.
## Common syntax of *.preset file
Lets check simple *.preset format file
```
swap=false
inverse=true;true
sensivity=256
keybinds=KEY_L1:KEY_LEFTCTRL+@asRel_REL_WHEEL@ADD;KEY_R1:KEY_LEFTCTRL+@asRel_REL_WHEEL@REM;KEY_L6:BTN_MIDDLE
penbinds=VPEN_PLUS:KEY_LEFTCTRL+KEY_Z;VPEN_MINUS:KEY_LEFTSHIFT
```
_swap=false_ --- change (true) order of XY axis or no (false).

_inverse=true;true_ --- inverse (from RTL or LTR direction of axis) first boolean for X axis, second boolean for Y axis.

_sensivity_ --- slice of readed sensivity, check formula isong in _main.rs_.

_keybinds= ..._ --- enumerate of 12 buttons key alias. Supported any length alias combination. Common syntax for button:
```
KEY_L1:_KEY_ALIAS_NAME_1_+_KEY_ALIAS_NAME_2_+...+_KEY_ALIAS_NAME_N_;... (from evdev alias source code)
```
You can set _KEY_L1_, _KEY_L2_, ..., _KEY_L6_ and _KEY_R1_, _KEY_R2_, ..., _KEY_R6_, _VPEN_PLUS_, _VPEN_MINUS_ if you want using it.
If you can want use Mouse Wheel, you must call annotation *@asRel_* for use enable REL_BEHAVIOUR (see evdev avails REL ALIAS from source code), and add *@ADD* or *@REM* annotation to increase or decrease value (for example Mouse Wheel value by +/-1).
