#!/bin/sh
# Installs the udev rules the game needs for a Mayflash DolphinBar (mode 4), replacing any old ones:
#  - its four hidraw nodes, so the game can talk to the remotes (for the logged-in user);
#  - its USB port's `disable` switch, so the game can power-cycle the bar to turn idle remotes off
#    (any local user can then switch that port).
set -eu

RULES=/etc/udev/rules.d/60-dolphinbar.rules

sudo tee "$RULES" >/dev/null <<'EOF'
KERNEL=="hidraw*", ATTRS{idVendor}=="057e", ATTRS{idProduct}=="0306", MODE="0660", TAG+="uaccess"
ACTION=="add", SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="057e", ATTR{idProduct}=="0306", RUN+="/bin/chmod 0666 /sys%p/port/disable"
EOF
sudo udevadm control --reload
sudo udevadm trigger --action=add --subsystem-match=usb --subsystem-match=hidraw
sudo udevadm settle
echo "Installed $RULES:"
cat "$RULES"
